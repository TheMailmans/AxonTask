/// User model and database operations
///
/// This module provides the User model and CRUD operations for managing user accounts.
/// Users can belong to multiple tenants via the Membership model.
///
/// # Schema
///
/// ```sql
/// CREATE TABLE users (
///     id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
///     email CITEXT NOT NULL UNIQUE,
///     email_verified BOOLEAN NOT NULL DEFAULT FALSE,
///     password_hash VARCHAR(255) NOT NULL,
///     name VARCHAR(255),
///     avatar_url VARCHAR(512),
///     created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
///     updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
///     last_login_at TIMESTAMPTZ
/// );
/// ```
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
/// // Create a new user
/// let new_user = CreateUser {
///     email: "user@example.com".to_string(),
///     password_hash: "$argon2id$...".to_string(),
///     name: Some("John Doe".to_string()),
///     avatar_url: None,
/// };
///
/// let user = User::create(&pool, new_user).await?;
/// println!("Created user: {}", user.id);
///
/// // Find by email
/// let found = User::find_by_email(&pool, "user@example.com").await?;
/// # Ok(())
/// # }
/// ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// User model representing a user account
///
/// Users can belong to multiple tenants via the memberships table.
/// Passwords are stored as Argon2id hashes, never in plaintext.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    /// Unique user ID (UUID v4)
    pub id: Uuid,

    /// Email address (case-insensitive via CITEXT)
    ///
    /// Must be unique across all users
    pub email: String,

    /// Whether the email address has been verified
    ///
    /// Set to true after email verification flow completes
    pub email_verified: bool,

    /// Argon2id password hash
    ///
    /// Never store plaintext passwords!
    /// Use `argon2` crate for hashing/verification
    pub password_hash: String,

    /// Optional display name
    pub name: Option<String>,

    /// Optional avatar/profile picture URL
    pub avatar_url: Option<String>,

    /// When the user account was created
    pub created_at: DateTime<Utc>,

    /// When the user account was last updated
    pub updated_at: DateTime<Utc>,

    /// When the user last logged in (None if never logged in)
    pub last_login_at: Option<DateTime<Utc>>,
}

/// Input for creating a new user
///
/// Email and password_hash are required. Name and avatar_url are optional.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUser {
    /// Email address (will be stored lowercase via CITEXT)
    pub email: String,

    /// Argon2id password hash (NOT plaintext password!)
    pub password_hash: String,

    /// Optional display name
    pub name: Option<String>,

    /// Optional avatar URL
    pub avatar_url: Option<String>,
}

/// Input for updating an existing user
///
/// All fields are optional. Only non-None fields will be updated.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateUser {
    /// New email address
    pub email: Option<String>,

    /// New password hash
    pub password_hash: Option<String>,

    /// New display name (use Some(None) to clear)
    pub name: Option<Option<String>>,

    /// New avatar URL (use Some(None) to clear)
    pub avatar_url: Option<Option<String>>,

    /// Update email verification status
    pub email_verified: Option<bool>,
}

impl User {
    /// Creates a new user in the database
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `data` - User creation data
    ///
    /// # Returns
    ///
    /// The newly created user with generated ID and timestamps
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Email already exists (unique constraint violation)
    /// - Database connection fails
    /// - Required fields are missing
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::user::{User, CreateUser};
    /// # use sqlx::PgPool;
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let new_user = CreateUser {
    ///     email: "user@example.com".to_string(),
    ///     password_hash: "$argon2id$...".to_string(),
    ///     name: Some("John Doe".to_string()),
    ///     avatar_url: None,
    /// };
    ///
    /// let user = User::create(&pool, new_user).await?;
    /// println!("Created user: {}", user.id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(pool: &PgPool, data: CreateUser) -> Result<Self, sqlx::Error> {
        let user = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (email, password_hash, name, avatar_url)
            VALUES ($1, $2, $3, $4)
            RETURNING id, email, email_verified, password_hash, name, avatar_url,
                      created_at, updated_at, last_login_at
            "#,
        )
        .bind(data.email)
        .bind(data.password_hash)
        .bind(data.name)
        .bind(data.avatar_url)
        .fetch_one(pool)
        .await?;

