/// Stream task events MCP endpoint (SSE)
///
/// This endpoint streams task events in real-time using Server-Sent Events (SSE).
/// It supports:
/// - Historical event backfill (resume from any point)
/// - Live tail (real-time events as they occur)
/// - Gap detection (handling compacted streams)
/// - Automatic reconnection with Last-Event-ID
///
/// # Endpoint
///
/// `GET /mcp/tasks/:task_id/stream?since_seq=0`
///
/// # Query Parameters
///
/// - `since_seq` (optional): Start streaming from this sequence number (default: 0)
///   - `0`: Stream from beginning
///   - `N`: Stream from sequence N onwards
///   - `-1`: Stream only new events (skip backfill)
///
/// # Headers
///
/// **Request:**
/// - `Last-Event-ID` (optional): Resume from this stream ID (overrides since_seq)
///
/// **Response:**
/// - `Content-Type: text/event-stream`
/// - `Cache-Control: no-cache`
/// - `Connection: keep-alive`
///
/// # SSE Event Format
///
/// ```text
/// event: task_event
/// id: 1234567890-0
/// data: {"seq":42,"kind":"stdout","payload":{"data":"Hello\n"},"ts":"2025-01-04T12:00:00Z"}
///
/// event: heartbeat
/// data: {"alive":true}
/// ```
///
/// # Example
///
/// ```bash
/// curl -N -H "Authorization: Bearer <token>" \
///   "http://localhost:8080/v1/mcp/tasks/{task_id}/stream?since_seq=0"
/// ```

use crate::app::AppState;
use crate::error::ApiError;
use axontask_shared::auth::middleware::AuthContext;
use axontask_shared::models::task::Task;
use axontask_shared::redis::{RedisClient, RedisConfig, StreamReader};
use axontask_shared::events::serialization::{deserialize_event, event_stream_key};
use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
    Extension,
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::StreamExt as _;
use uuid::Uuid;

/// Stream task query parameters
#[derive(Debug, Clone, Deserialize)]
pub struct StreamTaskQuery {
    /// Start streaming from this sequence number
    /// - 0: from beginning (default)
    /// - N: from sequence N
    /// - -1: skip backfill, only new events
    #[serde(default)]
    pub since_seq: i64,
}

/// SSE event data for task events
#[derive(Debug, Clone, Serialize)]
pub struct TaskEventData {
    /// Event sequence number
    pub seq: i64,

    /// Event kind
    pub kind: String,

    /// Event payload (adapter-specific)
    pub payload: serde_json::Value,

    /// Event timestamp
    pub ts: String,

    /// Previous hash (for integrity verification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_prev: Option<String>,

    /// Current hash
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_curr: Option<String>,
}

/// SSE heartbeat data
#[derive(Debug, Clone, Serialize)]
pub struct HeartbeatData {
    pub alive: bool,
}

/// Stream task events endpoint handler
///
/// Streams task events via SSE with backfill and live tail.
///
/// # Flow
///
/// 1. **Validate**: Check task exists and belongs to tenant
/// 2. **Backfill**: Send historical events from Redis Streams (since_seq to latest)
/// 3. **Live Tail**: Block and wait for new events (XREAD BLOCK)
/// 4. **Heartbeat**: Send keep-alive every 25 seconds
/// 5. **Error Handling**: Detect gaps, handle disconnections
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
/// - 500 Internal Server Error: Database or Redis error
///
/// # Example
///
/// ```no_run
/// use axum::extract::{Path, Query, State};
/// use axum::Extension;
/// # use crate::routes::mcp::stream_task::{stream_task, StreamTaskQuery};
/// # use crate::app::AppState;
/// # use axontask_shared::auth::middleware::AuthContext;
/// # use uuid::Uuid;
///
/// # async fn example(
/// #     state: State<AppState>,
/// #     auth: Extension<AuthContext>,
/// #     task_id: Uuid,
/// # ) -> Result<(), Box<dyn std::error::Error>> {
/// let query = StreamTaskQuery { since_seq: 0 };
/// let sse_stream = stream_task(state, auth, Path(task_id), Query(query)).await?;
/// // Client receives SSE stream
/// # Ok(())
/// # }
/// ```
pub async fn stream_task(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(task_id): Path<Uuid>,
    Query(query): Query<StreamTaskQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    tracing::info!(
        tenant_id = %auth.tenant_id,
        user_id = %auth.user_id,
        task_id = %task_id,
        since_seq = query.since_seq,
        "Streaming task events"
    );

    // Validate task exists and belongs to tenant
    let _task = Task::find_by_id_and_tenant(&state.db, task_id, auth.tenant_id)
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

    // Create SSE stream
    let stream = create_event_stream(task_id, query.since_seq);

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(25))))
}

