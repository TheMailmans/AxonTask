/// Database layer for AxonTask
///
/// This module provides database connection pooling, migrations, and models.
///
/// # Modules
///
/// - `pool`: PostgreSQL connection pool management with health checks
/// - `migrations`: Database migration runner (Phase 1, Task 1.2)
/// - Models are in the `models` module at crate root level
///
/// # Example
///
/// ```no_run
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = DatabaseConfig {
///         url: std::env::var("DATABASE_URL")?,
///         ..Default::default()
///     };
///
///     let pool = create_pool(config).await?;
///     Ok(())
/// }
/// ```

pub mod pool;
pub mod migrations; // Phase 1, Task 1.2
