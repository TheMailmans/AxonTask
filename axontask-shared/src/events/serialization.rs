/// Event serialization for Redis Streams
///
/// This module handles serialization and deserialization of task events to/from
/// Redis Stream format. Redis Streams store entries as field-value string pairs,
/// so we need to convert our structured TaskEvent into that format.
///
/// # Format
///
/// Each event is stored in a Redis Stream with the following fields:
/// ```text
/// seq: "42"
/// kind: "stdout"
/// payload: "{\"data\":\"hello\"}"
/// ts: "2025-01-03T12:00:00Z"
/// hash_prev: "hex_encoded_sha256"
/// hash_curr: "hex_encoded_sha256"
/// ```
///
/// # Stream Naming
///
/// Events are stored in Redis Streams with keys:
/// - `events:{task_id}` - Task event stream
/// - `ctrl:{task_id}` - Control messages (cancel, pause, etc.)
/// - `hb:{task_id}` - Heartbeat stream
///
/// # Example
///
/// ```no_run
/// use axontask_shared::events::serialization::{serialize_event, deserialize_event};
/// use axontask_shared::models::task_event::{TaskEvent, EventKind};
/// use serde_json::json;
/// use chrono::Utc;
/// use uuid::Uuid;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let event = TaskEvent {
///     task_id: Uuid::new_v4(),
///     seq: 42,
///     ts: Utc::now(),
///     kind: "stdout".to_string(),
///     payload: json!({"data": "Hello, world!\n"}),
///     hash_prev: None,
///     hash_curr: vec![1, 2, 3, 4],
/// };
///
/// // Serialize to Redis format
/// let fields = serialize_event(&event)?;
///
/// // Deserialize back
/// let roundtrip = deserialize_event(&fields)?;
/// assert_eq!(event.seq, roundtrip.seq);
/// # Ok(())
/// # }
/// ```

use crate::models::task_event::TaskEvent;
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

/// Serialization errors
#[derive(Error, Debug)]
pub enum SerializationError {
    /// Missing required field
    #[error("Missing required field: {0}")]
    MissingField(String),

