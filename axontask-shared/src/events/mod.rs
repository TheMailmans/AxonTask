/// Event handling and serialization
///
/// This module provides utilities for working with task events:
/// - Serialization/deserialization for Redis Streams
/// - Event validation
/// - Stream key generation
///
/// # Example
///
/// ```no_run
/// use axontask_shared::events::serialization::{serialize_event, event_stream_key};
/// use axontask_shared::models::task_event::TaskEvent;
/// use uuid::Uuid;
///
/// # fn example(event: &TaskEvent) -> Result<(), Box<dyn std::error::Error>> {
/// // Serialize event for Redis
/// let fields = serialize_event(event)?;
///
/// // Get stream key
/// let key = event_stream_key(event.task_id);
/// println!("Store in Redis Stream: {}", key);
/// # Ok(())
/// # }
/// ```

pub mod serialization;

// Re-export common types
pub use serialization::{
    control_stream_key, deserialize_event, event_stream_key, heartbeat_key, serialize_event,
    SerializationError,
};
