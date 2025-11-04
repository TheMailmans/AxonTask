/// Integration tests for database connection pool
///
/// These tests require a running PostgreSQL database.
/// Run with: cargo test --test db_pool_tests -- --test-threads=1
///
/// Database URL should be set via DATABASE_URL environment variable:
/// export DATABASE_URL="postgresql://axontask:axontask@localhost:5432/axontask_test"

use axontask_shared::db::pool::{close_pool, create_pool, get_pool_stats, health_check, DatabaseConfig};
use sqlx::Row;
use std::env;

/// Helper to get database URL from environment
fn get_test_database_url() -> String {
    env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://axontask:axontask@localhost:5432/axontask_test".to_string())
}

#[tokio::test]
async fn test_create_pool_success() {
    let config = DatabaseConfig {
        url: get_test_database_url(),
        max_connections: 5,
        min_connections: 1,
        connect_timeout_seconds: 10,
        idle_timeout_seconds: Some(60),
        max_lifetime_seconds: Some(300),
        test_before_acquire: true,
    };

    let result = create_pool(config).await;
    assert!(result.is_ok(), "Failed to create pool: {:?}", result.err());

    let pool = result.unwrap();

    // Verify pool was created
    let stats = get_pool_stats(&pool);
    assert!(stats.total_connections > 0, "Pool should have at least one connection");

    close_pool(pool).await;
}

#[tokio::test]
async fn test_create_pool_with_invalid_url() {
    let config = DatabaseConfig {
        url: "postgresql://invalid:invalid@nonexistent:5432/invalid".to_string(),
        max_connections: 1,
        min_connections: 0,
        connect_timeout_seconds: 2,
        idle_timeout_seconds: None,
        max_lifetime_seconds: None,
        test_before_acquire: false,
    };

    let result = create_pool(config).await;
    assert!(result.is_err(), "Should fail with invalid database URL");
}

#[tokio::test]
async fn test_health_check_success() {
    let config = DatabaseConfig {
        url: get_test_database_url(),
        ..Default::default()
    };

    let pool = create_pool(config).await.expect("Failed to create pool");

    let result = health_check(&pool).await;
    assert!(result.is_ok(), "Health check should succeed");

    close_pool(pool).await;
}

#[tokio::test]
async fn test_pool_query_execution() {
    let config = DatabaseConfig {
        url: get_test_database_url(),
        max_connections: 5,
        ..Default::default()
    };

    let pool = create_pool(config).await.expect("Failed to create pool");

    // Test simple query
    let row: (i64,) = sqlx::query_as("SELECT $1::bigint")
        .bind(42i64)
        .fetch_one(&pool)
        .await
        .expect("Failed to execute query");

    assert_eq!(row.0, 42);

    close_pool(pool).await;
}

#[tokio::test]
async fn test_pool_concurrent_queries() {
    let config = DatabaseConfig {
        url: get_test_database_url(),
        max_connections: 10,
        min_connections: 2,
        ..Default::default()
    };

    let pool = create_pool(config).await.expect("Failed to create pool");

    // Run 20 concurrent queries (more than pool size to test queueing)
    let mut handles = vec![];

    for i in 0..20 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            let row: (i64,) = sqlx::query_as("SELECT $1::bigint")
                .bind(i)
                .fetch_one(&pool_clone)
                .await
                .expect("Failed to execute query");

            assert_eq!(row.0, i);
        });
        handles.push(handle);
    }

    // Wait for all queries to complete
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    close_pool(pool).await;
}

#[tokio::test]
async fn test_get_pool_stats() {
    let config = DatabaseConfig {
        url: get_test_database_url(),
        max_connections: 5,
        min_connections: 2,
        ..Default::default()
    };

    let pool = create_pool(config).await.expect("Failed to create pool");

    // Get stats immediately after creation
    let stats = get_pool_stats(&pool);

    assert!(
        stats.total_connections >= 2,
        "Should have at least min_connections"
    );
    assert!(
        stats.total_connections <= 5,
        "Should not exceed max_connections"
    );

    // Acquire a connection to change stats
    let _conn = pool.acquire().await.expect("Failed to acquire connection");

    let stats_with_active = get_pool_stats(&pool);
    assert!(
        stats_with_active.active_connections > 0,
        "Should have at least one active connection"
    );

    close_pool(pool).await;
}

