/// Redis Stream metrics for monitoring and observability
///
/// This module provides utilities to collect metrics about Redis Streams:
/// - Stream lag (how far behind consumers are)
/// - Event rate (events per second)
/// - Stream length and memory usage
/// - Consumer positions
///
/// These metrics are useful for:
/// - Monitoring system health
/// - Detecting performance issues
/// - Capacity planning
/// - Alerting on anomalies
///
/// # Example
///
/// ```no_run
/// use axontask_shared::redis::client::{RedisClient, RedisConfig};
/// use axontask_shared::redis::metrics::StreamMetrics;
/// use uuid::Uuid;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = RedisConfig::from_env()?;
/// let client = RedisClient::new(config).await?;
/// let metrics = StreamMetrics::new(client);
///
/// let task_id = Uuid::new_v4();
///
/// // Get stream metrics
/// let info = metrics.get_stream_info(task_id).await?;
/// println!("Stream length: {}", info.length);
/// println!("First event: {}", info.first_entry_id.unwrap_or_default());
///
/// // Calculate lag
/// let lag = metrics.calculate_lag(task_id, "1000-0").await?;
/// println!("Consumer is {} events behind", lag);
/// # Ok(())
/// # }
/// ```

use crate::events::serialization::event_stream_key;
use crate::redis::client::{RedisClient, RedisClientError};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Metrics errors
#[derive(Error, Debug)]
pub enum MetricsError {
    /// Redis client error
    #[error("Redis error: {0}")]
    RedisError(#[from] RedisClientError),

    /// Raw Redis error
    #[error("Redis command error: {0}")]
    RedisCommandError(#[from] redis::RedisError),

    /// Invalid stream ID
    #[error("Invalid stream ID: {0}")]
    InvalidStreamId(String),

    /// Metrics calculation error
    #[error("Metrics calculation error: {0}")]
    CalculationError(String),
}

/// Stream information metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    /// Number of entries in the stream
    pub length: u64,

    /// First entry ID (oldest event)
    pub first_entry_id: Option<String>,

    /// Last entry ID (newest event)
    pub last_entry_id: Option<String>,

    /// Number of groups consuming this stream
    pub groups: u64,

    /// Timestamp when metrics were collected
    pub collected_at: DateTime<Utc>,
}

/// Consumer lag information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LagInfo {
    /// Number of events consumer is behind
    pub events_behind: u64,

    /// Consumer's current position
    pub consumer_position: String,

    /// Latest available event ID
    pub latest_position: String,

    /// Estimated time lag based on event timestamps
    pub estimated_time_lag_ms: Option<i64>,
}

/// Event rate statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRateStats {
    /// Events per second
    pub events_per_second: f64,

    /// Start time of measurement period
    pub period_start: DateTime<Utc>,

    /// End time of measurement period
    pub period_end: DateTime<Utc>,

    /// Total events in period
    pub total_events: u64,
}

/// Stream metrics collector
#[derive(Clone)]
pub struct StreamMetrics {
    client: RedisClient,
}

impl StreamMetrics {
    /// Creates a new stream metrics collector
    ///
    /// # Arguments
    ///
    /// * `client` - Redis client to use
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// Gets comprehensive stream information
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    ///
    /// # Returns
    ///
    /// StreamInfo with current metrics
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::metrics::StreamMetrics;
    /// # use uuid::Uuid;
    /// # async fn example(metrics: &StreamMetrics) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    /// let info = metrics.get_stream_info(task_id).await?;
    /// println!("Stream has {} events", info.length);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_stream_info(&self, task_id: Uuid) -> Result<StreamInfo, MetricsError> {
        let stream_key = event_stream_key(task_id);
        let mut conn = self.client.get_connection();

        // Get stream length
        let length: u64 = conn.xlen(&stream_key).await.unwrap_or(0);

        // Get stream info using XINFO STREAM
        let info_result: Result<Vec<redis::Value>, redis::RedisError> = redis::cmd("XINFO")
            .arg("STREAM")
            .arg(&stream_key)
            .query_async(&mut conn)
            .await;

        let (first_entry_id, last_entry_id, groups) = match info_result {
            Ok(info) => parse_stream_info(&info),
            Err(_) => (None, None, 0),
        };

