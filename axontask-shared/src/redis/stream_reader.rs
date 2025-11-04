/// Redis Stream reader for consuming events
///
/// This module provides functionality to read events from Redis Streams with:
/// - **Backfill**: Read historical events with pagination (XREAD with COUNT)
/// - **Live tail**: Block and wait for new events in real-time (XREAD BLOCK)
///
/// # Architecture
///
/// ```text
/// Redis Streams (events:{task_id})
///     │
///     ├──> Backfill: XREAD COUNT 1000 STREAMS events:{task_id} {since_id}
///     │    Returns: Historical events in batches
///     │
///     └──> Live Tail: XREAD BLOCK 5000 STREAMS events:{task_id} {last_id}
///          Returns: New events as they arrive (with timeout)
/// ```
///
/// # Example - Backfill
///
/// ```no_run
/// use axontask_shared::redis::client::{RedisClient, RedisConfig};
/// use axontask_shared::redis::stream_reader::StreamReader;
/// use uuid::Uuid;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = RedisConfig::from_env()?;
/// let client = RedisClient::new(config).await?;
/// let reader = StreamReader::new(client);
///
/// let task_id = Uuid::new_v4();
///
/// // Read all historical events
/// let events = reader.read_backfill(task_id, "0", 1000).await?;
/// println!("Backfilled {} events", events.len());
/// # Ok(())
/// # }
/// ```
///
/// # Example - Live Tail
///
/// ```no_run
/// # use axontask_shared::redis::client::{RedisClient, RedisConfig};
/// # use axontask_shared::redis::stream_reader::StreamReader;
/// # use uuid::Uuid;
/// # async fn example() -> anyhow::Result<()> {
/// # let config = RedisConfig::from_env()?;
/// # let client = RedisClient::new(config).await?;
/// # let reader = StreamReader::new(client);
/// # let task_id = Uuid::new_v4();
/// let mut last_id = "$".to_string(); // Start from end
///
/// loop {
///     let events = reader.read_live(task_id, &last_id, 5000).await?;
///
///     if events.is_empty() {
///         // Timeout, no new events
///         continue;
///     }
///
///     for (stream_id, event) in events {
///         println!("New event: seq={}", event.seq);
///         last_id = stream_id;
///     }
/// }
/// # Ok(())
/// # }
/// ```

use crate::events::serialization::{deserialize_event, event_stream_key, SerializationError};
use crate::models::task_event::TaskEvent;
use crate::redis::client::{RedisClient, RedisClientError};
use redis::{streams::StreamReadOptions, streams::StreamReadReply, AsyncCommands};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