        Ok(user)
    }

    /// Finds a user by ID
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `id` - User ID to search for
    ///
    /// # Returns
    ///
    /// The user if found, None otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::user::User;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    /// if let Some(user) = User::find_by_id(&pool, user_id).await? {
    ///     println!("Found user: {}", user.email);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, email_verified, password_hash, name, avatar_url,
                   created_at, updated_at, last_login_at
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(user)
    }

    /// Finds a user by email address
    ///
    /// Email lookup is case-insensitive (via CITEXT column type).
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `email` - Email address to search for (case-insensitive)
    ///
    /// # Returns
    ///
    /// The user if found, None otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::user::User;
    /// # use sqlx::PgPool;
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let user = User::find_by_email(&pool, "user@example.com").await?;
    /// if let Some(u) = user {
    ///     println!("Found user: {}", u.id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<Self>, sqlx::Error> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, email_verified, password_hash, name, avatar_url,
                   created_at, updated_at, last_login_at
            FROM users
            WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(pool)
        .await?;

        Ok(user)
    }

    /// Updates an existing user
    ///
    /// Only non-None fields in `data` will be updated. The `updated_at` timestamp
    /// is automatically set to the current time.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `id` - ID of user to update
    /// * `data` - Fields to update (only non-None values are updated)
    ///
    /// # Returns
    ///
    /// The updated user if found, None if user doesn't exist
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Email already exists for another user
    /// - Database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::user::{User, UpdateUser};
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    /// let update = UpdateUser {
    ///     name: Some(Some("Jane Doe".to_string())),
    ///     email_verified: Some(true),
    ///     ..Default::default()
    /// };
    ///
    /// if let Some(user) = User::update(&pool, user_id, update).await? {
    ///     println!("Updated user: {}", user.email);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update(
        pool: &PgPool,
        id: Uuid,
        data: UpdateUser,
    ) -> Result<Option<Self>, sqlx::Error> {
        // Build dynamic update query based on which fields are present
        let mut query = String::from("UPDATE users SET updated_at = NOW()");
        let mut bind_count = 1;

        if data.email.is_some() {
            bind_count += 1;
            query.push_str(&format!(", email = ${}", bind_count));
        }
        if data.password_hash.is_some() {
            bind_count += 1;
            query.push_str(&format!(", password_hash = ${}", bind_count));
        }
        if let Some(ref name_opt) = data.name {
            bind_count += 1;
            query.push_str(&format!(", name = ${}", bind_count));
        }
        if let Some(ref avatar_opt) = data.avatar_url {
            bind_count += 1;
            query.push_str(&format!(", avatar_url = ${}", bind_count));
        }
        if data.email_verified.is_some() {
            bind_count += 1;
            query.push_str(&format!(", email_verified = ${}", bind_count));
        }

        query.push_str(" WHERE id = $1 RETURNING id, email, email_verified, password_hash, name, avatar_url, created_at, updated_at, last_login_at");

        let mut q = sqlx::query_as::<_, User>(&query).bind(id);

        if let Some(email) = data.email {
            q = q.bind(email);
        }
        if let Some(password_hash) = data.password_hash {
            q = q.bind(password_hash);
        }
        if let Some(name_opt) = data.name {
            q = q.bind(name_opt);
        }
        if let Some(avatar_opt) = data.avatar_url {
            q = q.bind(avatar_opt);
        }
        if let Some(verified) = data.email_verified {
            q = q.bind(verified);
        }

        let user = q.fetch_optional(pool).await?;

        Ok(user)
    }

    /// Deletes a user by ID
    ///
    /// ⚠️  **WARNING**: This permanently deletes the user account. Use with caution!
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `id` - ID of user to delete
    ///
    /// # Returns
    ///
    /// True if user was deleted, false if user didn't exist
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails or if foreign key constraints
    /// prevent deletion
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::user::User;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    /// let deleted = User::delete(&pool, user_id).await?;
    /// if deleted {
    ///     println!("User deleted");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Updates the last login timestamp for a user
    ///
    /// This is typically called after successful authentication.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `id` - ID of user who logged in
    ///
    /// # Returns
    ///
    /// True if user was found and updated, false otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::user::User;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    /// User::update_last_login(&pool, user_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_last_login(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE users
            SET last_login_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Lists all users with pagination
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `limit` - Maximum number of users to return
    /// * `offset` - Number of users to skip (for pagination)
    ///
    /// # Returns
    ///
    /// Vector of users, ordered by creation date (newest first)
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::user::User;
    /// # use sqlx::PgPool;
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// // Get first page (10 users)
    /// let page1 = User::list(&pool, 10, 0).await?;
    ///
    /// // Get second page
    /// let page2 = User::list(&pool, 10, 10).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<Self>, sqlx::Error> {
        let users = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, email_verified, password_hash, name, avatar_url,
                   created_at, updated_at, last_login_at
            FROM users
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok(users)
    }

    /// Counts total number of users
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    ///
    /// # Returns
    ///
    /// Total number of users in the database
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::user::User;
    /// # use sqlx::PgPool;
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let total = User::count(&pool).await?;
    /// println!("Total users: {}", total);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn count(pool: &PgPool) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(pool)
            .await?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_user_struct() {
        let create_user = CreateUser {
            email: "test@example.com".to_string(),
            password_hash: "hash".to_string(),
            name: Some("Test User".to_string()),
            avatar_url: None,
        };

        assert_eq!(create_user.email, "test@example.com");
        assert_eq!(create_user.password_hash, "hash");
    }

    #[test]
    fn test_update_user_default() {
        let update = UpdateUser::default();
        assert!(update.email.is_none());
        assert!(update.password_hash.is_none());
        assert!(update.name.is_none());
        assert!(update.avatar_url.is_none());
        assert!(update.email_verified.is_none());
    }

    // Integration tests for database operations are in tests/models/user_tests.rs
}