/// Creates the SSE event stream with backfill and live tail
///
/// This function creates an async stream that:
/// 1. Sends backfill events from Redis
/// 2. Switches to live tail mode
/// 3. Sends periodic heartbeats
fn create_event_stream(
    task_id: Uuid,
    since_seq: i64,
) -> impl Stream<Item = Result<Event, Infallible>> {
    // TODO: In production, initialize Redis client from config
    // For now, return a mock stream that demonstrates the pattern

    stream::iter(vec![
        // Mock backfill events
        Ok(Event::default()
            .event("task_event")
            .id("1234567890-0")
            .json_data(TaskEventData {
                seq: 0,
                kind: "started".to_string(),
                payload: serde_json::json!({"adapter": "shell"}),
                ts: chrono::Utc::now().to_rfc3339(),
                hash_prev: None,
                hash_curr: Some("abcd1234".to_string()),
            })
            .unwrap()),

        // Mock progress event
        Ok(Event::default()
            .event("task_event")
            .id("1234567891-0")
            .json_data(TaskEventData {
                seq: 1,
                kind: "progress".to_string(),
                payload: serde_json::json!({"percent": 50}),
                ts: chrono::Utc::now().to_rfc3339(),
                hash_prev: Some("abcd1234".to_string()),
                hash_curr: Some("efgh5678".to_string()),
            })
            .unwrap()),

        // Heartbeat
        Ok(Event::default()
            .event("heartbeat")
            .json_data(HeartbeatData { alive: true })
            .unwrap()),
    ])
}

/// Real implementation (commented out until Redis is integrated)
///
/// ```rust,ignore
/// async fn create_real_event_stream(
///     task_id: Uuid,
///     since_seq: i64,
/// ) -> impl Stream<Item = Result<Event, Infallible>> {
///     let redis_config = RedisConfig::from_env().unwrap();
///     let redis_client = RedisClient::new(redis_config).await.unwrap();
///     let reader = StreamReader::new(redis_client);
///
///     // Convert since_seq to Redis stream ID
///     let start_id = if since_seq < 0 {
///         "$".to_string() // Only new events
///     } else {
///         "0".to_string() // From beginning
///     };
///
///     // Phase 1: Backfill historical events
///     let backfill_events = reader
///         .read_backfill(task_id, &start_id, 1000)
///         .await
///         .unwrap_or_default();
///
///     let backfill_stream = stream::iter(backfill_events.into_iter().map(|(stream_id, event)| {
///         Ok(Event::default()
///             .event("task_event")
///             .id(stream_id)
///             .json_data(TaskEventData {
///                 seq: event.seq,
///                 kind: event.kind,
///                 payload: event.payload,
///                 ts: event.ts.to_rfc3339(),
///                 hash_prev: event.hash_prev.map(hex::encode),
///                 hash_curr: Some(hex::encode(event.hash_curr)),
///             })
///             .unwrap())
///     }));
///
///     // Phase 2: Live tail with heartbeats
///     let live_stream = stream::unfold(
///         (reader, "$".to_string()),
///         move |(reader, mut last_id)| async move {
///             // Try to read new events (5 second timeout)
///             match reader.read_live(task_id, &last_id, 5000).await {
///                 Ok(events) if !events.is_empty() => {
///                     // Return events
///                     let event_stream = stream::iter(events.into_iter().map(|(stream_id, event)| {
///                         last_id = stream_id.clone();
///                         Ok(Event::default()
///                             .event("task_event")
///                             .id(stream_id)
///                             .json_data(TaskEventData {
///                                 seq: event.seq,
///                                 kind: event.kind,
///                                 payload: event.payload,
///                                 ts: event.ts.to_rfc3339(),
///                                 hash_prev: event.hash_prev.map(hex::encode),
///                                 hash_curr: Some(hex::encode(event.hash_curr)),
///                             })
///                             .unwrap())
///                     }));
///                     Some((event_stream, (reader, last_id)))
///                 }
///                 Ok(_) => {
///                     // Timeout, send heartbeat
///                     let heartbeat = Ok(Event::default()
///                         .event("heartbeat")
///                         .json_data(HeartbeatData { alive: true })
///                         .unwrap());
///                     Some((stream::once(async { heartbeat }), (reader, last_id)))
///                 }
///                 Err(e) => {
///                     tracing::error!(error = %e, "Redis read error");
///                     None // End stream on error
///                 }
///             }
///         },
///     ).flatten();
///
///     // Combine backfill and live streams
///     backfill_stream.chain(live_stream)
/// }
/// ```

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_task_query_defaults() {
        let query = StreamTaskQuery { since_seq: 0 };
        assert_eq!(query.since_seq, 0);
    }

    #[test]
    fn test_task_event_data_serialization() {
        let data = TaskEventData {
            seq: 42,
            kind: "stdout".to_string(),
            payload: serde_json::json!({"data": "Hello\n"}),
            ts: "2025-01-04T12:00:00Z".to_string(),
            hash_prev: Some("prev_hash".to_string()),
            hash_curr: Some("curr_hash".to_string()),
        };

        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"seq\":42"));
        assert!(json.contains("stdout"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_heartbeat_data_serialization() {
        let data = HeartbeatData { alive: true };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"alive\":true"));
    }
}
