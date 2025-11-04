/// Gap detection for handling clients that fall too far behind
///
/// When Redis Streams are trimmed (compaction), clients that haven't read
/// old events will find gaps. This module detects gaps and provides utilities
/// to generate summary events to catch clients up.
///
/// # Problem
///
/// ```text
/// Redis Stream (after XTRIM):
/// [1000-0] [1001-0] [1002-0] ... [5000-0]
///          ↑
///          Client cursor at 500-0 (doesn't exist anymore!)
/// ```
///
/// # Solution
///
/// 1. Detect gap: Check if client's cursor exists in stream
/// 2. Find earliest available event
/// 3. Generate digest event summarizing missing events
/// 4. Client continues from earliest available event
///
/// # Example
///
/// ```no_run
/// use axontask_shared::redis::client::{RedisClient, RedisConfig};
/// use axontask_shared::redis::gap_detection::GapDetector;
/// use uuid::Uuid;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = RedisConfig::from_env()?;
/// let client = RedisClient::new(config).await?;
/// let detector = GapDetector::new(client);
///
/// let task_id = Uuid::new_v4();
/// let client_cursor = "500-0";
///
/// // Check for gap
/// if let Some(gap_info) = detector.detect_gap(task_id, client_cursor).await? {
///     println!("Gap detected! Missing {} events", gap_info.estimated_missing_count);
///     println!("Resume from: {}", gap_info.earliest_available_id);
/// }
/// # Ok(())
/// # }
/// ```

use crate::events::serialization::event_stream_key;
use crate::redis::client::{RedisClient, RedisClientError};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Gap detection errors
#[derive(Error, Debug)]
pub enum GapDetectionError {
    /// Redis client error
    #[error("Redis error: {0}")]
    RedisError(#[from] RedisClientError),

    /// Raw Redis error
    #[error("Redis command error: {0}")]
    RedisCommandError(#[from] redis::RedisError),

    /// Invalid stream ID format
    #[error("Invalid stream ID: {0}")]
    InvalidStreamId(String),
}

/// Information about a detected gap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapInfo {
    /// The client's cursor that no longer exists
    pub client_cursor: String,

    /// The earliest available stream ID
    pub earliest_available_id: String,

    /// The latest available stream ID
    pub latest_available_id: String,

    /// Estimated number of events between client cursor and earliest available
    ///
    /// This is a rough estimate based on stream ID timestamps.
    pub estimated_missing_count: u64,

    /// Whether the stream has been compacted
    pub compacted: bool,
}

/// Configuration for gap detection
#[derive(Debug, Clone)]
pub struct GapDetectorConfig {
    /// Number of events to sample when estimating missing count
    pub sample_size: usize,
}

impl Default for GapDetectorConfig {
    fn default() -> Self {
        Self { sample_size: 100 }
    }
}

/// Gap detector for handling clients that fall behind
#[derive(Clone)]
pub struct GapDetector {
    client: RedisClient,
    config: GapDetectorConfig,
}

impl GapDetector {
    /// Creates a new gap detector with default configuration
    ///
    /// # Arguments
    ///
    /// * `client` - Redis client to use
    pub fn new(client: RedisClient) -> Self {
        Self {
            client,
            config: GapDetectorConfig::default(),
        }
    }

    /// Creates a new gap detector with custom configuration
    pub fn with_config(client: RedisClient, config: GapDetectorConfig) -> Self {
        Self { client, config }
    }

    /// Detects if a gap exists for a given client cursor
    ///
    /// A gap exists if:
    /// - The client's cursor stream ID doesn't exist in the stream
    /// - The cursor is older than the earliest available event
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `client_cursor` - Client's last known stream ID
    ///
    /// # Returns
    ///
    /// Some(GapInfo) if gap detected, None if client is up to date
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::gap_detection::GapDetector;
    /// # use uuid::Uuid;
    /// # async fn example(detector: &GapDetector) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    ///
    /// if let Some(gap_info) = detector.detect_gap(task_id, "1000-0").await? {
    ///     println!("Gap detected! Resume from {}", gap_info.earliest_available_id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn detect_gap(
        &self,
        task_id: Uuid,
        client_cursor: &str,
    ) -> Result<Option<GapInfo>, GapDetectionError> {
        let stream_key = event_stream_key(task_id);
        let mut conn = self.client.get_connection();

        // Get stream info to find earliest and latest IDs
        let stream_info: Vec<redis::Value> = redis::cmd("XINFO")
            .arg("STREAM")
            .arg(&stream_key)
            .query_async(&mut conn)
            .await?;

        // Parse XINFO response to get first and last entry IDs
        let (earliest_id, latest_id) = parse_xinfo_stream(&stream_info)?;

        // If stream is empty, no gap
        if earliest_id.is_empty() {
            return Ok(None);
        }

        // Check if client cursor exists (would be >= earliest_id)
        if Self::compare_stream_ids(client_cursor, &earliest_id)? >= 0 {
            // Client cursor is within valid range, no gap
            return Ok(None);
        }

        // Gap detected: client cursor is older than earliest available
        let estimated_missing = self
            .estimate_missing_count(&earliest_id, client_cursor)
            .await;

        tracing::warn!(
            task_id = %task_id,
            client_cursor = %client_cursor,
            earliest_id = %earliest_id,
            estimated_missing = estimated_missing,
            "Gap detected in event stream"
        );

        Ok(Some(GapInfo {
            client_cursor: client_cursor.to_string(),
            earliest_available_id: earliest_id.clone(),
            latest_available_id: latest_id,
            estimated_missing_count: estimated_missing,
            compacted: true,
        }))
    }

