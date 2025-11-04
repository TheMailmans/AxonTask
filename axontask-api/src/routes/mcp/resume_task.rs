/// Resume task streaming MCP endpoint
///
/// This endpoint is an alias for stream_task with explicit resume semantics.
/// It allows clients to resume streaming from a specific sequence number.
///
/// # Endpoint
///
/// `POST /mcp/tasks/:task_id/resume`
///
/// # Request Body
///
/// ```json
/// {
///   "last_seq": 42
/// }
/// ```
///
/// # Authentication
///
/// Requires either:
/// - JWT token (Authorization: Bearer <token>)
/// - API key (X-Api-Key: <key>)
///
/// # Response
///
/// Returns SSE stream (same as stream_task endpoint).
///
/// # Difference from stream_task
///
/// - `stream_task`: GET request with query params (more RESTful)
/// - `resume_task`: POST request with JSON body (explicit resume action)
///
/// Both endpoints provide identical functionality.

use crate::app::AppState;
use crate::error::ApiError;
use crate::routes::mcp::stream_task::{stream_task, StreamTaskQuery};
use axontask_shared::auth::middleware::AuthContext;
use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, Sse},
    Extension, Json,
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use uuid::Uuid;

/// Resume task request
#[derive(Debug, Clone, Deserialize)]
pub struct ResumeTaskRequest {
    /// Last sequence number received by client
    ///
    /// Stream will resume from `last_seq + 1`.
    /// Use 0 to stream from beginning.
    /// Use -1 to skip backfill and only receive new events.
    pub last_seq: i64,
}

/// Resume task endpoint handler
///
/// Resumes streaming task events from a specific sequence number.
/// This is functionally identical to stream_task but uses POST
/// with JSON body for explicit resume semantics.
///
/// # Authentication
///
/// Requires valid JWT token or API key with `tasks:read` scope.
///
/// # Tenant Isolation
///
/// Only streams events for tasks belonging to the authenticated user's tenant.
///
/// # Errors
///
/// - 401 Unauthorized: Missing or invalid authentication
/// - 403 Forbidden: Task belongs to different tenant
/// - 404 Not Found: Task does not exist
/// - 422 Unprocessable Entity: Invalid request body
/// - 500 Internal Server Error: Database or Redis error
///
/// # Example
///
/// ```no_run
/// use axum::extract::{Path, State};
/// use axum::Extension;
/// use axum::Json;
/// # use crate::routes::mcp::resume_task::{resume_task, ResumeTaskRequest};
/// # use crate::app::AppState;
/// # use axontask_shared::auth::middleware::AuthContext;
/// # use uuid::Uuid;
///
/// # async fn example(
/// #     state: State<AppState>,
/// #     auth: Extension<AuthContext>,
/// #     task_id: Uuid,
/// # ) -> Result<(), Box<dyn std::error::Error>> {
/// let request = ResumeTaskRequest { last_seq: 42 };
/// let sse_stream = resume_task(state, auth, Path(task_id), Json(request)).await?;
/// // Client receives SSE stream from sequence 43 onwards
/// # Ok(())
/// # }
/// ```
pub async fn resume_task(
    state: State<AppState>,
    auth: Extension<AuthContext>,
    Path(task_id): Path<Uuid>,
    Json(request): Json<ResumeTaskRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    tracing::info!(
        tenant_id = %auth.tenant_id,
        user_id = ?auth.user_id,
        task_id = %task_id,
        last_seq = request.last_seq,
        "Resuming task stream"
    );

    // Convert to StreamTaskQuery and delegate to stream_task
    let query = Query(StreamTaskQuery {
        since_seq: request.last_seq + 1, // Resume from next sequence
    });

    stream_task(state, auth, Path(task_id), query).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resume_task_request_deserialization() {
        let json = r#"{"last_seq": 42}"#;
        let request: ResumeTaskRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.last_seq, 42);
    }

    #[test]
    fn test_resume_task_request_negative() {
        let json = r#"{"last_seq": -1}"#;
        let request: ResumeTaskRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.last_seq, -1);
    }
}
