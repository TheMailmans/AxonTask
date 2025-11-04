/// Redis-based heartbeat system for worker liveness tracking
///
/// This module provides a heartbeat mechanism to track worker liveness.
/// Workers send periodic heartbeats to Redis with a TTL, and if heartbeats
/// stop arriving, the task is considered orphaned and can be reclaimed.
///
/// # Architecture
///
/// ```text
/// Worker
///     │
///     │ Every 30s: SETEX hb:{task_id} 60 "{worker_id}:{timestamp}"
///     ▼
/// Redis
///     │
///     │ TTL expires after 60s
///     ▼
/// Watchdog detects orphaned task (>2 missed heartbeats)
/// ```
///
/// # Heartbeat Protocol
///
/// - **Key**: `hb:{task_id}`
/// - **Value**: JSON with worker_id and timestamp
/// - **TTL**: 60 seconds (2x heartbeat interval)
/// - **Interval**: Workers send heartbeat every 30 seconds
/// - **Threshold**: Task considered orphaned after 2 missed heartbeats (60s)
///
/// # Example
///
/// ```no_run
/// use axontask_shared::redis::client::{RedisClient, RedisConfig};
/// use axontask_shared::redis::heartbeat::{HeartbeatManager, HeartbeatData};
/// use uuid::Uuid;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = RedisConfig::from_env()?;
/// let client = RedisClient::new(config).await?;
/// let hb_manager = HeartbeatManager::new(client);
///
/// let task_id = Uuid::new_v4();
/// let worker_id = "worker-1".to_string();
///
/// // Send heartbeat
/// hb_manager.send_heartbeat(task_id, &worker_id).await?;
///
/// // Check if heartbeat is active
/// let is_alive = hb_manager.is_alive(task_id).await?;
/// println!("Task alive: {}", is_alive);
/// # Ok(())
/// # }
/// ```

use crate::events::serialization::heartbeat_key;
use crate::redis::client::{RedisClient, RedisClientError};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Heartbeat errors
#[derive(Error, Debug)]
pub enum HeartbeatError {
    /// Redis client error
    #[error("Redis error: {0}")]
    RedisError(#[from] RedisClientError),

    /// Raw Redis error
    #[error("Redis command error: {0}")]
    RedisCommandError(#[from] redis::RedisError),

    /// JSON serialization error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Invalid heartbeat data
    #[error("Invalid heartbeat data: {0}")]
    InvalidData(String),
}

/// Heartbeat data stored in Redis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatData {
    /// Worker ID that sent the heartbeat
    pub worker_id: String,

    /// Timestamp when heartbeat was sent
    pub timestamp: DateTime<Utc>,

    /// Optional additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl HeartbeatData {
    /// Creates new heartbeat data
    pub fn new(worker_id: String) -> Self {
        Self {
            worker_id,
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    /// Creates heartbeat data with metadata
    pub fn with_metadata(worker_id: String, metadata: serde_json::Value) -> Self {
        Self {
            worker_id,
            timestamp: Utc::now(),
            metadata: Some(metadata),
        }
    }

    /// Serializes to JSON string for Redis storage
    pub fn to_json(&self) -> Result<String, HeartbeatError> {
        Ok(serde_json::to_string(self)?)
    }

    /// Deserializes from JSON string
    pub fn from_json(json: &str) -> Result<Self, HeartbeatError> {
        serde_json::from_str(json).map_err(|e| HeartbeatError::InvalidData(e.to_string()))
    }
}

/// Configuration for heartbeat behavior
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Heartbeat TTL in seconds (Redis key expiration)
    ///
    /// Should be 2x the heartbeat interval to allow for 1 missed heartbeat.
    /// Default: 60 seconds
    pub ttl_seconds: u64,

    /// Recommended heartbeat interval in seconds
    ///
    /// Workers should send heartbeats at this frequency.
    /// Default: 30 seconds
    pub interval_seconds: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            ttl_seconds: 60,
            interval_seconds: 30,
        }
    }
}

/// Redis-based heartbeat manager
///
/// Manages worker heartbeats for task liveness tracking.
#[derive(Clone)]
pub struct HeartbeatManager {
    client: RedisClient,
    config: HeartbeatConfig,
}

