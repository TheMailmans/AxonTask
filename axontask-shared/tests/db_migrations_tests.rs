/// Integration tests for database migrations
///
/// These tests require a running PostgreSQL database.
/// Run with: cargo test --test db_migrations_tests -- --test-threads=1
///
/// Database URL should be set via DATABASE_URL environment variable:
/// export DATABASE_URL="postgresql://axontask:axontask@localhost:5432/axontask_test"

use axontask_shared::db::migrations::{
    drop_database, ensure_database_exists, get_migration_status, run_migrations,
};
use axontask_shared::db::pool::{close_pool, create_pool, DatabaseConfig};
use std::env;

/// Helper to get test database URL
fn get_test_database_url() -> String {
    env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://axontask:axontask@localhost:5432/axontask_test".to_string())
}

#[tokio::test]
async fn test_ensure_database_exists() {
    let db_url = get_test_database_url();

    // This should succeed whether database exists or not
    let result = ensure_database_exists(&db_url).await;
    assert!(result.is_ok(), "Failed to ensure database exists: {:?}", result.err());
}

#[tokio::test]
async fn test_run_migrations() {
    let db_url = get_test_database_url();

    // Ensure database exists
    ensure_database_exists(&db_url).await.expect("Failed to create database");

    // Create pool
    let config = DatabaseConfig {
        url: db_url.clone(),
        ..Default::default()
    };
    let pool = create_pool(config).await.expect("Failed to create pool");

    // Run migrations
    let result = run_migrations(&pool).await;
    assert!(result.is_ok(), "Migrations failed: {:?}", result.err());

    // Verify migrations were applied
    let status = get_migration_status(&pool).await.expect("Failed to get migration status");
    assert!(status.applied_migrations > 0, "No migrations were applied");

    close_pool(pool).await;
}

#[tokio::test]
async fn test_migrations_are_idempotent() {
    let db_url = get_test_database_url();

    ensure_database_exists(&db_url).await.expect("Failed to create database");

    let config = DatabaseConfig {
        url: db_url.clone(),
        ..Default::default()
    };
    let pool = create_pool(config).await.expect("Failed to create pool");

    // Run migrations first time
    run_migrations(&pool).await.expect("First migration run failed");

    let status_1 = get_migration_status(&pool).await.expect("Failed to get status");

    // Run migrations again (should be a no-op)
    run_migrations(&pool).await.expect("Second migration run failed");

    let status_2 = get_migration_status(&pool).await.expect("Failed to get status");

    // Should have same number of migrations applied
    assert_eq!(
        status_1.applied_migrations, status_2.applied_migrations,
        "Migrations should be idempotent"
    );

    close_pool(pool).await;
}

#[tokio::test]
async fn test_get_migration_status_before_migrations() {
    let db_url = get_test_database_url();

    // Drop and recreate database to ensure clean state
    drop_database(&db_url).await.ok();
    ensure_database_exists(&db_url).await.expect("Failed to create database");

    let config = DatabaseConfig {
        url: db_url.clone(),
        ..Default::default()
    };
    let pool = create_pool(config).await.expect("Failed to create pool");

    // Get status before running migrations
    let status = get_migration_status(&pool).await.expect("Failed to get migration status");

    assert_eq!(status.applied_migrations, 0, "Should have 0 migrations before running");
    assert!(status.latest_version.is_none(), "Latest version should be None");

    close_pool(pool).await;
}

#[tokio::test]
async fn test_get_migration_status_after_migrations() {
    let db_url = get_test_database_url();

    ensure_database_exists(&db_url).await.expect("Failed to create database");

    let config = DatabaseConfig {
        url: db_url.clone(),
        ..Default::default()
    };
    let pool = create_pool(config).await.expect("Failed to create pool");

    // Run migrations
    run_migrations(&pool).await.expect("Migrations failed");

    // Get status after migrations
    let status = get_migration_status(&pool).await.expect("Failed to get migration status");

    assert!(status.applied_migrations > 0, "Should have migrations applied");
    assert!(status.latest_version.is_some(), "Latest version should be set");
    assert!(status.is_up_to_date, "Should be up to date after migrations");

    close_pool(pool).await;
}

#[tokio::test]
async fn test_migration_creates_all_tables() {
    let db_url = get_test_database_url();

    // Clean slate
    drop_database(&db_url).await.ok();
    ensure_database_exists(&db_url).await.expect("Failed to create database");

    let config = DatabaseConfig {
        url: db_url.clone(),
        ..Default::default()
    };
    let pool = create_pool(config).await.expect("Failed to create pool");

    // Run migrations
    run_migrations(&pool).await.expect("Migrations failed");

    // Verify all expected tables exist
    let expected_tables = vec![
        "tenants",
        "users",
        "memberships",
        "api_keys",
        "tasks",
        "task_events",
        "task_snapshots",
        "task_heartbeats",
        "webhooks",
        "webhook_deliveries",
        "usage_counters",
    ];

    for table_name in expected_tables {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT FROM information_schema.tables
                WHERE table_schema = 'public'
                AND table_name = $1
            )",
        )
        .bind(table_name)
        .fetch_one(&pool)
        .await
        .expect(&format!("Failed to check for table {}", table_name));

        assert!(exists, "Table '{}' should exist after migrations", table_name);
    }

    close_pool(pool).await;
}

#[tokio::test]
async fn test_migration_creates_enums() {
    let db_url = get_test_database_url();

    ensure_database_exists(&db_url).await.expect("Failed to create database");

    let config = DatabaseConfig {
        url: db_url.clone(),
        ..Default::default()
    };
    let pool = create_pool(config).await.expect("Failed to create pool");

    // Run migrations
    run_migrations(&pool).await.expect("Migrations failed");

    // Verify enum types exist
    let expected_enums = vec!["membership_role", "task_state"];

    for enum_name in expected_enums {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT FROM pg_type
                WHERE typname = $1
            )",
        )
        .bind(enum_name)
        .fetch_one(&pool)
        .await
        .expect(&format!("Failed to check for enum {}", enum_name));

        assert!(exists, "Enum '{}' should exist after migrations", enum_name);
    }

    close_pool(pool).await;
}

#[tokio::test]
async fn test_drop_database() {
    // Create a temporary test database
    let temp_db_url = "postgresql://axontask:axontask@localhost:5432/axontask_test_temp";

    // Ensure it exists
    ensure_database_exists(temp_db_url).await.ok();

    // Drop it
    let result = drop_database(temp_db_url).await;
    assert!(result.is_ok(), "Failed to drop database: {:?}", result.err());

    // Verify it's gone (this should fail to connect)
    let config = DatabaseConfig {
        url: temp_db_url.to_string(),
        connect_timeout_seconds: 2,
        ..Default::default()
    };

    let result = create_pool(config).await;
    assert!(result.is_err(), "Database should not exist after dropping");
}