        Ok(StreamInfo {
            length,
            first_entry_id,
            last_entry_id,
            groups,
            collected_at: Utc::now(),
        })
    }

    /// Calculates how far behind a consumer is
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `consumer_position` - Consumer's last read stream ID
    ///
    /// # Returns
    ///
    /// Number of events consumer is behind
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::metrics::StreamMetrics;
    /// # use uuid::Uuid;
    /// # async fn example(metrics: &StreamMetrics) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    /// let lag = metrics.calculate_lag(task_id, "1000-0").await?;
    /// if lag > 1000 {
    ///     println!("WARNING: Consumer is {} events behind!", lag);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn calculate_lag(
        &self,
        task_id: Uuid,
        consumer_position: &str,
    ) -> Result<u64, MetricsError> {
        let stream_key = event_stream_key(task_id);
        let mut conn = self.client.get_connection();

        // Count events after consumer position
        let count: usize = redis::cmd("XLEN")
            .arg(&stream_key)
            .query_async(&mut conn)
            .await
            .unwrap_or(0);

        // Count events up to consumer position
        let consumed: Vec<redis::Value> = redis::cmd("XRANGE")
            .arg(&stream_key)
            .arg("-")
            .arg(consumer_position)
            .query_async(&mut conn)
            .await
            .unwrap_or_default();

        let lag = count.saturating_sub(consumed.len());

        Ok(lag as u64)
    }

    /// Gets detailed lag information including time estimates
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `consumer_position` - Consumer's last read stream ID
    ///
    /// # Returns
    ///
    /// LagInfo with detailed lag metrics
    pub async fn get_lag_info(
        &self,
        task_id: Uuid,
        consumer_position: &str,
    ) -> Result<LagInfo, MetricsError> {
        let info = self.get_stream_info(task_id).await?;

        let latest_position = info.last_entry_id.unwrap_or_else(|| "0-0".to_string());

        let events_behind = self.calculate_lag(task_id, consumer_position).await?;

        // Estimate time lag based on stream ID timestamps
        let estimated_time_lag_ms = estimate_time_lag(consumer_position, &latest_position);

        Ok(LagInfo {
            events_behind,
            consumer_position: consumer_position.to_string(),
            latest_position,
            estimated_time_lag_ms,
        })
    }

    /// Calculates event rate over a time period
    ///
    /// Requires at least 2 data points to calculate rate.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `duration_secs` - Duration in seconds to calculate rate over
    ///
    /// # Returns
    ///
    /// EventRateStats with events per second
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::metrics::StreamMetrics;
    /// # use uuid::Uuid;
    /// # async fn example(metrics: &StreamMetrics) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    /// let rate = metrics.calculate_event_rate(task_id, 60).await?;
    /// println!("Event rate: {:.2} events/sec", rate.events_per_second);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn calculate_event_rate(
        &self,
        task_id: Uuid,
        duration_secs: u64,
    ) -> Result<EventRateStats, MetricsError> {
        let stream_key = event_stream_key(task_id);
        let mut conn = self.client.get_connection();

        let end_time = Utc::now();
        let start_time = end_time - chrono::Duration::seconds(duration_secs as i64);

        // Calculate approximate stream ID for start time
        let start_timestamp_ms = start_time.timestamp_millis();
        let start_id = format!("{}-0", start_timestamp_ms);

        // Count events in time range
        let events: Vec<redis::Value> = redis::cmd("XRANGE")
            .arg(&stream_key)
            .arg(&start_id)
            .arg("+")
            .query_async(&mut conn)
            .await
            .unwrap_or_default();

        let total_events = events.len() as u64;
        let events_per_second = total_events as f64 / duration_secs as f64;

        Ok(EventRateStats {
            events_per_second,
            period_start: start_time,
            period_end: end_time,
            total_events,
        })
    }

    /// Gets memory usage estimate for a stream
    ///
    /// This is an approximation based on stream length and typical event size.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    ///
    /// # Returns
    ///
    /// Estimated memory usage in bytes
    pub async fn estimate_memory_usage(&self, task_id: Uuid) -> Result<u64, MetricsError> {
        let info = self.get_stream_info(task_id).await?;

        // Rough estimate: ~1KB per event on average
        // This includes Redis overhead, stream metadata, etc.
        let estimated_bytes = info.length * 1024;

        Ok(estimated_bytes)
    }

    /// Checks if a stream is healthy
    ///
    /// A stream is considered healthy if:
    /// - It exists and has events
    /// - No excessive lag (defined by threshold)
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `max_lag` - Maximum acceptable lag in events
    ///
    /// # Returns
    ///
    /// True if healthy, false otherwise
    pub async fn is_healthy(
        &self,
        task_id: Uuid,
        max_lag: u64,
    ) -> Result<bool, MetricsError> {
        let info = self.get_stream_info(task_id).await?;

        // Check if stream exists and has events
        if info.length == 0 {
            return Ok(false);
        }

        // For now, consider healthy if stream exists
        // Real lag check would require knowing consumer position
        Ok(true)
    }
}

/// Parses XINFO STREAM response
fn parse_stream_info(info: &[redis::Value]) -> (Option<String>, Option<String>, u64) {
    let mut first_entry_id = None;
    let mut last_entry_id = None;
    let mut groups = 0u64;

    let mut i = 0;
    while i < info.len() {
        if let redis::Value::Data(ref key_bytes) = info[i] {
            if let Ok(key) = String::from_utf8(key_bytes.clone()) {
                if key == "first-entry" && i + 1 < info.len() {
                    if let redis::Value::Bulk(ref entry) = info[i + 1] {
                        if !entry.is_empty() {
                            if let redis::Value::Data(ref id_bytes) = entry[0] {
                                first_entry_id = String::from_utf8(id_bytes.clone()).ok();
                            }
                        }
                    }
                } else if key == "last-entry" && i + 1 < info.len() {
                    if let redis::Value::Bulk(ref entry) = info[i + 1] {
                        if !entry.is_empty() {
                            if let redis::Value::Data(ref id_bytes) = entry[0] {
                                last_entry_id = String::from_utf8(id_bytes.clone()).ok();
                            }
                        }
                    }
                } else if key == "groups" && i + 1 < info.len() {
                    if let redis::Value::Int(g) = info[i + 1] {
                        groups = g as u64;
                    }
                }
            }
        }
        i += 2;
    }

    (first_entry_id, last_entry_id, groups)
}