/// Stream reader errors
#[derive(Error, Debug)]
pub enum StreamReaderError {
    /// Redis client error
    #[error("Redis error: {0}")]
    RedisError(#[from] RedisClientError),

    /// Serialization error
    #[error("Deserialization error: {0}")]
    DeserializationError(#[from] SerializationError),

    /// Raw Redis error
    #[error("Redis command error: {0}")]
    RedisCommandError(#[from] redis::RedisError),

    /// Invalid stream ID
    #[error("Invalid stream ID: {0}")]
    InvalidStreamId(String),
}

/// Configuration for stream reader behavior
#[derive(Debug, Clone)]
pub struct StreamReaderConfig {
    /// Default batch size for backfill operations
    pub default_batch_size: usize,

    /// Default timeout for live reads in milliseconds
    pub default_live_timeout_ms: usize,

    /// Maximum batch size to prevent memory issues
    pub max_batch_size: usize,
}

impl Default for StreamReaderConfig {
    fn default() -> Self {
        Self {
            default_batch_size: 1000,
            default_live_timeout_ms: 5000,
            max_batch_size: 10000,
        }
    }
}

/// Redis Stream reader for consuming events
///
/// Provides both backfill (historical) and live tail (real-time) reading.
#[derive(Clone)]
pub struct StreamReader {
    client: RedisClient,
    config: StreamReaderConfig,
}

impl StreamReader {
    /// Creates a new stream reader with default configuration
    ///
    /// # Arguments
    ///
    /// * `client` - Redis client to use for reading
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::client::{RedisClient, RedisConfig};
    /// # use axontask_shared::redis::stream_reader::StreamReader;
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = RedisConfig::from_env()?;
    /// let client = RedisClient::new(config).await?;
    /// let reader = StreamReader::new(client);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(client: RedisClient) -> Self {
        Self {
            client,
            config: StreamReaderConfig::default(),
        }
    }

    /// Creates a new stream reader with custom configuration
    ///
    /// # Arguments
    ///
    /// * `client` - Redis client to use for reading
    /// * `config` - Reader configuration
    pub fn with_config(client: RedisClient, config: StreamReaderConfig) -> Self {
        Self { client, config }
    }

    /// Reads historical events (backfill) from a Redis Stream
    ///
    /// Uses XREAD with COUNT to fetch a batch of historical events.
    /// This is non-blocking and returns immediately.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `since_id` - Stream ID to start from (use "0" for beginning, or last known ID)
    /// * `count` - Maximum number of events to fetch
    ///
    /// # Returns
    ///
    /// Vector of (stream_id, event) tuples in chronological order.
    /// Returns empty vector if no events found.
    ///
    /// # Special Stream IDs
    ///
    /// - `"0"` - Read from the beginning of the stream
    /// - `"$"` - Read from the end (typically returns empty for backfill)
    /// - `"{timestamp}-{sequence}"` - Read from specific position
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::stream_reader::StreamReader;
    /// # use uuid::Uuid;
    /// # async fn example(reader: &StreamReader) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    ///
    /// // Read first 100 events
    /// let events = reader.read_backfill(task_id, "0", 100).await?;
    ///
    /// // Read next 100 events
    /// if let Some((last_id, _)) = events.last() {
    ///     let more_events = reader.read_backfill(task_id, last_id, 100).await?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read_backfill(
        &self,
        task_id: Uuid,
        since_id: &str,
        count: usize,
    ) -> Result<Vec<(String, TaskEvent)>, StreamReaderError> {
        // Enforce max batch size
        let safe_count = std::cmp::min(count, self.config.max_batch_size);

        let stream_key = event_stream_key(task_id);
        let mut conn = self.client.get_connection();

        // Execute XREAD with COUNT
        let opts = StreamReadOptions::default().count(safe_count);
        let reply: StreamReadReply = conn
            .xread_options(&[&stream_key], &[since_id], &opts)
            .await?;

        // Parse reply and deserialize events
        let mut events = Vec::new();

        for stream_key_result in reply.keys {
            for stream_id_result in stream_key_result.ids {
                let stream_id = stream_id_result.id;

                // Convert Redis map to HashMap<String, String>
                let fields: HashMap<String, String> = stream_id_result
                    .map
                    .into_iter()
                    .filter_map(|(k, v)| {
                        let key = String::from_utf8(k.as_ref().to_vec()).ok()?;
                        let value = redis::from_redis_value::<String>(&v).ok()?;
                        Some((key, value))
                    })
                    .collect();

                // Deserialize event
                match deserialize_event(&fields) {
                    Ok(event) => events.push((stream_id, event)),
                    Err(e) => {
                        tracing::error!(
                            task_id = %task_id,
                            stream_id = %stream_id,
                            error = %e,
                            "Failed to deserialize event, skipping"
                        );
                        // Continue processing other events
                    }
                }
            }
        }

        tracing::debug!(
            task_id = %task_id,
            since_id = %since_id,
            count = safe_count,
            fetched = events.len(),
            "Backfilled events from stream"
        );

        Ok(events)
    }

    /// Reads new events in real-time (live tail) with blocking
    ///
    /// Uses XREAD BLOCK to wait for new events. This is blocking up to the
    /// specified timeout.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `after_id` - Stream ID to read after (use "$" for latest, or last known ID)
    /// * `timeout_ms` - Block timeout in milliseconds (0 = infinite, not recommended)
    ///
    /// # Returns
    ///
    /// Vector of (stream_id, event) tuples for new events.
    /// Returns empty vector if timeout expires with no new events.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::stream_reader::StreamReader;
    /// # use uuid::Uuid;
    /// # async fn example(reader: &StreamReader) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    /// let mut last_id = "$".to_string();
    ///
    /// loop {
    ///     // Block for up to 5 seconds waiting for new events
    ///     let events = reader.read_live(task_id, &last_id, 5000).await?;
    ///
    ///     if events.is_empty() {
    ///         println!("No new events (timeout)");
    ///         continue;
    ///     }
    ///
    ///     for (stream_id, event) in events {
    ///         println!("New event: {}", event.kind);
    ///         last_id = stream_id;
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read_live(
        &self,
        task_id: Uuid,
        after_id: &str,
        timeout_ms: usize,
    ) -> Result<Vec<(String, TaskEvent)>, StreamReaderError> {
        let stream_key = event_stream_key(task_id);
        let mut conn = self.client.get_connection();

        // Execute XREAD BLOCK
        let opts = StreamReadOptions::default()
            .count(self.config.default_batch_size)
            .block(timeout_ms);

        let reply: StreamReadReply = conn
            .xread_options(&[&stream_key], &[after_id], &opts)
            .await?;

        // Parse reply and deserialize events
        let mut events = Vec::new();

        for stream_key_result in reply.keys {
            for stream_id_result in stream_key_result.ids {
                let stream_id = stream_id_result.id;

                // Convert Redis map to HashMap<String, String>
                let fields: HashMap<String, String> = stream_id_result
                    .map
                    .into_iter()
                    .filter_map(|(k, v)| {
                        let key = String::from_utf8(k.as_ref().to_vec()).ok()?;
                        let value = redis::from_redis_value::<String>(&v).ok()?;
                        Some((key, value))
                    })
                    .collect();

                // Deserialize event
                match deserialize_event(&fields) {
                    Ok(event) => events.push((stream_id, event)),
                    Err(e) => {
                        tracing::error!(
                            task_id = %task_id,
                            stream_id = %stream_id,
                            error = %e,
                            "Failed to deserialize event, skipping"
                        );
                    }
                }
            }
        }

        if !events.is_empty() {
            tracing::debug!(
                task_id = %task_id,
                after_id = %after_id,
                count = events.len(),
                "Received live events"
            );
        }

        Ok(events)
    }

    /// Reads all events for a task from the beginning
    ///
    /// Convenience method that handles pagination automatically.
    ///
    /// ⚠️  Use with caution for tasks with many events, as this loads
    /// everything into memory.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    ///
    /// # Returns
    ///
    /// Vector of all events in chronological order
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::stream_reader::StreamReader;
    /// # use uuid::Uuid;
    /// # async fn example(reader: &StreamReader) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    /// let all_events = reader.read_all(task_id).await?;
    /// println!("Task has {} total events", all_events.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read_all(&self, task_id: Uuid) -> Result<Vec<TaskEvent>, StreamReaderError> {
        let mut all_events = Vec::new();
        let mut last_id = "0".to_string();

        loop {
            let batch = self
                .read_backfill(task_id, &last_id, self.config.default_batch_size)
                .await?;

            if batch.is_empty() {
                break;
            }

            // Update last_id for next iteration
            if let Some((stream_id, _)) = batch.last() {
                last_id = stream_id.clone();
            }

            // Add events to result
            all_events.extend(batch.into_iter().map(|(_, event)| event));
        }

        tracing::info!(
            task_id = %task_id,
            total_events = all_events.len(),
            "Read all events from stream"
        );

        Ok(all_events)
    }

