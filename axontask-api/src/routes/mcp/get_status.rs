/// Get task status MCP endpoint
///
/// This endpoint returns the current status of a task including:
/// - Current state (pending, running, succeeded, failed, etc.)
/// - Timestamps (created, started, ended)
/// - Last event sequence number
/// - Resource usage metrics
///
/// # Endpoint
///
/// `GET /mcp/tasks/:task_id/status`
///
/// # Authentication
///
/// Requires either:
/// - JWT token (Authorization: Bearer <token>)
/// - API key (X-Api-Key: <key>)
///
/// # Example Response
///
/// ```json
/// {
///   "task_id": "550e8400-e29b-41d4-a716-446655440000",
///   "name": "deploy-app",
///   "state": "running",
///   "created_at": "2025-01-04T12:00:00Z",
///   "started_at": "2025-01-04T12:00:05Z",
///   "ended_at": null,
///   "last_seq": 42,
///   "metrics": {
///     "duration_ms": 5000,
///     "bytes_streamed": 1024
///   }
/// }
/// ```

use crate::app::AppState;
use crate::error::ApiError;
use axontask_shared::auth::middleware::AuthContext;
use axontask_shared::models::task::Task;
use axum::{extract::{Path, State}, Extension, Json};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Task status response
#[derive(Debug, Clone, Serialize)]
pub struct TaskStatusResponse {
    /// Task ID
    pub task_id: Uuid,

    /// Task name
    pub name: String,

    /// Current state
    pub state: String,

    /// When task was created
    pub created_at: DateTime<Utc>,

    /// When task started executing (if started)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,

    /// When task completed (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,

    /// Last event sequence number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seq: Option<i64>,

    /// Resource usage metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<TaskMetrics>,

    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Task resource usage metrics
#[derive(Debug, Clone, Serialize)]
pub struct TaskMetrics {
    /// Total execution duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,

    /// Total bytes streamed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_streamed: Option<i64>,

    /// Task-minutes consumed (for billing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_minutes: Option<f64>,
}

/// Get task status endpoint handler
///
/// Returns the current status of a task with tenant isolation.
///
/// # Authentication
///
/// Requires valid JWT token or API key with `tasks:read` scope.
///
/// # Tenant Isolation
///
/// Only returns tasks belonging to the authenticated user's tenant.
///
/// # Errors
///
/// - 401 Unauthorized: Missing or invalid authentication
/// - 403 Forbidden: Task belongs to different tenant
/// - 404 Not Found: Task does not exist
/// - 500 Internal Server Error: Database error
///
/// # Example
///
/// ```no_run
/// use axum::extract::{Path, State};
/// use axum::Extension;
/// # use crate::routes::mcp::get_status::get_task_status;
/// # use crate::app::AppState;
/// # use axontask_shared::auth::middleware::AuthContext;
/// # use uuid::Uuid;
///
/// # async fn example(
/// #     state: State<AppState>,
/// #     auth: Extension<AuthContext>,
/// #     task_id: Uuid,
/// # ) -> Result<(), Box<dyn std::error::Error>> {
/// let response = get_task_status(state, auth, Path(task_id)).await?;
/// println!("Task state: {}", response.state);
/// # Ok(())
/// # }
/// ```
pub async fn get_task_status(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(task_id): Path<Uuid>,
) -> Result<Json<TaskStatusResponse>, ApiError> {
    tracing::debug!(
        tenant_id = %auth.tenant_id,
        user_id = ?auth.user_id,
        task_id = %task_id,
        "Getting task status"
    );

    // Find task with tenant isolation
    let task = Task::find_by_id_and_tenant(&state.db, task_id, auth.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, task_id = %task_id, "Failed to query task");
            ApiError::InternalError("Failed to query task".to_string())
        })?
        .ok_or_else(|| {
            tracing::warn!(
                task_id = %task_id,
                tenant_id = %auth.tenant_id,
                "Task not found or access denied"
            );
            ApiError::NotFound("Task not found".to_string())
        })?;

    // Calculate duration if task has started
    let duration_ms = if let (Some(started), Some(ended)) = (task.started_at, task.ended_at) {
        Some((ended - started).num_milliseconds())
    } else if let Some(started) = task.started_at {
        // Task is still running
        Some((Utc::now() - started).num_milliseconds())
    } else {
        None
    };

    // Calculate task-minutes (for billing)
    let task_minutes = duration_ms.map(|ms| ms as f64 / 60000.0);

    // Build metrics
    let metrics = if duration_ms.is_some() || task.bytes_streamed > 0 {
        Some(TaskMetrics {
            duration_ms,
            bytes_streamed: Some(task.bytes_streamed),
            task_minutes,
        })
    } else {
        None
    };

    // Build response
    let response = TaskStatusResponse {
        task_id: task.id,
        name: task.name,
        state: task.state.as_str().to_string(),
        created_at: task.created_at,
        started_at: task.started_at,
        ended_at: task.ended_at,
        last_seq: Some(task.cursor),
        metrics,
        error: task.error_message,
    };

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axontask_shared::models::task::TaskState;

    #[test]
    fn test_task_status_response_serialization() {
        let response = TaskStatusResponse {
            task_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            name: "test-task".to_string(),
            state: TaskState::Running.as_str().to_string(),
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            ended_at: None,
            last_seq: Some(42),
            metrics: Some(TaskMetrics {
                duration_ms: Some(5000),
                bytes_streamed: Some(1024),
                task_minutes: Some(0.083),
            }),
            error: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("task_id"));
        assert!(json.contains("running"));
        assert!(json.contains("last_seq"));
        assert!(json.contains("duration_ms"));
    }

    #[test]
    fn test_task_status_response_with_error() {
        let response = TaskStatusResponse {
            task_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            name: "failed-task".to_string(),
            state: TaskState::Failed.as_str().to_string(),
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
            last_seq: Some(10),
            metrics: Some(TaskMetrics {
                duration_ms: Some(2000),
                bytes_streamed: Some(512),
                task_minutes: Some(0.033),
            }),
            error: Some("Connection timeout".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("failed"));
        assert!(json.contains("Connection timeout"));
    }

    #[test]
    fn test_task_status_response_pending() {
        let response = TaskStatusResponse {
            task_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            name: "pending-task".to_string(),
            state: TaskState::Pending.as_str().to_string(),
            created_at: Utc::now(),
            started_at: None,
            ended_at: None,
            last_seq: None,
            metrics: None,
            error: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("pending"));
        assert!(!json.contains("started_at"));
        assert!(!json.contains("metrics"));
    }
}