    /// Invalid field value
    #[error("Invalid field value for {field}: {error}")]
    InvalidValue { field: String, error: String },

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// UUID parsing error
    #[error("UUID error: {0}")]
    UuidError(#[from] uuid::Error),

    /// Timestamp parsing error
    #[error("Timestamp error: {0}")]
    TimestampError(String),

    /// Hex encoding/decoding error
    #[error("Hex error: {0}")]
    HexError(String),
}

/// Serializes a TaskEvent to Redis Stream field-value pairs
///
/// Converts a TaskEvent into a HashMap of string key-value pairs suitable
/// for storing in a Redis Stream entry.
///
/// # Arguments
///
/// * `event` - The event to serialize
///
/// # Returns
///
/// A HashMap where both keys and values are strings, ready for XADD.
///
/// # Example
///
/// ```no_run
/// # use axontask_shared::events::serialization::serialize_event;
/// # use axontask_shared::models::task_event::TaskEvent;
/// # use serde_json::json;
/// # use chrono::Utc;
/// # use uuid::Uuid;
/// # fn example(event: &TaskEvent) -> Result<(), Box<dyn std::error::Error>> {
/// let fields = serialize_event(event)?;
/// // fields can now be passed to Redis XADD command
/// # Ok(())
/// # }
/// ```
pub fn serialize_event(event: &TaskEvent) -> Result<HashMap<String, String>, SerializationError> {
    let mut fields = HashMap::new();

    // Serialize task_id
    fields.insert("task_id".to_string(), event.task_id.to_string());

    // Serialize seq
    fields.insert("seq".to_string(), event.seq.to_string());

    // Serialize timestamp (RFC3339 format)
    fields.insert("ts".to_string(), event.ts.to_rfc3339());

    // Serialize kind
    fields.insert("kind".to_string(), event.kind.clone());

    // Serialize payload (as JSON string)
    let payload_json = serde_json::to_string(&event.payload)?;
    fields.insert("payload".to_string(), payload_json);

    // Serialize hash_prev (hex encoded, or empty string if None)
    let hash_prev_hex = event
        .hash_prev
        .as_ref()
        .map(|h| hex::encode(h))
        .unwrap_or_default();
    fields.insert("hash_prev".to_string(), hash_prev_hex);

    // Serialize hash_curr (hex encoded)
    let hash_curr_hex = hex::encode(&event.hash_curr);
    fields.insert("hash_curr".to_string(), hash_curr_hex);

    Ok(fields)
}

/// Deserializes a TaskEvent from Redis Stream field-value pairs
///
/// Converts Redis Stream field-value pairs back into a TaskEvent.
///
/// # Arguments
///
/// * `fields` - HashMap of field-value pairs from Redis Stream entry
///
/// # Returns
///
/// The deserialized TaskEvent.
///
/// # Errors
///
/// Returns an error if:
/// - Required fields are missing
/// - Field values are malformed (invalid UUID, timestamp, JSON, etc.)
///
/// # Example
///
/// ```no_run
/// # use axontask_shared::events::serialization::deserialize_event;
/// # use std::collections::HashMap;
/// # fn example(fields: &HashMap<String, String>) -> Result<(), Box<dyn std::error::Error>> {
/// let event = deserialize_event(fields)?;
/// println!("Deserialized event seq={}", event.seq);
/// # Ok(())
/// # }
/// ```
pub fn deserialize_event(
    fields: &HashMap<String, String>,
) -> Result<TaskEvent, SerializationError> {
    // Parse task_id
    let task_id_str = fields
        .get("task_id")
        .ok_or_else(|| SerializationError::MissingField("task_id".to_string()))?;
    let task_id = Uuid::parse_str(task_id_str)?;

    // Parse seq
    let seq_str = fields
        .get("seq")
        .ok_or_else(|| SerializationError::MissingField("seq".to_string()))?;
    let seq = seq_str.parse::<i64>().map_err(|e| {
        SerializationError::InvalidValue {
            field: "seq".to_string(),
            error: e.to_string(),
        }
    })?;

    // Parse timestamp
    let ts_str = fields
        .get("ts")
        .ok_or_else(|| SerializationError::MissingField("ts".to_string()))?;
    let ts = DateTime::parse_from_rfc3339(ts_str)
        .map_err(|e| SerializationError::TimestampError(e.to_string()))?
        .with_timezone(&Utc);

    // Parse kind
    let kind = fields
        .get("kind")
        .ok_or_else(|| SerializationError::MissingField("kind".to_string()))?
        .clone();

    // Parse payload
    let payload_str = fields
        .get("payload")
        .ok_or_else(|| SerializationError::MissingField("payload".to_string()))?;
    let payload: JsonValue = serde_json::from_str(payload_str)?;

    // Parse hash_prev (optional, empty string means None)
    let hash_prev_str = fields.get("hash_prev").map(|s| s.as_str()).unwrap_or("");
    let hash_prev = if hash_prev_str.is_empty() {
        None
    } else {
        let bytes = hex::decode(hash_prev_str)
            .map_err(|e| SerializationError::HexError(e.to_string()))?;
        Some(bytes)
    };

    // Parse hash_curr
    let hash_curr_str = fields
        .get("hash_curr")
        .ok_or_else(|| SerializationError::MissingField("hash_curr".to_string()))?;
    let hash_curr =
        hex::decode(hash_curr_str).map_err(|e| SerializationError::HexError(e.to_string()))?;

    Ok(TaskEvent {
        task_id,
        seq,
        ts,
        kind,
        payload,
        hash_prev,
        hash_curr,
    })
}

/// Generates Redis Stream key for task events
///
/// # Arguments
///
/// * `task_id` - The task UUID
///
/// # Returns
///
/// Stream key in format `events:{task_id}`
///
/// # Example
///
/// ```
/// use axontask_shared::events::serialization::event_stream_key;
/// use uuid::Uuid;
///
/// let task_id = Uuid::new_v4();
/// let key = event_stream_key(task_id);
/// assert!(key.starts_with("events:"));
/// ```
pub fn event_stream_key(task_id: Uuid) -> String {
    format!("events:{}", task_id)
}

/// Generates Redis Stream key for control messages
///
/// # Arguments
///
/// * `task_id` - The task UUID
///
/// # Returns
///
/// Stream key in format `ctrl:{task_id}`
///
/// # Example
///
/// ```
/// use axontask_shared::events::serialization::control_stream_key;
/// use uuid::Uuid;
///
/// let task_id = Uuid::new_v4();
/// let key = control_stream_key(task_id);
/// assert!(key.starts_with("ctrl:"));
/// ```
pub fn control_stream_key(task_id: Uuid) -> String {
    format!("ctrl:{}", task_id)
}

/// Generates Redis key for heartbeats
///
/// # Arguments
///
/// * `task_id` - The task UUID
///
/// # Returns
///
/// Key in format `hb:{task_id}`
///
/// # Example
///
/// ```
/// use axontask_shared::events::serialization::heartbeat_key;
/// use uuid::Uuid;
///
/// let task_id = Uuid::new_v4();
/// let key = heartbeat_key(task_id);
/// assert!(key.starts_with("hb:"));
/// ```
pub fn heartbeat_key(task_id: Uuid) -> String {
    format!("hb:{}", task_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_event() -> TaskEvent {
        TaskEvent {
            task_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            seq: 42,
            ts: DateTime::parse_from_rfc3339("2025-01-03T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            kind: "stdout".to_string(),
            payload: json!({"data": "Hello, world!\n"}),
            hash_prev: Some(vec![1, 2, 3, 4, 5, 6, 7, 8]),
            hash_curr: vec![9, 10, 11, 12, 13, 14, 15, 16],
        }
    }

    #[test]
    fn test_serialize_event() {
        let event = create_test_event();
        let fields = serialize_event(&event).unwrap();

        assert_eq!(
            fields.get("task_id").unwrap(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(fields.get("seq").unwrap(), "42");
        assert_eq!(fields.get("ts").unwrap(), "2025-01-03T12:00:00+00:00");
        assert_eq!(fields.get("kind").unwrap(), "stdout");
        assert!(fields.get("payload").unwrap().contains("Hello, world!"));
        assert_eq!(fields.get("hash_prev").unwrap(), "0102030405060708");
        assert_eq!(fields.get("hash_curr").unwrap(), "090a0b0c0d0e0f10");
    }

    #[test]
    fn test_serialize_event_no_hash_prev() {
        let mut event = create_test_event();
        event.hash_prev = None;

        let fields = serialize_event(&event).unwrap();
        assert_eq!(fields.get("hash_prev").unwrap(), "");
    }

    #[test]
    fn test_deserialize_event() {
        let event = create_test_event();
        let fields = serialize_event(&event).unwrap();
        let roundtrip = deserialize_event(&fields).unwrap();

        assert_eq!(roundtrip.task_id, event.task_id);
        assert_eq!(roundtrip.seq, event.seq);
        assert_eq!(roundtrip.ts, event.ts);
        assert_eq!(roundtrip.kind, event.kind);
        assert_eq!(roundtrip.payload, event.payload);
        assert_eq!(roundtrip.hash_prev, event.hash_prev);
        assert_eq!(roundtrip.hash_curr, event.hash_curr);
    }

    #[test]
    fn test_roundtrip_serialization() {
        let event = create_test_event();
        let fields = serialize_event(&event).unwrap();
        let roundtrip = deserialize_event(&fields).unwrap();

        // Serialize again to ensure consistency
        let fields2 = serialize_event(&roundtrip).unwrap();
        assert_eq!(fields, fields2);
    }

    #[test]
    fn test_roundtrip_all_event_kinds() {
        let kinds = vec![
            "started", "progress", "stdout", "stderr", "success", "error", "canceled", "timeout",
            "digest",
        ];

        for kind in kinds {
            let mut event = create_test_event();
            event.kind = kind.to_string();

            let fields = serialize_event(&event).unwrap();
            let roundtrip = deserialize_event(&fields).unwrap();

            assert_eq!(roundtrip.kind, kind);
        }
    }

    #[test]
    fn test_deserialize_missing_field() {
        let mut fields = HashMap::new();
        fields.insert("seq".to_string(), "42".to_string());
        // Missing task_id

        let result = deserialize_event(&fields);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SerializationError::MissingField(_)
        ));
    }

    #[test]
    fn test_deserialize_invalid_uuid() {
        let mut fields = HashMap::new();
        fields.insert("task_id".to_string(), "not-a-uuid".to_string());
        fields.insert("seq".to_string(), "42".to_string());
        fields.insert("ts".to_string(), "2025-01-03T12:00:00Z".to_string());
        fields.insert("kind".to_string(), "stdout".to_string());
        fields.insert("payload".to_string(), "{}".to_string());
        fields.insert("hash_prev".to_string(), "".to_string());
        fields.insert("hash_curr".to_string(), "090a0b0c".to_string());

        let result = deserialize_event(&fields);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SerializationError::UuidError(_)));
    }

    #[test]
    fn test_deserialize_invalid_seq() {
        let mut fields = HashMap::new();
        fields.insert(
            "task_id".to_string(),
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
        );
        fields.insert("seq".to_string(), "not-a-number".to_string());
        fields.insert("ts".to_string(), "2025-01-03T12:00:00Z".to_string());
        fields.insert("kind".to_string(), "stdout".to_string());
        fields.insert("payload".to_string(), "{}".to_string());
        fields.insert("hash_prev".to_string(), "".to_string());
        fields.insert("hash_curr".to_string(), "090a0b0c".to_string());

        let result = deserialize_event(&fields);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SerializationError::InvalidValue { .. }
        ));
    }

    #[test]
    fn test_deserialize_invalid_json() {
        let mut fields = HashMap::new();
        fields.insert(
            "task_id".to_string(),
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
        );
        fields.insert("seq".to_string(), "42".to_string());
        fields.insert("ts".to_string(), "2025-01-03T12:00:00Z".to_string());
        fields.insert("kind".to_string(), "stdout".to_string());
        fields.insert("payload".to_string(), "{invalid json}".to_string());
        fields.insert("hash_prev".to_string(), "".to_string());
        fields.insert("hash_curr".to_string(), "090a0b0c".to_string());

        let result = deserialize_event(&fields);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SerializationError::JsonError(_)
        ));
    }

    #[test]
    fn test_stream_key_generation() {
        let task_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();

        assert_eq!(
            event_stream_key(task_id),
            "events:550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(
            control_stream_key(task_id),
            "ctrl:550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(
            heartbeat_key(task_id),
            "hb:550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_payload_with_complex_json() {
        let mut event = create_test_event();
        event.payload = json!({
            "data": "output line\n",
            "metadata": {
                "timestamp": 1234567890,
                "source": "worker-1"
            },
            "tags": ["important", "urgent"]
        });

        let fields = serialize_event(&event).unwrap();
        let roundtrip = deserialize_event(&fields).unwrap();

        assert_eq!(roundtrip.payload, event.payload);
    }
}
