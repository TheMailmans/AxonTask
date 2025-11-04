/// Start task MCP endpoint
///
/// This endpoint allows clients to start a new background task.
/// The task is created in the database with "pending" state and
/// can be picked up by workers.
///
/// # Endpoint
///
/// `POST /mcp/start_task`
///
/// # Authentication
///
/// Requires either:
/// - JWT token (Authorization: Bearer <token>)
/// - API key (X-Api-Key: <key>)
///
/// # Example Request
///
/// ```json
/// {
///   "name": "deploy-app",
///   "adapter": "fly",
///   "args": {
///     "app": "myapp",
///     "region": "iad"
///   },
///   "timeout_s": 900
/// }
/// ```
///
/// # Example Response
///
/// ```json
/// {
///   "task_id": "550e8400-e29b-41d4-a716-446655440000",
///   "stream_url": "/mcp/tasks/550e8400-e29b-41d4-a716-446655440000/stream",
///   "status": "pending",
///   "created_at": "2025-01-04T12:00:00Z"
/// }
/// ```

use crate::app::AppState;
use crate::error::ApiError;
use axontask_shared::auth::middleware::AuthContext;
use axontask_shared::models::task::{CreateTask, Task, TaskState};
use axum::{extract::State, Extension, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;
use validator::Validate;

/// Start task request
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct StartTaskRequest {
    /// Task name (for display/logging)
    #[validate(length(min = 1, max = 255))]
    pub name: String,

    /// Adapter type (shell, docker, fly, etc.)
    #[validate(length(min = 1, max = 50))]
    pub adapter: String,

    /// Adapter-specific arguments (JSON)
    pub args: JsonValue,

    /// Optional timeout in seconds (default: 3600 = 1 hour)
    #[validate(range(min = 1, max = 86400))] // Max 24 hours
    pub timeout_s: Option<i32>,

    /// Optional tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Start task response
#[derive(Debug, Clone, Serialize)]
pub struct StartTaskResponse {
    /// Created task ID
    pub task_id: Uuid,

    /// URL to stream events (relative path)
    pub stream_url: String,

    /// Initial task status
    pub status: String,

    /// Task creation timestamp
    pub created_at: DateTime<Utc>,

    /// Estimated timeout (if specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_s: Option<i32>,
}

/// Start task endpoint handler
///
/// Creates a new task in the database with "pending" state.
/// The task can then be picked up by a worker for execution.
///
/// # Authentication
///
/// Requires valid JWT token or API key with `tasks:write` scope.
///
/// # Tenant Isolation
///
/// Tasks are automatically associated with the authenticated user's tenant.
///
/// # Validation
///
/// - name: 1-255 characters
/// - adapter: 1-50 characters
/// - timeout_s: 1-86400 seconds (1 second to 24 hours)
/// - args: Valid JSON
///
/// # Errors
///
/// - 400 Bad Request: Invalid input
/// - 401 Unauthorized: Missing or invalid authentication
/// - 422 Unprocessable Entity: Validation errors
/// - 500 Internal Server Error: Database error
///
/// # Example
///
/// ```no_run
/// use axum::extract::State;
/// use axum::Extension;
/// use axum::Json;
/// # use crate::routes::mcp::start_task::{start_task, StartTaskRequest};
/// # use crate::app::AppState;
/// # use axontask_shared::auth::middleware::AuthContext;
/// # use serde_json::json;
/// # use uuid::Uuid;
///
/// # async fn example(
/// #     state: State<AppState>,
/// #     auth: Extension<AuthContext>,
/// # ) -> Result<(), Box<dyn std::error::Error>> {
/// let request = StartTaskRequest {
///     name: "deploy-app".to_string(),
///     adapter: "fly".to_string(),
///     args: json!({"app": "myapp"}),
///     timeout_s: Some(900),
///     tags: vec!["deployment".to_string()],
/// };
///
/// let response = start_task(state, auth, Json(request)).await?;
/// println!("Task created: {}", response.task_id);
/// # Ok(())
/// # }
/// ```
pub async fn start_task(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(request): Json<StartTaskRequest>,
) -> Result<Json<StartTaskResponse>, ApiError> {
    // Validate request
    request
        .validate()
        .map_err(|e| ApiError::ValidationError(e.into()))?;

    tracing::info!(
        tenant_id = %auth.tenant_id,
        user_id = %auth.user_id,
        task_name = %request.name,
        adapter = %request.adapter,
        "Creating new task"
    );

    // TODO: Validate adapter type against whitelist
    // TODO: Check tenant quotas (concurrent tasks, daily tasks)
    // TODO: Check plan-specific adapter access

    // Create task in database
    let create_task = CreateTask {
        tenant_id: auth.tenant_id,
        name: request.name.clone(),
        adapter: request.adapter.clone(),
        args: request.args.clone(),
        timeout_seconds: request.timeout_s,
        tags: if request.tags.is_empty() {
            None
        } else {
            Some(request.tags.clone())
        },
    };

    let task = Task::create(&state.db, create_task).await.map_err(|e| {
        tracing::error!(error = %e, "Failed to create task in database");
        ApiError::InternalError("Failed to create task".to_string())
    })?;

    tracing::info!(
        task_id = %task.id,
        tenant_id = %auth.tenant_id,
        state = ?task.state,
        "Task created successfully"
    );

    // TODO: Enqueue task to worker queue (Redis list or pub/sub)
    // For now, workers will poll the database for pending tasks

    // Build response
    let response = StartTaskResponse {
        task_id: task.id,
        stream_url: format!("/mcp/tasks/{}/stream", task.id),
        status: task.state.as_str().to_string(),
        created_at: task.created_at,
        timeout_s: task.timeout_seconds,
    };

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_start_task_request_validation() {
        // Valid request
        let valid = StartTaskRequest {
            name: "test-task".to_string(),
            adapter: "shell".to_string(),
            args: json!({"command": "echo hello"}),
            timeout_s: Some(300),
            tags: vec!["test".to_string()],
        };
        assert!(valid.validate().is_ok());

        // Empty name
        let invalid_name = StartTaskRequest {
            name: "".to_string(),
            adapter: "shell".to_string(),
            args: json!({}),
            timeout_s: None,
            tags: vec![],
        };
        assert!(invalid_name.validate().is_err());

        // Name too long
        let long_name = StartTaskRequest {
            name: "a".repeat(256),
            adapter: "shell".to_string(),
            args: json!({}),
            timeout_s: None,
            tags: vec![],
        };
        assert!(long_name.validate().is_err());

        // Invalid timeout (too small)
        let invalid_timeout = StartTaskRequest {
            name: "test".to_string(),
            adapter: "shell".to_string(),
            args: json!({}),
            timeout_s: Some(0),
            tags: vec![],
        };
        assert!(invalid_timeout.validate().is_err());

        // Invalid timeout (too large)
        let invalid_timeout_large = StartTaskRequest {
            name: "test".to_string(),
            adapter: "shell".to_string(),
            args: json!({}),
            timeout_s: Some(100000),
            tags: vec![],
        };
        assert!(invalid_timeout_large.validate().is_err());
    }

    #[test]
    fn test_start_task_response_serialization() {
        let response = StartTaskResponse {
            task_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            stream_url: "/mcp/tasks/550e8400-e29b-41d4-a716-446655440000/stream".to_string(),
            status: "pending".to_string(),
            created_at: Utc::now(),
            timeout_s: Some(900),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("task_id"));
        assert!(json.contains("stream_url"));
        assert!(json.contains("pending"));
    }
}