    /// Checks if a specific stream ID exists in the stream
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `stream_id` - Stream ID to check
    ///
    /// # Returns
    ///
    /// True if the stream ID exists, false otherwise
    pub async fn stream_id_exists(
        &self,
        task_id: Uuid,
        stream_id: &str,
    ) -> Result<bool, GapDetectionError> {
        let stream_key = event_stream_key(task_id);
        let mut conn = self.client.get_connection();

        // Try to read that specific entry
        let result: Vec<redis::Value> = redis::cmd("XRANGE")
            .arg(&stream_key)
            .arg(stream_id)
            .arg(stream_id)
            .arg("COUNT")
            .arg(1)
            .query_async(&mut conn)
            .await?;

        Ok(!result.is_empty())
    }

    /// Gets the earliest available stream ID
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    ///
    /// # Returns
    ///
    /// The earliest stream ID, or None if stream is empty
    pub async fn get_earliest_id(
        &self,
        task_id: Uuid,
    ) -> Result<Option<String>, GapDetectionError> {
        let stream_key = event_stream_key(task_id);
        let mut conn = self.client.get_connection();

        let result: Vec<redis::Value> = redis::cmd("XRANGE")
            .arg(&stream_key)
            .arg("-")
            .arg("+")
            .arg("COUNT")
            .arg(1)
            .query_async(&mut conn)
            .await?;

        if result.is_empty() {
            return Ok(None);
        }

        // Parse first entry to get stream ID
        if let redis::Value::Bulk(ref entries) = result[0] {
            if let redis::Value::Bulk(ref entry_parts) = entries[0] {
                if let redis::Value::Data(ref id_bytes) = entry_parts[0] {
                    if let Ok(id) = String::from_utf8(id_bytes.clone()) {
                        return Ok(Some(id));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Compares two Redis Stream IDs
    ///
    /// Redis Stream IDs are in format "timestamp-sequence"
    ///
    /// # Returns
    ///
    /// - Negative: id1 < id2
    /// - Zero: id1 == id2
    /// - Positive: id1 > id2
    fn compare_stream_ids(id1: &str, id2: &str) -> Result<i64, GapDetectionError> {
        let (ts1, seq1) = parse_stream_id(id1)?;
        let (ts2, seq2) = parse_stream_id(id2)?;

        if ts1 != ts2 {
            Ok(ts1 - ts2)
        } else {
            Ok(seq1 - seq2)
        }
    }

    /// Estimates number of missing events based on stream ID timestamps
    ///
    /// This is a rough estimate and may not be accurate.
    async fn estimate_missing_count(&self, earliest_id: &str, client_cursor: &str) -> u64 {
        // Parse timestamps from stream IDs
        let (earliest_ts, _) = match parse_stream_id(earliest_id) {
            Ok(parsed) => parsed,
            Err(_) => return 0,
        };

        let (client_ts, _) = match parse_stream_id(client_cursor) {
            Ok(parsed) => parsed,
            Err(_) => return 0,
        };

        // Time difference in milliseconds
        let time_diff = earliest_ts.saturating_sub(client_ts) as u64;

        // Rough estimate: assume ~10 events per second on average
        // This is a very rough heuristic
        let estimated = (time_diff / 100).max(1);

        estimated
    }
}

/// Parses Redis Stream ID into (timestamp, sequence)
fn parse_stream_id(stream_id: &str) -> Result<(i64, i64), GapDetectionError> {
    let parts: Vec<&str> = stream_id.split('-').collect();
    if parts.len() != 2 {
        return Err(GapDetectionError::InvalidStreamId(stream_id.to_string()));
    }

    let timestamp = parts[0]
        .parse::<i64>()
        .map_err(|_| GapDetectionError::InvalidStreamId(stream_id.to_string()))?;

    let sequence = parts[1]
        .parse::<i64>()
        .map_err(|_| GapDetectionError::InvalidStreamId(stream_id.to_string()))?;

    Ok((timestamp, sequence))
}

/// Parses XINFO STREAM response to extract first and last entry IDs
fn parse_xinfo_stream(info: &[redis::Value]) -> Result<(String, String), GapDetectionError> {
    let mut first_entry_id = String::new();
    let mut last_entry_id = String::new();

    // XINFO STREAM returns a flat array of key-value pairs
    let mut i = 0;
    while i < info.len() {
        if let redis::Value::Data(ref key_bytes) = info[i] {
            if let Ok(key) = String::from_utf8(key_bytes.clone()) {
                if key == "first-entry" && i + 1 < info.len() {
                    if let redis::Value::Bulk(ref entry) = info[i + 1] {
                        if !entry.is_empty() {
                            if let redis::Value::Data(ref id_bytes) = entry[0] {
                                first_entry_id = String::from_utf8(id_bytes.clone())
                                    .unwrap_or_default();
                            }
                        }
                    }
                } else if key == "last-entry" && i + 1 < info.len() {
                    if let redis::Value::Bulk(ref entry) = info[i + 1] {
                        if !entry.is_empty() {
                            if let redis::Value::Data(ref id_bytes) = entry[0] {
                                last_entry_id =
                                    String::from_utf8(id_bytes.clone()).unwrap_or_default();
                            }
                        }
                    }
                }
            }
        }
        i += 2; // Key-value pairs
    }

    Ok((first_entry_id, last_entry_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stream_id() {
        let (ts, seq) = parse_stream_id("1234567890-0").unwrap();
        assert_eq!(ts, 1234567890);
        assert_eq!(seq, 0);

        let (ts, seq) = parse_stream_id("1234567890-42").unwrap();
        assert_eq!(ts, 1234567890);
        assert_eq!(seq, 42);
    }

    #[test]
    fn test_parse_invalid_stream_id() {
        assert!(parse_stream_id("invalid").is_err());
        assert!(parse_stream_id("1234567890").is_err());
        assert!(parse_stream_id("1234567890-abc").is_err());
    }

    #[test]
    fn test_compare_stream_ids() {
        // Earlier timestamp
        assert!(GapDetector::compare_stream_ids("1000-0", "2000-0").unwrap() < 0);

        // Later timestamp
        assert!(GapDetector::compare_stream_ids("2000-0", "1000-0").unwrap() > 0);

        // Same timestamp, different sequence
        assert!(GapDetector::compare_stream_ids("1000-0", "1000-1").unwrap() < 0);
        assert!(GapDetector::compare_stream_ids("1000-1", "1000-0").unwrap() > 0);

        // Equal
        assert_eq!(
            GapDetector::compare_stream_ids("1000-0", "1000-0").unwrap(),
            0
        );
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_detect_gap_no_gap() {
        use crate::redis::client::RedisConfig;
        use crate::redis::stream_writer::StreamWriter;
        use crate::models::task_event::TaskEvent;
        use serde_json::json;
        use chrono::Utc;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client.clone());
        let detector = GapDetector::new(client);

        let task_id = Uuid::new_v4();

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

        // Check with valid cursor (should be no gap)
        let gap = detector.detect_gap(task_id, "0-0").await.unwrap();
        assert!(gap.is_none());
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_stream_id_exists() {
        use crate::redis::client::RedisConfig;
        use crate::redis::stream_writer::StreamWriter;
        use crate::models::task_event::TaskEvent;
        use serde_json::json;
        use chrono::Utc;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client.clone());
        let detector = GapDetector::new(client);

        let task_id = Uuid::new_v4();

        // Write an event
        let event = TaskEvent {
            task_id,
            seq: 0,
            ts: Utc::now(),
            kind: "started".to_string(),
            payload: json!({}),
            hash_prev: None,
            hash_curr: vec![1, 2, 3, 4],
        };
        let stream_id = writer.publish_event(&event).await.unwrap();

        // Should exist
        assert!(detector.stream_id_exists(task_id, &stream_id).await.unwrap());

        // Should not exist
        assert!(!detector.stream_id_exists(task_id, "999999-0").await.unwrap());
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_get_earliest_id() {
        use crate::redis::client::RedisConfig;
        use crate::redis::stream_writer::StreamWriter;
        use crate::models::task_event::TaskEvent;
        use serde_json::json;
        use chrono::Utc;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let writer = StreamWriter::new(client.clone());
        let detector = GapDetector::new(client);

        let task_id = Uuid::new_v4();

        // Empty stream
        let earliest = detector.get_earliest_id(task_id).await.unwrap();
        assert!(earliest.is_none());

        // Write some events
        let first_id = writer
            .publish_event(&TaskEvent {
                task_id,
                seq: 0,
                ts: Utc::now(),
                kind: "started".to_string(),
                payload: json!({}),
                hash_prev: None,
                hash_curr: vec![1, 2, 3, 4],
            })
            .await
            .unwrap();

        writer
            .publish_event(&TaskEvent {
                task_id,
                seq: 1,
                ts: Utc::now(),
                kind: "progress".to_string(),
                payload: json!({}),
                hash_prev: Some(vec![1, 2, 3, 4]),
                hash_curr: vec![5, 6, 7, 8],
            })
            .await
            .unwrap();

        // Should return first event's ID
        let earliest = detector.get_earliest_id(task_id).await.unwrap();
        assert_eq!(earliest, Some(first_id));
    }
}