    /// Gets the last event in the stream without reading all events
    ///
    /// More efficient than `read_all()` when you only need the latest event.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    ///
    /// # Returns
    ///
    /// The last event, or None if stream is empty
    pub async fn read_last(
        &self,
        task_id: Uuid,
    ) -> Result<Option<(String, TaskEvent)>, StreamReaderError> {
        let stream_key = event_stream_key(task_id);
        let mut conn = self.client.get_connection();

        // Use XREVRANGE to get the last entry
        let reply: StreamReadReply = conn.xrevrange_count(&stream_key, "+", "-", 1).await?;

        for stream_key_result in reply.keys {
            for stream_id_result in stream_key_result.ids {
                let stream_id = stream_id_result.id;

                let fields: HashMap<String, String> = stream_id_result
                    .map
                    .into_iter()
                    .filter_map(|(k, v)| {
                        let key = String::from_utf8(k.as_ref().to_vec()).ok()?;
                        let value = redis::from_redis_value::<String>(&v).ok()?;
                        Some((key, value))
                    })
                    .collect();

                let event = deserialize_event(&fields)?;
                return Ok(Some((stream_id, event)));
            }
        }

        Ok(None)
    }

    /// Counts total number of entries in the stream
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    ///
    /// # Returns
    ///
    /// Number of entries in the stream
    pub async fn count_entries(&self, task_id: Uuid) -> Result<usize, StreamReaderError> {
        let stream_key = event_stream_key(task_id);
        let mut conn = self.client.get_connection();

        let count: usize = conn.xlen(&stream_key).await?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redis::client::RedisConfig;
    use crate::redis::stream_writer::StreamWriter;
    use chrono::Utc;
    use serde_json::json;

    fn create_test_event(task_id: Uuid, seq: i64) -> TaskEvent {
        TaskEvent {
            task_id,
            seq,
            ts: Utc::now(),
            kind: "stdout".to_string(),
            payload: json!({"data": format!("Line {}\n", seq)}),
            hash_prev: if seq > 0 {
                Some(vec![1, 2, 3, 4])
            } else {
                None
            },
            hash_curr: vec![5, 6, 7, 8],
        }
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_read_backfill() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client.clone());
        let reader = StreamReader::new(client);

        let task_id = Uuid::new_v4();

        // Write some events
        for seq in 0..5 {
            let event = create_test_event(task_id, seq);
            writer.publish_event(&event).await.unwrap();
        }

        // Read them back
        let events = reader.read_backfill(task_id, "0", 100).await.unwrap();
        assert_eq!(events.len(), 5);

        // Verify order
        for (i, (_, event)) in events.iter().enumerate() {
            assert_eq!(event.seq, i as i64);
        }
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_read_backfill_pagination() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client.clone());
        let reader = StreamReader::new(client);

