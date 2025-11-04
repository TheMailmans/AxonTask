/// Database connection pool management
///
/// This module provides a production-grade PostgreSQL connection pool using sqlx.
/// It includes health checks, automatic reconnection, and proper error handling.
///
/// # Example
///
/// ```no_run
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = DatabaseConfig {
///         url: "postgresql://user:pass@localhost/db".to_string(),
///         max_connections: 10,
///         min_connections: 2,
///         connect_timeout_seconds: 30,
///         idle_timeout_seconds: Some(600),
///         max_lifetime_seconds: Some(1800),
///         test_before_acquire: true,
///     };
///
///     let pool = create_pool(config).await?;
///
///     // Use the pool
///     let row: (i64,) = sqlx::query_as("SELECT $1")
///         .bind(42i64)
///         .fetch_one(&pool)
///         .await?;
///
///     Ok(())
/// }
/// ```

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Configuration for the database connection pool
///
/// All timeouts are specified in seconds for ease of configuration from environment variables.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// PostgreSQL connection URL (e.g., "postgresql://user:pass@localhost:5432/dbname")
    pub url: String,

    /// Maximum number of connections in the pool
    ///
    /// Default: 10 (suitable for most applications)
    /// Production: Set based on database max_connections and number of instances
    pub max_connections: u32,

    /// Minimum number of idle connections to maintain
    ///
    /// Default: 2
    /// Setting this > 0 ensures faster response times by keeping connections warm
    pub min_connections: u32,

    /// Timeout for acquiring a connection from the pool (seconds)
    ///
    /// Default: 30 seconds
    /// If all connections are in use, requests will wait this long before timing out
    pub connect_timeout_seconds: u64,

    /// How long a connection can remain idle before being closed (seconds)
    ///
    /// Default: Some(600) (10 minutes)
    /// None = connections never closed due to idle time
    pub idle_timeout_seconds: Option<u64>,

    /// Maximum lifetime of a connection before forced recycling (seconds)
    ///
    /// Default: Some(1800) (30 minutes)
    /// None = connections live forever (not recommended in production)
    /// Prevents issues from long-lived connections (memory leaks, stale state)
    pub max_lifetime_seconds: Option<u64>,

    /// Whether to test connections before returning them from the pool
    ///
    /// Default: true
    /// Adds slight latency but ensures connections are always healthy
    pub test_before_acquire: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            max_connections: 10,
            min_connections: 2,
            connect_timeout_seconds: 30,
            idle_timeout_seconds: Some(600),
            max_lifetime_seconds: Some(1800),
            test_before_acquire: true,
        }
    }
}

/// Creates and initializes a PostgreSQL connection pool
///
/// This function:
/// 1. Creates a pool with the specified configuration
/// 2. Performs a health check to verify database connectivity
/// 3. Returns an error if the database is unreachable
///
/// # Errors
///
/// Returns an error if:
/// - The database URL is invalid
/// - Cannot connect to the database
/// - Health check fails
///
/// # Example
///
/// ```no_run
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
///
/// # async fn example() -> Result<(), sqlx::Error> {
/// let config = DatabaseConfig {
///     url: std::env::var("DATABASE_URL").unwrap(),
///     ..Default::default()
/// };
///
/// let pool = create_pool(config).await?;
/// # Ok(())
/// # }
/// ```
pub async fn create_pool(config: DatabaseConfig) -> Result<PgPool, sqlx::Error> {
    info!(
        max_connections = config.max_connections,
        min_connections = config.min_connections,
        connect_timeout_seconds = config.connect_timeout_seconds,
        "Creating database connection pool"
    );

    // Build pool with configuration
    let mut pool_options = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.connect_timeout_seconds))
        .test_before_acquire(config.test_before_acquire);

    // Set optional timeouts
    if let Some(idle_timeout) = config.idle_timeout_seconds {
        pool_options = pool_options.idle_timeout(Duration::from_secs(idle_timeout));
        debug!(idle_timeout_seconds = idle_timeout, "Set idle timeout");
    }

    if let Some(max_lifetime) = config.max_lifetime_seconds {
        pool_options = pool_options.max_lifetime(Duration::from_secs(max_lifetime));
        debug!(max_lifetime_seconds = max_lifetime, "Set max lifetime");
    }

    // Create the pool
    let pool = pool_options.connect(&config.url).await?;

    // Perform health check
    health_check(&pool).await?;

    info!("Database connection pool created successfully");
    Ok(pool)
}

/// Performs a health check on the database connection
///
/// Executes a simple query to verify the database is reachable and responding.
///
/// # Errors
///
/// Returns an error if the health check query fails
///
/// # Example
///
/// ```no_run
/// use axontask_shared::db::pool::health_check;
/// use sqlx::PgPool;
///
/// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
/// health_check(&pool).await?;
/// println!("Database is healthy");
/// # Ok(())
/// # }
/// ```
pub async fn health_check(pool: &PgPool) -> Result<(), sqlx::Error> {
    debug!("Performing database health check");

    let result: (i32,) = sqlx::query_as("SELECT 1")
        .fetch_one(pool)
        .await?;

    if result.0 == 1 {
        debug!("Database health check passed");
        Ok(())
    } else {
        warn!("Database health check returned unexpected value: {}", result.0);
        Err(sqlx::Error::Protocol(
            "Health check returned unexpected value".into(),
        ))
    }
}

/// Gets current pool statistics for monitoring
///
/// Returns information about the pool's current state including:
/// - Number of active (in-use) connections
/// - Number of idle connections
/// - Total connections
///
/// # Example
///
/// ```no_run
/// use axontask_shared::db::pool::get_pool_stats;
/// use sqlx::PgPool;
///
/// # async fn example(pool: PgPool) {
/// let stats = get_pool_stats(&pool);
/// println!("Pool stats: {:?}", stats);
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Number of connections currently in use
    pub active_connections: usize,

    /// Number of idle connections available
    pub idle_connections: usize,

    /// Total connections in the pool
    pub total_connections: usize,
}

pub fn get_pool_stats(pool: &PgPool) -> PoolStats {
    let size = pool.size();
    let idle = pool.num_idle();

    PoolStats {
        active_connections: size.saturating_sub(idle) as usize,
        idle_connections: idle as usize,
        total_connections: size as usize,
    }
}

/// Gracefully closes the connection pool
///
/// This should be called during application shutdown to ensure all connections
/// are properly closed and resources are released.
///
/// # Example
///
/// ```no_run
/// use axontask_shared::db::pool::close_pool;
/// use sqlx::PgPool;
///
/// # async fn example(pool: PgPool) {
/// close_pool(pool).await;
/// println!("Pool closed gracefully");
/// # }
/// ```
pub async fn close_pool(pool: PgPool) {
    info!("Closing database connection pool");
    pool.close().await;
    info!("Database connection pool closed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_config_default() {
        let config = DatabaseConfig::default();
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.min_connections, 2);
        assert_eq!(config.connect_timeout_seconds, 30);
        assert_eq!(config.idle_timeout_seconds, Some(600));
        assert_eq!(config.max_lifetime_seconds, Some(1800));
        assert!(config.test_before_acquire);
    }

    #[test]
    fn test_database_config_clone() {
        let config = DatabaseConfig::default();
        let cloned = config.clone();
        assert_eq!(config.max_connections, cloned.max_connections);
        assert_eq!(config.url, cloned.url);
    }

    // Integration tests require a running database
    // These are in the tests/ directory and run with `cargo test --test '*'`
}
