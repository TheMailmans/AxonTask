/// Redis integration for event streaming and caching
///
/// This module provides production-grade Redis integration including:
/// - Connection pooling with automatic reconnection
/// - Redis Streams for event fanout and replay
/// - Stream writer for publishing events
/// - Stream reader for backfill and live tailing
/// - Heartbeat system for worker liveness
/// - Gap detection and compaction
///
/// # Architecture
///
/// ```text
/// ┌─────────────┐
/// │   Worker    │ ──XADD──> events:{task_id}
/// └─────────────┘
///        │
///        │ SETEX
///        ▼
///   hb:{task_id}  (heartbeat, TTL 60s)
///        │
///        │ XREAD
///        ▼
/// ┌─────────────┐
/// │  API/Client │ <──backfill─── events:{task_id} (since_id)
/// └─────────────┘ <──live tail── events:{task_id} (BLOCK)
/// ```
///
/// # Example
///
/// ```no_run
/// use axontask_shared::redis::client::{RedisClient, RedisConfig};
///
/// # async fn example() -> anyhow::Result<()> {
/// // Create Redis client
/// let config = RedisConfig::from_env()?;
/// let client = RedisClient::new(config).await?;
///
/// // Health check
/// let healthy = client.ping().await?;
/// println!("Redis healthy: {}", healthy);
/// # Ok(())
/// # }
/// ```

pub mod client;
pub mod gap_detection;
pub mod heartbeat;
pub mod metrics;
pub mod stream_reader;
pub mod stream_writer;

// Re-export common types for convenience
pub use client::{RedisClient, RedisClientError, RedisConfig, RedisStats};
pub use gap_detection::{GapDetectionError, GapDetector, GapDetectorConfig, GapInfo};
pub use heartbeat::{HeartbeatConfig, HeartbeatData, HeartbeatError, HeartbeatManager};
pub use metrics::{EventRateStats, LagInfo, MetricsError, StreamInfo, StreamMetrics};
pub use stream_reader::{StreamReader, StreamReaderConfig, StreamReaderError};
pub use stream_writer::{StreamWriter, StreamWriterConfig, StreamWriterError};