impl HeartbeatManager {
    /// Creates a new heartbeat manager with default configuration
    ///
    /// # Arguments
    ///
    /// * `client` - Redis client to use
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::client::{RedisClient, RedisConfig};
    /// # use axontask_shared::redis::heartbeat::HeartbeatManager;
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = RedisConfig::from_env()?;
    /// let client = RedisClient::new(config).await?;
    /// let hb_manager = HeartbeatManager::new(client);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(client: RedisClient) -> Self {
        Self {
            client,
            config: HeartbeatConfig::default(),
        }
    }

    /// Creates a new heartbeat manager with custom configuration
    ///
    /// # Arguments
    ///
    /// * `client` - Redis client to use
    /// * `config` - Heartbeat configuration
    pub fn with_config(client: RedisClient, config: HeartbeatConfig) -> Self {
        Self { client, config }
    }

    /// Sends a heartbeat for a task
    ///
    /// Sets a Redis key with TTL to indicate worker is alive and processing this task.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `worker_id` - Worker ID sending the heartbeat
    ///
    /// # Returns
    ///
    /// Ok(()) if heartbeat was sent successfully
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::heartbeat::HeartbeatManager;
    /// # use uuid::Uuid;
    /// # async fn example(hb_manager: &HeartbeatManager) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    /// hb_manager.send_heartbeat(task_id, "worker-1").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_heartbeat(
        &self,
        task_id: Uuid,
        worker_id: &str,
    ) -> Result<(), HeartbeatError> {
        let key = heartbeat_key(task_id);
        let data = HeartbeatData::new(worker_id.to_string());
        let value = data.to_json()?;

        let mut conn = self.client.get_connection();

        // SETEX key ttl value
        conn.set_ex(&key, value, self.config.ttl_seconds)
            .await?;

        tracing::trace!(
            task_id = %task_id,
            worker_id = %worker_id,
            ttl = self.config.ttl_seconds,
            "Sent heartbeat"
        );

        Ok(())
    }

    /// Sends a heartbeat with custom metadata
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `worker_id` - Worker ID
    /// * `metadata` - Additional metadata (e.g., worker stats, host info)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::heartbeat::HeartbeatManager;
    /// # use uuid::Uuid;
    /// # use serde_json::json;
    /// # async fn example(hb_manager: &HeartbeatManager) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    /// let metadata = json!({"host": "worker-node-1", "load": 0.5});
    /// hb_manager.send_heartbeat_with_metadata(task_id, "worker-1", metadata).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_heartbeat_with_metadata(
        &self,
        task_id: Uuid,
        worker_id: &str,
        metadata: serde_json::Value,
    ) -> Result<(), HeartbeatError> {
        let key = heartbeat_key(task_id);
        let data = HeartbeatData::with_metadata(worker_id.to_string(), metadata);
        let value = data.to_json()?;

        let mut conn = self.client.get_connection();
        conn.set_ex(&key, value, self.config.ttl_seconds)
            .await?;

        tracing::trace!(
            task_id = %task_id,
            worker_id = %worker_id,
            "Sent heartbeat with metadata"
        );

        Ok(())
    }

    /// Checks if a task has an active heartbeat
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    ///
    /// # Returns
    ///
    /// True if heartbeat key exists (worker is alive), false otherwise
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::heartbeat::HeartbeatManager;
    /// # use uuid::Uuid;
    /// # async fn example(hb_manager: &HeartbeatManager) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    /// if !hb_manager.is_alive(task_id).await? {
    ///     println!("Task is orphaned!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn is_alive(&self, task_id: Uuid) -> Result<bool, HeartbeatError> {
        let key = heartbeat_key(task_id);
        let mut conn = self.client.get_connection();

        let exists: bool = conn.exists(&key).await?;

        Ok(exists)
    }

    /// Gets heartbeat data for a task
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    ///
    /// # Returns
    ///
    /// Some(HeartbeatData) if heartbeat exists, None otherwise
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::heartbeat::HeartbeatManager;
    /// # use uuid::Uuid;
    /// # async fn example(hb_manager: &HeartbeatManager) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    /// if let Some(data) = hb_manager.get_heartbeat(task_id).await? {
    ///     println!("Worker: {}, Last heartbeat: {}", data.worker_id, data.timestamp);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_heartbeat(
        &self,
        task_id: Uuid,
    ) -> Result<Option<HeartbeatData>, HeartbeatError> {
        let key = heartbeat_key(task_id);
        let mut conn = self.client.get_connection();

        let value: Option<String> = conn.get(&key).await?;

        match value {
            Some(json) => {
                let data = HeartbeatData::from_json(&json)?;
                Ok(Some(data))
            }
            None => Ok(None),
        }
    }

    /// Gets time until heartbeat expires
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    ///
    /// # Returns
    ///
    /// Remaining TTL in seconds, or None if key doesn't exist
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::heartbeat::HeartbeatManager;
    /// # use uuid::Uuid;
    /// # async fn example(hb_manager: &HeartbeatManager) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    /// if let Some(ttl) = hb_manager.get_ttl(task_id).await? {
    ///     println!("Heartbeat expires in {} seconds", ttl);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_ttl(&self, task_id: Uuid) -> Result<Option<i64>, HeartbeatError> {
        let key = heartbeat_key(task_id);
        let mut conn = self.client.get_connection();

        let ttl: i64 = conn.ttl(&key).await?;

        // Redis returns -2 if key doesn't exist, -1 if no expiration
        match ttl {
            -2 => Ok(None), // Key doesn't exist
            -1 => Ok(None), // No expiration (shouldn't happen in our case)
            seconds => Ok(Some(seconds)),
        }
    }

    /// Removes heartbeat for a task
    ///
    /// Used when task completes or is canceled.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    ///
    /// # Returns
    ///
    /// True if heartbeat was removed, false if it didn't exist
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::heartbeat::HeartbeatManager;
    /// # use uuid::Uuid;
    /// # async fn example(hb_manager: &HeartbeatManager) -> Result<(), Box<dyn std::error::Error>> {
    /// let task_id = Uuid::new_v4();
    /// hb_manager.remove_heartbeat(task_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn remove_heartbeat(&self, task_id: Uuid) -> Result<bool, HeartbeatError> {
        let key = heartbeat_key(task_id);
        let mut conn = self.client.get_connection();

        let deleted: u32 = conn.del(&key).await?;

        Ok(deleted > 0)
    }

    /// Gets recommended heartbeat interval
    ///
    /// Workers should send heartbeats at this frequency.
    pub fn heartbeat_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.config.interval_seconds)
    }

    /// Gets heartbeat TTL
    ///
    /// Returns how long a heartbeat is considered valid.
    pub fn heartbeat_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.config.ttl_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redis::client::RedisConfig;

    #[test]
    fn test_heartbeat_data_serialization() {
        let data = HeartbeatData::new("worker-1".to_string());
        let json = data.to_json().unwrap();
        let roundtrip = HeartbeatData::from_json(&json).unwrap();

        assert_eq!(data.worker_id, roundtrip.worker_id);
        assert_eq!(data.timestamp, roundtrip.timestamp);
    }

    #[test]
    fn test_heartbeat_config_default() {
        let config = HeartbeatConfig::default();
        assert_eq!(config.ttl_seconds, 60);
        assert_eq!(config.interval_seconds, 30);
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_send_heartbeat() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let hb_manager = HeartbeatManager::new(client);

        let task_id = Uuid::new_v4();
        let worker_id = "test-worker";

        // Send heartbeat
        hb_manager
            .send_heartbeat(task_id, worker_id)
            .await
            .unwrap();

        // Should be alive
        let is_alive = hb_manager.is_alive(task_id).await.unwrap();
        assert!(is_alive);

        // Should have data
        let data = hb_manager.get_heartbeat(task_id).await.unwrap();
        assert!(data.is_some());
        assert_eq!(data.unwrap().worker_id, worker_id);
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_heartbeat_expiration() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();

        // Use very short TTL for testing
        let hb_config = HeartbeatConfig {
            ttl_seconds: 1,
            interval_seconds: 1,
        };
        let hb_manager = HeartbeatManager::with_config(client, hb_config);

        let task_id = Uuid::new_v4();

        // Send heartbeat
        hb_manager
            .send_heartbeat(task_id, "test-worker")
            .await
            .unwrap();

        // Should be alive initially
        assert!(hb_manager.is_alive(task_id).await.unwrap());

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Should be dead now
        assert!(!hb_manager.is_alive(task_id).await.unwrap());
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_get_ttl() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let hb_manager = HeartbeatManager::new(client);

        let task_id = Uuid::new_v4();

        // No heartbeat yet
        let ttl = hb_manager.get_ttl(task_id).await.unwrap();
        assert!(ttl.is_none());

        // Send heartbeat
        hb_manager
            .send_heartbeat(task_id, "test-worker")
            .await
            .unwrap();

        // Should have TTL
        let ttl = hb_manager.get_ttl(task_id).await.unwrap();
        assert!(ttl.is_some());
        let ttl_secs = ttl.unwrap();
        assert!(ttl_secs > 0 && ttl_secs <= 60);
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_remove_heartbeat() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let hb_manager = HeartbeatManager::new(client);

        let task_id = Uuid::new_v4();

        // Send heartbeat
        hb_manager
            .send_heartbeat(task_id, "test-worker")
            .await
            .unwrap();

        // Should be alive
        assert!(hb_manager.is_alive(task_id).await.unwrap());

        // Remove heartbeat
        let removed = hb_manager.remove_heartbeat(task_id).await.unwrap();
        assert!(removed);

        // Should be dead
        assert!(!hb_manager.is_alive(task_id).await.unwrap());
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_heartbeat_with_metadata() {
        use serde_json::json;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let hb_manager = HeartbeatManager::new(client);

        let task_id = Uuid::new_v4();
        let metadata = json!({"host": "worker-node-1", "cpu": 0.75});

        // Send heartbeat with metadata
        hb_manager
            .send_heartbeat_with_metadata(task_id, "test-worker", metadata.clone())
            .await
            .unwrap();

        // Get heartbeat and verify metadata
        let data = hb_manager.get_heartbeat(task_id).await.unwrap().unwrap();
        assert_eq!(data.worker_id, "test-worker");
        assert_eq!(data.metadata, Some(metadata));
    }
}
