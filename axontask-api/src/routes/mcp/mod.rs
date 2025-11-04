/// MCP (Model Context Protocol) tool endpoints
///
/// This module provides endpoints for AI agents to start, monitor, and control
/// long-running background tasks with persistent streaming.
///
/// # Endpoints
///
/// - `POST /mcp/start_task` - Start a new task
/// - `GET /mcp/tasks/:id/status` - Get task status
/// - `GET /mcp/tasks/:id/stream` - Stream task events (SSE)
/// - `POST /mcp/tasks/:id/cancel` - Cancel a running task
/// - `POST /mcp/tasks/:id/resume` - Resume event streaming
///
/// # Authentication
///
/// All endpoints require either:
/// - JWT token: `Authorization: Bearer <token>`
/// - API key: `X-Api-Key: <key>`
///
/// # Rate Limiting
///
/// Endpoints are rate-limited based on tenant plan:
/// - Trial: 10 requests/minute
/// - Entry: 60 requests/minute
/// - Pro: 300 requests/minute
/// - Enterprise: Custom
///
/// # Example Usage
///
/// ```no_run
/// // Start a task
/// POST /mcp/start_task
/// {
///   "name": "deploy-app",
///   "adapter": "fly",
///   "args": {"app": "myapp"},
///   "timeout_s": 900
/// }
///
/// // Stream events
/// GET /mcp/tasks/{task_id}/stream
/// → SSE stream of events
///
/// // Get status
/// GET /mcp/tasks/{task_id}/status
/// → {"state": "running", "started_at": "...", "last_seq": 42}
///
/// // Cancel task
/// POST /mcp/tasks/{task_id}/cancel
/// → {"canceled": true}
/// ```

pub mod cancel_task;
pub mod get_status;
pub mod resume_task;
pub mod start_task;
pub mod stream_task;

// Re-export handlers for convenience
pub use cancel_task::{cancel_task, CancelTaskResponse};
pub use get_status::{get_task_status, TaskStatusResponse};
pub use resume_task::{resume_task, ResumeTaskRequest};
pub use start_task::{start_task, StartTaskRequest, StartTaskResponse};
pub use stream_task::{stream_task, StreamTaskQuery};