        let task_id = Uuid::new_v4();

        // Write 10 events
        for seq in 0..10 {
            let event = create_test_event(task_id, seq);
            writer.publish_event(&event).await.unwrap();
        }

        // Read in batches of 3
        let batch1 = reader.read_backfill(task_id, "0", 3).await.unwrap();
        assert_eq!(batch1.len(), 3);

        let last_id = &batch1.last().unwrap().0;
        let batch2 = reader.read_backfill(task_id, last_id, 3).await.unwrap();
        assert_eq!(batch2.len(), 3);

        // Sequences should continue
        assert_eq!(batch1[0].1.seq, 0);
        assert_eq!(batch1[2].1.seq, 2);
        assert_eq!(batch2[0].1.seq, 3);
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_read_live() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client.clone());
        let reader = StreamReader::new(client);

        let task_id = Uuid::new_v4();

        // Start live reader from end
        let reader_task = {
            let reader = reader.clone();
            tokio::spawn(async move { reader.read_live(task_id, "$", 2000).await })
        };

        // Give reader time to start blocking
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Write an event
        let event = create_test_event(task_id, 0);
        writer.publish_event(&event).await.unwrap();

        // Reader should receive it
        let events = reader_task.await.unwrap().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].1.seq, 0);
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_read_all() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client.clone());
        let reader = StreamReader::new(client);

        let task_id = Uuid::new_v4();

        // Write multiple events
        for seq in 0..25 {
            let event = create_test_event(task_id, seq);
            writer.publish_event(&event).await.unwrap();
        }

        // Read all
        let all_events = reader.read_all(task_id).await.unwrap();
        assert_eq!(all_events.len(), 25);

        // Verify order
        for (i, event) in all_events.iter().enumerate() {
            assert_eq!(event.seq, i as i64);
        }
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_read_last() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client.clone());
        let reader = StreamReader::new(client);

        let task_id = Uuid::new_v4();

        // Write some events
        for seq in 0..5 {
            let event = create_test_event(task_id, seq);
            writer.publish_event(&event).await.unwrap();
        }

        // Get last event
        let last = reader.read_last(task_id).await.unwrap();
        assert!(last.is_some());
        assert_eq!(last.unwrap().1.seq, 4);
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_count_entries() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client.clone());
        let reader = StreamReader::new(client);

        let task_id = Uuid::new_v4();

        // Initially empty
        let count = reader.count_entries(task_id).await.unwrap();
        assert_eq!(count, 0);

        // Write some events
        for seq in 0..3 {
            let event = create_test_event(task_id, seq);
            writer.publish_event(&event).await.unwrap();
        }

        // Count should match
        let count = reader.count_entries(task_id).await.unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_read_empty_stream() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let reader = StreamReader::new(client);

        let task_id = Uuid::new_v4();

        // Read from non-existent stream
        let events = reader.read_backfill(task_id, "0", 100).await.unwrap();
        assert_eq!(events.len(), 0);

        let all_events = reader.read_all(task_id).await.unwrap();
        assert_eq!(all_events.len(), 0);

        let last = reader.read_last(task_id).await.unwrap();
        assert!(last.is_none());
    }
}
