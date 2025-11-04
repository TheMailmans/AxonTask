/// API Key model and database operations
///
/// This module provides the ApiKey model for programmatic access to the API.
/// API keys are an alternative to JWT tokens, suitable for server-to-server communication.
///
/// # Security
///
/// - Keys are stored as SHA-256 hashes (never plaintext)
/// - Keys are prefixed with "axon_" for identification
/// - Full key is only returned on creation (never again)
/// - Keys can be scoped to specific permissions
/// - Keys can be revoked or set to expire
///
/// # Schema
///
/// ```sql
/// CREATE TABLE api_keys (
///     id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
///     tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
///     name VARCHAR(255) NOT NULL,
///     key_prefix VARCHAR(10) NOT NULL,
///     key_hash VARCHAR(64) NOT NULL UNIQUE,
///     scopes TEXT[] NOT NULL DEFAULT ARRAY['read:task', 'write:task'],
///     created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
///     last_used_at TIMESTAMPTZ,
///     revoked BOOLEAN NOT NULL DEFAULT FALSE,
///     revoked_at TIMESTAMPTZ,
///     expires_at TIMESTAMPTZ
/// );
/// ```
///
/// # Example
///
/// ```no_run
/// use axontask_shared::models::api_key::{ApiKey, CreateApiKey};
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
/// use uuid::Uuid;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let pool = create_pool(DatabaseConfig::default()).await?;
/// let tenant_id = Uuid::new_v4();
///
/// // Create a new API key
/// let (api_key, plaintext_key) = ApiKey::create(&pool, CreateApiKey {
///     tenant_id,
///     name: "Production API Key".to_string(),
///     scopes: vec!["read:task".to_string(), "write:task".to_string()],
///     expires_at: None,
/// }).await?;
///
/// // IMPORTANT: Save plaintext_key now - it's never shown again!
/// println!("API Key: {}", plaintext_key);
/// # Ok(())
/// # }
/// ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// API Key model
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApiKey {
    /// Unique API key ID
    pub id: Uuid,

    /// Tenant this key belongs to
    pub tenant_id: Uuid,

    /// Human-readable name for the key
    pub name: String,

    /// First 10 characters of the key (for display: "axon_abc12...")
    pub key_prefix: String,

    /// SHA-256 hash of the full key (never store plaintext!)
    pub key_hash: String,

    /// Permission scopes (e.g., ["read:task", "write:task", "admin"])
    pub scopes: Vec<String>,

    /// When the key was created
    pub created_at: DateTime<Utc>,

    /// When the key was last used
    pub last_used_at: Option<DateTime<Utc>>,

    /// Whether the key has been revoked
    pub revoked: bool,

    /// When the key was revoked (if applicable)
    pub revoked_at: Option<DateTime<Utc>>,

    /// Optional expiration date
    pub expires_at: Option<DateTime<Utc>>,
}

/// Input for creating a new API key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKey {
    /// Tenant ID
    pub tenant_id: Uuid,

    /// Human-readable name
    pub name: String,

    /// Permission scopes
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,

    /// Optional expiration date
    pub expires_at: Option<DateTime<Utc>>,
}

fn default_scopes() -> Vec<String> {
    vec!["read:task".to_string(), "write:task".to_string()]
}

impl ApiKey {
    /// Generates a secure random API key
    ///
    /// Format: axon_{32_random_chars}
    ///
    /// # Example
    ///
    /// ```
    /// use axontask_shared::models::api_key::ApiKey;
    ///
    /// let key = ApiKey::generate_key();
    /// assert!(key.starts_with("axon_"));
    /// assert_eq!(key.len(), 37); // "axon_" (5) + 32 chars
    /// ```
    pub fn generate_key() -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut rng = rand::thread_rng();

        let random: String = (0..32)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();

