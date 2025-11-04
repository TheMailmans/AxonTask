/// Database migration runner
///
/// This module provides utilities for running and managing database migrations
/// using sqlx's migration system.
///
/// # Migration Files
///
/// Migrations are stored in the `migrations/` directory at the project root.
/// Each migration consists of two files:
/// - `{timestamp}_{name}.sql` - The "up" migration
/// - `{timestamp}_{name}.down.sql` - The "down" migration (rollback)
///
/// # Example
///
/// ```no_run
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
/// use axontask_shared::db::migrations::{run_migrations, get_migration_status};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = DatabaseConfig {
///         url: std::env::var("DATABASE_URL")?,
///         ..Default::default()
///     };
///
///     let pool = create_pool(config).await?;
///
///     // Run all pending migrations
///     run_migrations(&pool).await?;
///
///     // Check status
///     let status = get_migration_status(&pool).await?;
///     println!("Applied {} migrations", status.applied_migrations);
///
///     Ok(())
/// }
/// ```

use sqlx::{migrate::MigrateDatabase, postgres::PgPool, Postgres};
use tracing::{debug, info, warn};

/// Migration status information
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    /// Number of migrations that have been applied
    pub applied_migrations: usize,

    /// Latest applied migration version (timestamp)
    pub latest_version: Option<i64>,

    /// Whether the database schema is up to date
    pub is_up_to_date: bool,
}

/// Runs all pending database migrations
///
/// This function:
/// 1. Checks if migrations table exists (creates if needed)
/// 2. Runs all migrations that haven't been applied yet
/// 3. Returns an error if any migration fails
///
/// # Safety
///
/// Migrations are run in a transaction when possible. If a migration fails,
/// it will be rolled back and an error will be returned.
///
/// # Errors
///
/// Returns an error if:
/// - Cannot access the migrations directory
/// - A migration file is malformed
/// - A migration fails to execute
/// - Database connection is lost during migration
///
/// # Example
///
/// ```no_run
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
/// use axontask_shared::db::migrations::run_migrations;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let pool = create_pool(DatabaseConfig::default()).await?;
/// run_migrations(&pool).await?;
/// # Ok(())
/// # }
/// ```
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    info!("Starting database migrations");

    // Run migrations from the migrations/ directory
    let migrations = sqlx::migrate!("./migrations");

    match migrations.run(pool).await {
        Ok(()) => {
            info!("All database migrations completed successfully");
            Ok(())
        }
        Err(e) => {
            warn!("Migration failed: {}", e);
            Err(e)
        }
    }
}

/// Gets the current migration status
///
/// Returns information about which migrations have been applied and whether
/// the database is up to date.
///
/// # Errors
///
/// Returns an error if:
/// - Cannot query the migrations table
/// - Database connection fails
///
/// # Example
///
/// ```no_run
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
/// use axontask_shared::db::migrations::get_migration_status;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let pool = create_pool(DatabaseConfig::default()).await?;
/// let status = get_migration_status(&pool).await?;
/// println!("Applied migrations: {}", status.applied_migrations);
/// # Ok(())
/// # }
/// ```
pub async fn get_migration_status(pool: &PgPool) -> Result<MigrationStatus, sqlx::Error> {
    debug!("Checking migration status");

    // Check if migrations table exists
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT FROM information_schema.tables
            WHERE table_schema = 'public'
            AND table_name = '_sqlx_migrations'
        )",
    )
    .fetch_one(pool)
    .await?;

    if !table_exists {
        debug!("Migrations table does not exist yet");
        return Ok(MigrationStatus {
            applied_migrations: 0,
            latest_version: None,
            is_up_to_date: false,
        });
    }

    // Get the count and latest version of applied migrations
    let (count, latest_version): (i64, Option<i64>) = sqlx::query_as(
        "SELECT
            COUNT(*) as count,
            MAX(version) as latest_version
         FROM _sqlx_migrations
         WHERE success = true",
    )
    .fetch_one(pool)
    .await?;

    debug!(
        applied_migrations = count,
        latest_version = ?latest_version,
        "Migration status retrieved"
    );

    // Note: We can't easily determine if we're "up to date" without parsing
    // the migrations directory, so we'll just return the current state
    Ok(MigrationStatus {
        applied_migrations: count as usize,
        latest_version,
        is_up_to_date: count > 0, // Simplified: assume we're up to date if any migrations applied
    })
}

/// Creates the database if it doesn't exist
///
/// This is useful for development and testing. In production, the database
/// should already exist.
///
/// # Errors
///
/// Returns an error if:
/// - Cannot connect to PostgreSQL server
/// - Don't have permission to create databases
/// - Database creation fails
///
/// # Example
///
/// ```no_run
/// use axontask_shared::db::migrations::ensure_database_exists;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let database_url = "postgresql://user:pass@localhost:5432/mydb";
/// ensure_database_exists(database_url).await?;
/// # Ok(())
/// # }
/// ```
pub async fn ensure_database_exists(database_url: &str) -> Result<(), sqlx::Error> {
    info!("Checking if database exists");

    if !Postgres::database_exists(database_url).await? {
        info!("Database does not exist, creating it");
        Postgres::create_database(database_url).await?;
        info!("Database created successfully");
    } else {
        debug!("Database already exists");
    }

    Ok(())
}

/// Drops the database (USE WITH CAUTION!)
///
/// This function will delete the entire database and all its data.
/// Only use this in development/testing environments.
///
/// # Safety
///
/// ⚠️  **WARNING**: This function PERMANENTLY DELETES ALL DATA in the database.
/// Never use this in production!
///
/// # Errors
///
/// Returns an error if:
/// - Cannot connect to PostgreSQL server
/// - Don't have permission to drop databases
/// - Database is in use by other connections
///
/// # Example
///
/// ```no_run
/// use axontask_shared::db::migrations::drop_database;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let database_url = "postgresql://user:pass@localhost:5432/test_db";
/// // CAUTION: This will delete all data!
/// drop_database(database_url).await?;
/// # Ok(())
/// # }
/// ```
pub async fn drop_database(database_url: &str) -> Result<(), sqlx::Error> {
    warn!("⚠️  DROPPING DATABASE: {}", database_url);

    if Postgres::database_exists(database_url).await? {
        Postgres::drop_database(database_url).await?;
        info!("Database dropped successfully");
    } else {
        debug!("Database does not exist, nothing to drop");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_status_clone() {
        let status = MigrationStatus {
            applied_migrations: 5,
            latest_version: Some(20250103000000),
            is_up_to_date: true,
        };

        let cloned = status.clone();
        assert_eq!(status.applied_migrations, cloned.applied_migrations);
        assert_eq!(status.latest_version, cloned.latest_version);
        assert_eq!(status.is_up_to_date, cloned.is_up_to_date);
    }

    // Integration tests require a running database
    // These are in the tests/ directory
}