#[tokio::test]
async fn test_pool_connection_reuse() {
    let config = DatabaseConfig {
        url: get_test_database_url(),
        max_connections: 2,
        min_connections: 1,
        ..Default::default()
    };

    let pool = create_pool(config).await.expect("Failed to create pool");

    // Execute multiple queries sequentially
    for i in 0..10 {
        let row: (i64,) = sqlx::query_as("SELECT $1::bigint")
            .bind(i)
            .fetch_one(&pool)
            .await
            .expect("Failed to execute query");

        assert_eq!(row.0, i);
    }

    // Pool should still have connections (reused)
    let stats = get_pool_stats(&pool);
    assert!(stats.total_connections > 0);

    close_pool(pool).await;
}

#[tokio::test]
async fn test_pool_transaction() {
    let config = DatabaseConfig {
        url: get_test_database_url(),
        ..Default::default()
    };

    let pool = create_pool(config).await.expect("Failed to create pool");

    // Test transaction commit
    let mut tx = pool.begin().await.expect("Failed to begin transaction");

    let row: (i64,) = sqlx::query_as("SELECT 1::bigint")
        .fetch_one(&mut *tx)
        .await
        .expect("Failed to execute query in transaction");

    assert_eq!(row.0, 1);

    tx.commit().await.expect("Failed to commit transaction");

    // Test transaction rollback
    let mut tx = pool.begin().await.expect("Failed to begin transaction");

    let _: (i64,) = sqlx::query_as("SELECT 2::bigint")
        .fetch_one(&mut *tx)
        .await
        .expect("Failed to execute query in transaction");

    tx.rollback().await.expect("Failed to rollback transaction");

    close_pool(pool).await;
}

#[tokio::test]
async fn test_close_pool() {
    let config = DatabaseConfig {
        url: get_test_database_url(),
        ..Default::default()
    };

    let pool = create_pool(config).await.expect("Failed to create pool");

    // Close the pool
    close_pool(pool.clone()).await;

    // Attempting to use the pool after close should fail
    let result: Result<(i64,), _> = sqlx::query_as("SELECT 1::bigint")
        .fetch_one(&pool)
        .await;

    assert!(result.is_err(), "Queries should fail after pool is closed");
}

#[tokio::test]
async fn test_pool_exhaustion_timeout() {
    let config = DatabaseConfig {
        url: get_test_database_url(),
        max_connections: 2,
        min_connections: 0,
        connect_timeout_seconds: 2,  // Short timeout
        idle_timeout_seconds: None,
        max_lifetime_seconds: None,
        test_before_acquire: false,
    };

    let pool = create_pool(config).await.expect("Failed to create pool");

    // Acquire all available connections and hold them
    let _conn1 = pool.acquire().await.expect("Failed to acquire connection 1");
    let _conn2 = pool.acquire().await.expect("Failed to acquire connection 2");

    // Try to acquire a third connection (should timeout)
    let start = std::time::Instant::now();
    let result = pool.acquire().await;
    let elapsed = start.elapsed();

    assert!(result.is_err(), "Should timeout when pool is exhausted");
    assert!(
        elapsed.as_secs() >= 2 && elapsed.as_secs() <= 4,
        "Should timeout after approximately connect_timeout_seconds"
    );

    close_pool(pool).await;
}

#[tokio::test]
async fn test_database_config_defaults() {
    let mut config = DatabaseConfig::default();
    config.url = get_test_database_url();

    let pool = create_pool(config).await.expect("Failed to create pool with defaults");

    let stats = get_pool_stats(&pool);
    assert!(stats.total_connections > 0);

    close_pool(pool).await;
}
