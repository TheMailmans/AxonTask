/// Redis Stream writer for publishing events
///
/// This module provides functionality to write (publish) events to Redis Streams.
/// Events are written using the XADD command with automatic retry logic.
///
/// # Architecture
///
/// ```text
/// Worker/API
///     │
///     │ publish_event()
///     ▼
/// StreamWriter
///     │
///     │ XADD events:{task_id}
///     ▼
/// Redis Streams ──> Consumers (API SSE, other workers)
/// ```
///
/// # Example
///
/// ```no_run
/// use axontask_shared::redis::client::{RedisClient, RedisConfig};
/// use axontask_shared::redis::stream_writer::StreamWriter;
/// use axontask_shared::models::task_event::{TaskEvent, EventKind};
/// use serde_json::json;
/// use chrono::Utc;
/// use uuid::Uuid;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = RedisConfig::from_env()?;
/// let client = RedisClient::new(config).await?;
/// let writer = StreamWriter::new(client);
///
/// let event = TaskEvent {
///     task_id: Uuid::new_v4(),
///     seq: 0,
///     ts: Utc::now(),
///     kind: "started".to_string(),
///     payload: json!({"adapter": "shell"}),
///     hash_prev: None,
///     hash_curr: vec![1, 2, 3, 4],
/// };
///
/// let stream_id = writer.publish_event(&event).await?;
/// println!("Published event with stream ID: {}", stream_id);
/// # Ok(())
/// # }
/// ```

use crate::events::serialization::{event_stream_key, serialize_event, SerializationError};
use crate::models::task_event::TaskEvent;
use crate::redis::client::{RedisClient, RedisClientError};
use redis::AsyncCommands;
use thiserror::Error;
use uuid::Uuid;

/// Stream writer errors
#[derive(Error, Debug)]
pub enum StreamWriterError {
    /// Redis client error
    #[error("Redis error: {0}")]
    RedisError(#[from] RedisClientError),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] SerializationError),

    /// Write failed after retries
    #[error("Failed to write event after {attempts} attempts: {last_error}")]
    WriteFailed { attempts: u32, last_error: String },

    /// Raw Redis error
    #[error("Redis command error: {0}")]
    RedisCommandError(#[from] redis::RedisError),
}

/// Configuration for stream writer retry behavior
#[derive(Debug, Clone)]
pub struct StreamWriterConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,

    /// Base delay between retries in milliseconds
    pub base_retry_delay_ms: u64,

    /// Maximum delay between retries in milliseconds
    pub max_retry_delay_ms: u64,
}

impl Default for StreamWriterConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_retry_delay_ms: 100,
            max_retry_delay_ms: 5000,
        }
    }
}

/// Redis Stream writer for publishing events
///
/// Handles publishing events to Redis Streams with automatic retry logic
/// and exponential backoff.
#[derive(Clone)]
pub struct StreamWriter {
    client: RedisClient,
    config: StreamWriterConfig,
}