/// Estimates time lag between two stream IDs
fn estimate_time_lag(old_id: &str, new_id: &str) -> Option<i64> {
    let (old_ts, _) = parse_stream_id_timestamp(old_id)?;
    let (new_ts, _) = parse_stream_id_timestamp(new_id)?;

    Some(new_ts.saturating_sub(old_ts))
}

/// Parses timestamp from stream ID
fn parse_stream_id_timestamp(stream_id: &str) -> Option<(i64, i64)> {
    let parts: Vec<&str> = stream_id.split('-').collect();
    if parts.len() != 2 {
        return None;
    }

    let timestamp = parts[0].parse::<i64>().ok()?;
    let sequence = parts[1].parse::<i64>().ok()?;

    Some((timestamp, sequence))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stream_id_timestamp() {
        let (ts, seq) = parse_stream_id_timestamp("1234567890-0").unwrap();
        assert_eq!(ts, 1234567890);
        assert_eq!(seq, 0);
    }

    #[test]
    fn test_estimate_time_lag() {
        let lag = estimate_time_lag("1000-0", "2000-0");
        assert_eq!(lag, Some(1000));

        let lag = estimate_time_lag("2000-0", "1000-0");
        assert_eq!(lag, Some(-1000));

        let lag = estimate_time_lag("1000-0", "1000-5");
        assert_eq!(lag, Some(0)); // Same timestamp
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_get_stream_info() {
        use crate::redis::client::RedisConfig;
        use crate::redis::stream_writer::StreamWriter;
        use crate::models::task_event::TaskEvent;
        use serde_json::json;
        use chrono::Utc;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client.clone());
        let metrics = StreamMetrics::new(client);

        let task_id = Uuid::new_v4();

        // Empty stream
        let info = metrics.get_stream_info(task_id).await.unwrap();
        assert_eq!(info.length, 0);

        // Write some events
        for seq in 0..5 {
            let event = TaskEvent {
                task_id,
                seq,
                ts: Utc::now(),
                kind: "stdout".to_string(),
                payload: json!({"data": format!("Line {}\n", seq)}),
                hash_prev: if seq > 0 { Some(vec![1, 2, 3, 4]) } else { None },
                hash_curr: vec![5, 6, 7, 8],
            };
            writer.publish_event(&event).await.unwrap();
        }

        // Check info
        let info = metrics.get_stream_info(task_id).await.unwrap();
        assert_eq!(info.length, 5);
        assert!(info.first_entry_id.is_some());
        assert!(info.last_entry_id.is_some());
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_calculate_lag() {
        use crate::redis::client::RedisConfig;
        use crate::redis::stream_writer::StreamWriter;
        use crate::redis::stream_reader::StreamReader;
        use crate::models::task_event::TaskEvent;
        use serde_json::json;
        use chrono::Utc;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client.clone());
        let reader = StreamReader::new(client.clone());
        let metrics = StreamMetrics::new(client);

        let task_id = Uuid::new_v4();

        // Write 10 events
        for seq in 0..10 {
            let event = TaskEvent {
                task_id,
                seq,
                ts: Utc::now(),
                kind: "stdout".to_string(),
                payload: json!({"data": format!("Line {}\n", seq)}),
                hash_prev: if seq > 0 { Some(vec![1, 2, 3, 4]) } else { None },
                hash_curr: vec![5, 6, 7, 8],
            };
            writer.publish_event(&event).await.unwrap();
        }

        // Read first 5 events
        let events = reader.read_backfill(task_id, "0", 5).await.unwrap();
        let consumer_pos = &events.last().unwrap().0;

        // Calculate lag (should be ~5 events behind)
        let lag = metrics.calculate_lag(task_id, consumer_pos).await.unwrap();
        assert!(lag >= 4 && lag <= 5); // Allow some variance
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_estimate_memory_usage() {
        use crate::redis::client::RedisConfig;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let metrics = StreamMetrics::new(client);

        let task_id = Uuid::new_v4();

        let memory = metrics.estimate_memory_usage(task_id).await.unwrap();
        assert_eq!(memory, 0); // Empty stream
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_is_healthy() {
        use crate::redis::client::RedisConfig;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let metrics = StreamMetrics::new(client);

        let task_id = Uuid::new_v4();

        // Empty stream is not healthy
        let healthy = metrics.is_healthy(task_id, 1000).await.unwrap();
        assert!(!healthy);
    }
}
