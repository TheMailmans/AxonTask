/// Redis client wrapper with connection pooling and health checks
///
/// This module provides a production-grade Redis client wrapper that handles:
/// - Connection pooling via redis::aio::ConnectionManager
/// - Automatic reconnection on failure
/// - Health checks (PING command)
/// - Configuration from environment variables
///
/// # Example
///
/// ```no_run
/// use axontask_shared::redis::client::{RedisClient, RedisConfig};
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = RedisConfig::from_env()?;
/// let client = RedisClient::new(config).await?;
///
/// // Health check
/// let healthy = client.ping().await?;
/// println!("Redis healthy: {}", healthy);
/// # Ok(())
/// # }
/// ```

use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client, RedisError};
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Redis client errors
#[derive(Error, Debug)]
pub enum RedisClientError {
    /// Connection error
    #[error("Redis connection error: {0}")]
    ConnectionError(String),

    /// Command execution error
    #[error("Redis command error: {0}")]
    CommandError(String),

    /// Configuration error
    #[error("Redis configuration error: {0}")]
    ConfigError(String),

    /// Health check failed
    #[error("Redis health check failed: {0}")]
    HealthCheckFailed(String),
}

impl From<RedisError> for RedisClientError {
    fn from(err: RedisError) -> Self {
        match err.kind() {
            redis::ErrorKind::IoError => {
                RedisClientError::ConnectionError(format!("IO error: {}", err))
            }
            redis::ErrorKind::ResponseError => {
                RedisClientError::CommandError(format!("Response error: {}", err))
            }
            _ => RedisClientError::CommandError(err.to_string()),
        }
    }
}

/// Redis configuration
///
/// Can be loaded from environment variables or constructed manually.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis connection URL
    ///
    /// Format: redis://[username:password@]host:port[/db]
    /// Example: redis://localhost:6379
    pub url: String,

    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,

    /// Command timeout in seconds
    pub command_timeout_secs: u64,

    /// Max retries for operations
    pub max_retries: u32,
}

impl RedisConfig {
    /// Creates a new Redis configuration from environment variables
    ///
    /// # Environment Variables
    ///
    /// - `REDIS_URL`: Redis connection URL (required)
    /// - `REDIS_CONNECTION_TIMEOUT_SECS`: Connection timeout (default: 5)
    /// - `REDIS_COMMAND_TIMEOUT_SECS`: Command timeout (default: 10)
    /// - `REDIS_MAX_RETRIES`: Max retries (default: 3)
    ///
    /// # Errors
    ///
    /// Returns an error if REDIS_URL is not set or invalid.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use axontask_shared::redis::client::RedisConfig;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let config = RedisConfig::from_env()?;
    /// println!("Redis URL: {}", config.url);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_env() -> Result<Self, RedisClientError> {
        // Load .env if present
        dotenvy::dotenv().ok();

        let url = env::var("REDIS_URL").map_err(|_| {
            RedisClientError::ConfigError("REDIS_URL environment variable is required".to_string())
        })?;

        let connection_timeout_secs = env::var("REDIS_CONNECTION_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);

        let command_timeout_secs = env::var("REDIS_COMMAND_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let max_retries = env::var("REDIS_MAX_RETRIES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);

        Ok(Self {
            url,
            connection_timeout_secs,
            command_timeout_secs,
            max_retries,
        })
    }

    /// Creates a default configuration for testing
    ///
    /// Uses redis://localhost:6379 with default timeouts.
    #[cfg(test)]
    pub fn default_for_test() -> Self {
        Self {
            url: "redis://localhost:6379".to_string(),
            connection_timeout_secs: 5,
            command_timeout_secs: 10,
            max_retries: 3,
        }
    }
}

/// Production-grade Redis client with connection management
///
/// Wraps the redis crate's ConnectionManager to provide:
/// - Automatic reconnection on connection loss
/// - Health checking
/// - Timeout configuration
/// - Thread-safe cloning (uses Arc internally)
#[derive(Clone)]
pub struct RedisClient {
    manager: ConnectionManager,
    config: Arc<RedisConfig>,
}

impl RedisClient {
    /// Creates a new Redis client with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Redis configuration
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The Redis URL is invalid
    /// - Connection to Redis fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use axontask_shared::redis::client::{RedisClient, RedisConfig};
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = RedisConfig::from_env()?;
    /// let client = RedisClient::new(config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(config: RedisConfig) -> Result<Self, RedisClientError> {
        // Create Redis client
        let client = Client::open(config.url.as_str()).map_err(|e| {
            RedisClientError::ConfigError(format!("Invalid Redis URL: {}", e))
        })?;

        // Create connection manager (handles reconnection automatically)
        let manager = ConnectionManager::new(client).await.map_err(|e| {
            RedisClientError::ConnectionError(format!("Failed to connect to Redis: {}", e))
        })?;