        format!("axon_{}", random)
    }

    /// Hashes an API key with SHA-256
    ///
    /// # Example
    ///
    /// ```
    /// use axontask_shared::models::api_key::ApiKey;
    ///
    /// let key = "axon_abc123";
    /// let hash = ApiKey::hash_key(key);
    /// assert_eq!(hash.len(), 64); // SHA-256 hex is 64 chars
    /// ```
    pub fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Extracts the prefix from a key (first 10 chars)
    ///
    /// # Example
    ///
    /// ```
    /// use axontask_shared::models::api_key::ApiKey;
    ///
    /// let key = "axon_abc123xyz";
    /// let prefix = ApiKey::extract_prefix(&key);
    /// assert_eq!(prefix, "axon_abc12");
    /// ```
    pub fn extract_prefix(key: &str) -> String {
        key.chars().take(10).collect()
    }

    /// Checks if the API key is expired
    ///
    /// Returns true if expires_at is set and is in the past
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            expires_at < Utc::now()
        } else {
            false
        }
    }

    /// Creates a new API key
    ///
    /// Returns both the database record and the plaintext key.
    /// **IMPORTANT**: The plaintext key is only returned once and never stored!
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `data` - API key creation data
    ///
    /// # Returns
    ///
    /// Tuple of (ApiKey record, plaintext key string)
    ///
    /// # Errors
    ///
    /// Returns an error if database operation fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::api_key::{ApiKey, CreateApiKey};
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    /// let (key, plaintext) = ApiKey::create(&pool, CreateApiKey {
    ///     tenant_id,
    ///     name: "My API Key".to_string(),
    ///     scopes: vec!["read:task".to_string()],
    ///     expires_at: None,
    /// }).await?;
    ///
    /// // Store plaintext securely - it won't be shown again!
    /// println!("API Key: {}", plaintext);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(pool: &PgPool, data: CreateApiKey) -> Result<(Self, String), sqlx::Error> {
        // Generate key and hash it
        let plaintext_key = Self::generate_key();
        let key_hash = Self::hash_key(&plaintext_key);
        let key_prefix = Self::extract_prefix(&plaintext_key);

        let api_key = sqlx::query_as::<_, ApiKey>(
            r#"
            INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, tenant_id, name, key_prefix, key_hash, scopes, created_at,
                      last_used_at, revoked, revoked_at, expires_at
            "#,
        )
        .bind(data.tenant_id)
        .bind(data.name)
        .bind(key_prefix)
        .bind(key_hash)
        .bind(&data.scopes)
        .bind(data.expires_at)
        .fetch_one(pool)
        .await?;

        Ok((api_key, plaintext_key))
    }

    /// Finds an API key by ID
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let api_key = sqlx::query_as::<_, ApiKey>(
            r#"
            SELECT id, tenant_id, name, key_prefix, key_hash, scopes, created_at,
                   last_used_at, revoked, revoked_at, expires_at
            FROM api_keys
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(api_key)
    }

    /// Validates an API key and returns the key record if valid
    ///
    /// Checks:
    /// - Key hash matches
    /// - Not revoked
    /// - Not expired
    ///
    /// Also updates last_used_at timestamp if valid.
    pub async fn validate(pool: &PgPool, plaintext_key: &str) -> Result<Option<Self>, sqlx::Error> {
        let key_hash = Self::hash_key(plaintext_key);

        // Find and validate the key
        let api_key = sqlx::query_as::<_, ApiKey>(
            r#"
            UPDATE api_keys
            SET last_used_at = NOW()
            WHERE key_hash = $1
              AND revoked = FALSE
              AND (expires_at IS NULL OR expires_at > NOW())
            RETURNING id, tenant_id, name, key_prefix, key_hash, scopes, created_at,
                      last_used_at, revoked, revoked_at, expires_at
            "#,
        )
        .bind(key_hash)
        .fetch_optional(pool)
        .await?;

        Ok(api_key)
    }

    /// Revokes an API key
    pub async fn revoke(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE api_keys
            SET revoked = TRUE, revoked_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Revokes an API key with tenant isolation
    ///
    /// This ensures the API key belongs to the specified tenant before revoking.
    pub async fn revoke_with_tenant(pool: &PgPool, id: Uuid, tenant_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE api_keys
            SET revoked = TRUE, revoked_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Deletes an API key
    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM api_keys WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Lists all API keys for a tenant
    pub async fn list_by_tenant(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        let keys = sqlx::query_as::<_, ApiKey>(
            r#"
            SELECT id, tenant_id, name, key_prefix, key_hash, scopes, created_at,
                   last_used_at, revoked, revoked_at, expires_at
            FROM api_keys
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        Ok(keys)
    }

    /// Lists active (non-revoked, non-expired) API keys for a tenant
    pub async fn list_active_by_tenant(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        let keys = sqlx::query_as::<_, ApiKey>(
            r#"
            SELECT id, tenant_id, name, key_prefix, key_hash, scopes, created_at,
                   last_used_at, revoked, revoked_at, expires_at
            FROM api_keys
            WHERE tenant_id = $1
              AND revoked = FALSE
              AND (expires_at IS NULL OR expires_at > NOW())
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        Ok(keys)
    }

    /// Checks if key has a specific scope
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(&scope.to_string()) || self.scopes.contains(&"admin".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_key() {
        let key = ApiKey::generate_key();
        assert!(key.starts_with("axon_"));
        assert_eq!(key.len(), 37);
    }

    #[test]
    fn test_hash_key() {
        let key = "axon_test123";
        let hash = ApiKey::hash_key(key);
        assert_eq!(hash.len(), 64);

        // Same key produces same hash
        assert_eq!(hash, ApiKey::hash_key(key));
    }

    #[test]
    fn test_extract_prefix() {
        let key = "axon_abc123xyz";
        assert_eq!(ApiKey::extract_prefix(&key), "axon_abc12");
    }

    #[test]
    fn test_has_scope() {
        let api_key = ApiKey {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            name: "Test".to_string(),
            key_prefix: "axon_test".to_string(),
            key_hash: "hash".to_string(),
            scopes: vec!["read:task".to_string(), "write:task".to_string()],
            created_at: Utc::now(),
            last_used_at: None,
            revoked: false,
            revoked_at: None,
            expires_at: None,
        };

        assert!(api_key.has_scope("read:task"));
        assert!(api_key.has_scope("write:task"));
        assert!(!api_key.has_scope("admin"));
    }

    #[test]
    fn test_default_scopes() {
        let scopes = default_scopes();
        assert_eq!(scopes.len(), 2);
        assert!(scopes.contains(&"read:task".to_string()));
        assert!(scopes.contains(&"write:task".to_string()));
    }
}
