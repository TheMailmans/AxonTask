/// Database models for AxonTask
///
/// This module contains all database models and their CRUD operations.
///
/// # Models
///
/// - `user`: User accounts and authentication
/// - `tenant`: Organizations/accounts for multi-tenancy (Task 1.4)
/// - `membership`: User-tenant relationships with roles (Task 1.5)
/// - `api_key`: API keys for programmatic access (Task 1.6)
/// - `task`: Background tasks (Task 1.7)
/// - `task_event`: Append-only event log with hash chaining (Task 1.8)
/// - `webhook`: Webhook configurations (Task 1.9)
/// - `usage`: Usage tracking for billing and quotas (Task 1.10)
///
/// # Example
///
/// ```no_run
/// use axontask_shared::models::user::{User, CreateUser};
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let pool = create_pool(DatabaseConfig::default()).await?;
///
/// let new_user = CreateUser {
///     email: "user@example.com".to_string(),
///     password_hash: "$argon2id$...".to_string(),
///     name: Some("John Doe".to_string()),
///     avatar_url: None,
/// };
///
/// let user = User::create(&pool, new_user).await?;
/// # Ok(())
/// # }
/// ```

pub mod user; // Phase 1, Task 1.3

// Will be added in subsequent tasks
// pub mod tenant;     // Task 1.4
// pub mod membership; // Task 1.5
// pub mod api_key;    // Task 1.6
// pub mod task;       // Task 1.7
// pub mod task_event; // Task 1.8
// pub mod webhook;    // Task 1.9
// pub mod usage;      // Task 1.10