impl StreamWriter {
    /// Creates a new stream writer with default configuration
    ///
    /// # Arguments
    ///
    /// * `client` - Redis client to use for publishing
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::client::{RedisClient, RedisConfig};
    /// # use axontask_shared::redis::stream_writer::StreamWriter;
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = RedisConfig::from_env()?;
    /// let client = RedisClient::new(config).await?;
    /// let writer = StreamWriter::new(client);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(client: RedisClient) -> Self {
        Self {
            client,
            config: StreamWriterConfig::default(),
        }
    }

    /// Creates a new stream writer with custom configuration
    ///
    /// # Arguments
    ///
    /// * `client` - Redis client to use for publishing
    /// * `config` - Writer configuration (retry behavior)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::client::{RedisClient, RedisConfig};
    /// # use axontask_shared::redis::stream_writer::{StreamWriter, StreamWriterConfig};
    /// # async fn example() -> anyhow::Result<()> {
    /// let redis_config = RedisConfig::from_env()?;
    /// let client = RedisClient::new(redis_config).await?;
    ///
    /// let writer_config = StreamWriterConfig {
    ///     max_retries: 5,
    ///     base_retry_delay_ms: 200,
    ///     max_retry_delay_ms: 10000,
    /// };
    ///
    /// let writer = StreamWriter::with_config(client, writer_config);
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_config(client: RedisClient, config: StreamWriterConfig) -> Self {
        Self { client, config }
    }

    /// Publishes an event to the appropriate Redis Stream
    ///
    /// The event is serialized and written to `events:{task_id}` stream.
    /// Includes automatic retry with exponential backoff.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to publish
    ///
    /// # Returns
    ///
    /// The Redis Stream entry ID (format: "timestamp-sequence")
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Serialization fails
    /// - Redis connection fails after retries
    /// - XADD command fails after retries
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::stream_writer::StreamWriter;
    /// # use axontask_shared::models::task_event::TaskEvent;
    /// # async fn example(writer: &StreamWriter, event: &TaskEvent) -> Result<(), Box<dyn std::error::Error>> {
    /// let stream_id = writer.publish_event(event).await?;
    /// println!("Event published with ID: {}", stream_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn publish_event(&self, event: &TaskEvent) -> Result<String, StreamWriterError> {
        // Serialize event to Redis format
        let fields = serialize_event(event)?;

        // Get stream key
        let stream_key = event_stream_key(event.task_id);

        // Publish with retry
        let stream_id = self
            .xadd_with_retry(&stream_key, &fields)
            .await
            .map_err(|e| StreamWriterError::WriteFailed {
                attempts: self.config.max_retries + 1,
                last_error: e.to_string(),
            })?;

        tracing::debug!(
            task_id = %event.task_id,
            seq = event.seq,
            kind = %event.kind,
            stream_id = %stream_id,
            "Published event to Redis Stream"
        );

        Ok(stream_id)
    }

    /// Publishes multiple events in a batch
    ///
    /// More efficient than calling `publish_event` multiple times.
    /// All events must belong to the same task.
    ///
    /// # Arguments
    ///
    /// * `events` - Vector of events to publish (must have same task_id)
    ///
    /// # Returns
    ///
    /// Vector of Redis Stream entry IDs in the same order as input events
    ///
    /// # Errors
    ///
    /// Returns an error if events have different task_ids or if publish fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::stream_writer::StreamWriter;
    /// # use axontask_shared::models::task_event::TaskEvent;
    /// # async fn example(writer: &StreamWriter, events: Vec<TaskEvent>) -> Result<(), Box<dyn std::error::Error>> {
    /// let stream_ids = writer.publish_batch(&events).await?;
    /// println!("Published {} events", stream_ids.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn publish_batch(
        &self,
        events: &[TaskEvent],
    ) -> Result<Vec<String>, StreamWriterError> {
        if events.is_empty() {
            return Ok(Vec::new());
        }

        // Verify all events have the same task_id
        let task_id = events[0].task_id;
        if !events.iter().all(|e| e.task_id == task_id) {
            return Err(StreamWriterError::WriteFailed {
                attempts: 1,
                last_error: "All events in batch must have the same task_id".to_string(),
            });
        }

        let mut stream_ids = Vec::with_capacity(events.len());

        for event in events {
            let stream_id = self.publish_event(event).await?;
            stream_ids.push(stream_id);
        }

        tracing::debug!(
            task_id = %task_id,
            count = events.len(),
            "Published event batch to Redis Stream"
        );

        Ok(stream_ids)
    }

    /// Internal: Executes XADD with retry logic
    async fn xadd_with_retry(
        &self,
        stream_key: &str,
        fields: &std::collections::HashMap<String, String>,
    ) -> Result<String, redis::RedisError> {
        let mut attempt = 0;
        let mut last_error = None;

        while attempt <= self.config.max_retries {
            // Get connection
            let mut conn = self.client.get_connection();

            // Convert HashMap to Vec of (key, value) tuples for redis crate
            let items: Vec<(&str, &str)> = fields
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();

            // Execute XADD
            match conn.xadd(stream_key, "*", &items).await {
                Ok(stream_id) => return Ok(stream_id),
                Err(e) => {
                    last_error = Some(e);
                    attempt += 1;

                    if attempt <= self.config.max_retries {
                        // Calculate exponential backoff delay
                        let delay_ms = std::cmp::min(
                            self.config.base_retry_delay_ms * 2u64.pow(attempt - 1),
                            self.config.max_retry_delay_ms,
                        );

                        tracing::warn!(
                            stream_key = %stream_key,
                            attempt = attempt,
                            delay_ms = delay_ms,
                            "XADD failed, retrying..."
                        );

                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    /// Publishes a control message to the control stream
    ///
    /// Control messages are used for signaling (cancel, pause, resume, etc.)
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `command` - Control command (e.g., "cancel", "pause")
    /// * `data` - Optional command data
    ///
    /// # Returns
    ///
    /// The Redis Stream entry ID
    pub async fn publish_control_message(
        &self,
        task_id: Uuid,
        command: &str,
        data: Option<&str>,
    ) -> Result<String, StreamWriterError> {
        let stream_key = crate::events::serialization::control_stream_key(task_id);

        let mut fields = std::collections::HashMap::new();
        fields.insert("command".to_string(), command.to_string());
        if let Some(d) = data {
            fields.insert("data".to_string(), d.to_string());
        }

        let stream_id = self
            .xadd_with_retry(&stream_key, &fields)
            .await
            .map_err(|e| StreamWriterError::WriteFailed {
                attempts: self.config.max_retries + 1,
                last_error: e.to_string(),
            })?;

        tracing::info!(
            task_id = %task_id,
            command = %command,
            stream_id = %stream_id,
            "Published control message"
        );

        Ok(stream_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn create_test_event(task_id: Uuid, seq: i64) -> TaskEvent {
        TaskEvent {
            task_id,
            seq,
            ts: Utc::now(),
            kind: "stdout".to_string(),
            payload: json!({"data": "test output\n"}),
            hash_prev: if seq > 0 {
                Some(vec![1, 2, 3, 4])
            } else {
                None
            },
            hash_curr: vec![5, 6, 7, 8],
        }
    }

    #[test]
    fn test_stream_writer_config_default() {
        let config = StreamWriterConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_retry_delay_ms, 100);
        assert_eq!(config.max_retry_delay_ms, 5000);
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_publish_event() {
        use crate::redis::client::RedisConfig;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client);

        let task_id = Uuid::new_v4();
        let event = create_test_event(task_id, 0);

        let stream_id = writer.publish_event(&event).await.unwrap();
        assert!(!stream_id.is_empty());
        assert!(stream_id.contains('-')); // Redis stream ID format: timestamp-sequence
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_publish_batch() {
        use crate::redis::client::RedisConfig;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client);

        let task_id = Uuid::new_v4();
        let events = vec![
            create_test_event(task_id, 0),
            create_test_event(task_id, 1),
            create_test_event(task_id, 2),
        ];

        let stream_ids = writer.publish_batch(&events).await.unwrap();
        assert_eq!(stream_ids.len(), 3);
        assert!(stream_ids.iter().all(|id| !id.is_empty()));
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_publish_batch_different_task_ids() {
        use crate::redis::client::RedisConfig;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client);

        let events = vec![
            create_test_event(Uuid::new_v4(), 0),
            create_test_event(Uuid::new_v4(), 1), // Different task_id
        ];

        let result = writer.publish_batch(&events).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_publish_control_message() {
        use crate::redis::client::RedisConfig;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client);

        let task_id = Uuid::new_v4();
        let stream_id = writer
            .publish_control_message(task_id, "cancel", Some("user requested"))
            .await
            .unwrap();

        assert!(!stream_id.is_empty());
    }
}
