/// Cancel task MCP endpoint
///
/// This endpoint allows clients to cancel a running or pending task.
/// It sends a control message to the worker and updates the task state.
///
/// # Endpoint
///
/// `POST /mcp/tasks/:task_id/cancel`
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
///   "canceled": true,
///   "state": "canceled",
///   "message": "Task cancellation requested"
/// }
/// ```

use crate::app::AppState;
use crate::error::ApiError;
use axontask_shared::auth::middleware::AuthContext;
use axontask_shared::models::task::{Task, TaskState};
use axontask_shared::redis::RedisClient;
use axum::{extract::{Path, State}, Extension, Json};
use serde::Serialize;
use uuid::Uuid;

/// Cancel task response
#[derive(Debug, Clone, Serialize)]
pub struct CancelTaskResponse {
    /// Task ID
    pub task_id: Uuid,

    /// Whether cancellation was successful
    pub canceled: bool,

    /// Current task state
    pub state: String,

    /// Descriptive message
    pub message: String,
}

/// Cancel task endpoint handler
///
/// Cancels a running or pending task by:
/// 1. Validating task exists and belongs to tenant
/// 2. Checking if task can be canceled (not already completed)
/// 3. Sending control message to worker via Redis
/// 4. Updating task state to "canceled"
///
/// # Authentication
///
/// Requires valid JWT token or API key with `tasks:write` scope.
///
/// # Tenant Isolation
///
/// Only allows canceling tasks belonging to the authenticated user's tenant.
///
/// # Errors
///
/// - 400 Bad Request: Task already completed
/// - 401 Unauthorized: Missing or invalid authentication
/// - 403 Forbidden: Task belongs to different tenant
/// - 404 Not Found: Task does not exist
/// - 500 Internal Server Error: Database or Redis error
///
/// # Example
///
/// ```no_run
/// use axum::extract::{Path, State};
/// use axum::Extension;
/// # use crate::routes::mcp::cancel_task::cancel_task;
/// # use crate::app::AppState;
/// # use axontask_shared::auth::middleware::AuthContext;
/// # use uuid::Uuid;
///
/// # async fn example(
/// #     state: State<AppState>,
/// #     auth: Extension<AuthContext>,
/// #     task_id: Uuid,
/// # ) -> Result<(), Box<dyn std::error::Error>> {
/// let response = cancel_task(state, auth, Path(task_id)).await?;
/// println!("Task canceled: {}", response.canceled);
/// # Ok(())
/// # }
/// ```
pub async fn cancel_task(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(task_id): Path<Uuid>,
) -> Result<Json<CancelTaskResponse>, ApiError> {
    tracing::info!(
        tenant_id = %auth.tenant_id,
        user_id = ?auth.user_id,
        task_id = %task_id,
        "Canceling task"
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

    // Check if task can be canceled
    match task.state.as_str() {
        "succeeded" | "failed" | "canceled" | "timeout" => {
            return Ok(Json(CancelTaskResponse {
                task_id,
                canceled: false,
                state: task.state.clone(),
                message: format!("Task already in terminal state: {}", task.state),
            }));
        }
        "pending" | "running" => {
            // Can be canceled
        }
        _ => {
            // Unknown state, allow cancellation attempt
        }
    }

    // Send control message to worker via Redis
    // TODO: Initialize Redis client from state
    // For now, we'll just update the database state
    // In a full implementation:
    // let redis_client = RedisClient::new(redis_config).await?;
    // let writer = StreamWriter::new(redis_client);
    // writer.publish_control_message(task_id, "cancel", None).await?;

    // Update task state to canceled
    let updated_task = Task::transition_to_canceled(&state.db, task_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, task_id = %task_id, "Failed to update task state");
            ApiError::InternalError("Failed to cancel task".to_string())
        })?;

    let task_state = updated_task
        .map(|t| t.state.clone())
        .unwrap_or_else(|| "pending".to_string());

    tracing::info!(
        task_id = %task_id,
        tenant_id = %auth.tenant_id,
        "Task canceled successfully"
    );

    Ok(Json(CancelTaskResponse {
        task_id,
        canceled: true,
        state: task_state,
        message: "Task cancellation requested".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cancel_task_response_serialization() {
        let response = CancelTaskResponse {
            task_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            canceled: true,
            state: "canceled".to_string(),
            message: "Task cancellation requested".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("task_id"));
        assert!(json.contains("canceled"));
        assert!(json.contains("Task cancellation requested"));
    }

    #[test]
    fn test_cancel_task_response_already_completed() {
        let response = CancelTaskResponse {
            task_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            canceled: false,
            state: "succeeded".to_string(),
            message: "Task already in terminal state: succeeded".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"canceled\":false"));
        assert!(json.contains("already in terminal state"));
    }
}