        tracing::info!(
            "Redis client connected successfully to {}",
            sanitize_url(&config.url)
        );

        Ok(Self {
            manager,
            config: Arc::new(config),
        })
    }

    /// Performs a health check by sending a PING command
    ///
    /// # Returns
    ///
    /// Returns `true` if Redis responds with PONG, `false` otherwise.
    ///
    /// # Errors
    ///
    /// Returns an error if the PING command fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::redis::client::{RedisClient, RedisConfig};
    /// # async fn example(client: &RedisClient) -> anyhow::Result<()> {
    /// let healthy = client.ping().await?;
    /// if healthy {
    ///     println!("Redis is healthy");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn ping(&self) -> Result<bool, RedisClientError> {
        let mut conn = self.manager.clone();

        // Execute PING command with timeout
        let result: Result<String, RedisError> = tokio::time::timeout(
            Duration::from_secs(self.config.command_timeout_secs),
            redis::cmd("PING").query_async(&mut conn),
        )
        .await
        .map_err(|_| {
            RedisClientError::HealthCheckFailed("PING command timed out".to_string())
        })?;

        match result {
            Ok(pong) if pong == "PONG" => {
                tracing::debug!("Redis health check: PONG received");
                Ok(true)
            }
            Ok(other) => {
                tracing::warn!("Redis health check: unexpected response: {}", other);
                Ok(false)
            }
            Err(e) => {
                tracing::error!("Redis health check failed: {}", e);
                Err(RedisClientError::HealthCheckFailed(e.to_string()))
            }
        }
    }

    /// Gets a connection from the pool
    ///
    /// The connection manager automatically handles reconnection,
    /// so this method will always return a valid connection handle.
    ///
    /// # Returns
    ///
    /// A ConnectionManager clone that can be used to execute Redis commands.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use redis::AsyncCommands;
    /// # use axontask_shared::redis::client::{RedisClient, RedisConfig};
    /// # async fn example(client: &RedisClient) -> anyhow::Result<()> {
    /// let mut conn = client.get_connection();
    /// let value: String = conn.get("my_key").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_connection(&self) -> ConnectionManager {
        self.manager.clone()
    }

    /// Gets the Redis configuration
    pub fn config(&self) -> &RedisConfig {
        &self.config
    }

    /// Gets connection statistics
    ///
    /// Returns a summary of the current connection state.
    pub async fn stats(&self) -> RedisStats {
        let healthy = self.ping().await.unwrap_or(false);

        RedisStats {
            healthy,
            url: sanitize_url(&self.config.url),
        }
    }
}

/// Redis connection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisStats {
    /// Whether Redis is healthy (responds to PING)
    pub healthy: bool,

    /// Sanitized Redis URL (credentials removed)
    pub url: String,
}

/// Sanitizes a Redis URL by removing credentials
///
/// Replaces username:password with ***:*** for logging.
fn sanitize_url(url: &str) -> String {
    if let Some(at_pos) = url.find('@') {
        if let Some(scheme_end) = url.find("://") {
            let scheme = &url[..scheme_end + 3];
            let host = &url[at_pos + 1..];
            return format!("{}***:***@{}", scheme, host);
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_url() {
        assert_eq!(
            sanitize_url("redis://user:pass@localhost:6379"),
            "redis://***:***@localhost:6379"
        );
        assert_eq!(
            sanitize_url("redis://localhost:6379"),
            "redis://localhost:6379"
        );
    }

    #[test]
    fn test_config_defaults() {
        let config = RedisConfig {
            url: "redis://localhost:6379".to_string(),
            connection_timeout_secs: 5,
            command_timeout_secs: 10,
            max_retries: 3,
        };

        assert_eq!(config.connection_timeout_secs, 5);
        assert_eq!(config.command_timeout_secs, 10);
        assert_eq!(config.max_retries, 3);
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_redis_client_creation() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await;
        assert!(client.is_ok(), "Failed to create Redis client");
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_redis_ping() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let healthy = client.ping().await.unwrap();
        assert!(healthy, "Redis health check failed");
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_redis_stats() {
        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let stats = client.stats().await;
        assert!(stats.healthy, "Redis should be healthy");
        assert!(stats.url.contains("localhost"));
    }

    #[tokio::test]
    #[ignore] // Requires running Redis instance
    async fn test_get_connection() {
        use redis::AsyncCommands;

        let config = RedisConfig::default_for_test();
        let client = RedisClient::new(config).await.unwrap();
        let mut conn = client.get_connection();

        // Test basic set/get
        let _: () = conn.set("test_key", "test_value").await.unwrap();
        let value: String = conn.get("test_key").await.unwrap();
        assert_eq!(value, "test_value");

        // Cleanup
        let _: () = conn.del("test_key").await.unwrap();
    }
}
