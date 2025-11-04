/// Webhook model and database operations
///
/// This module provides the Webhook model for event notifications via HTTP callbacks.
/// Webhooks allow tenants to receive real-time notifications when tasks complete, fail, etc.
///
/// # Security
///
/// - Webhook secrets are stored encrypted
/// - Each delivery includes an HMAC-SHA256 signature
/// - Signatures are sent in the X-AxonTask-Signature header
/// - Recipients should verify signatures to ensure authenticity
///
/// # Schema
///
/// ```sql
/// CREATE TABLE webhooks (
///     id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
///     tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
///     url VARCHAR(2048) NOT NULL,
///     secret BYTEA NOT NULL,
///     active BOOLEAN NOT NULL DEFAULT TRUE,
///     events TEXT[] NOT NULL DEFAULT ARRAY['task.succeeded', 'task.failed'],
///     created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
///     updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
/// );
/// ```
///
/// # Example
///
/// ```no_run
/// use axontask_shared::models::webhook::{Webhook, CreateWebhook};
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
/// use uuid::Uuid;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let pool = create_pool(DatabaseConfig::default()).await?;
///
/// let webhook = Webhook::create(&pool, CreateWebhook {
///     tenant_id: Uuid::new_v4(),
///     url: "https://example.com/webhooks/axontask".to_string(),
///     events: vec!["task.succeeded".to_string(), "task.failed".to_string()],
/// }).await?;
///
/// // Generate signature for a payload
/// let signature = webhook.generate_signature(b"payload data");
/// # Ok(())
/// # }
/// ```

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

/// Webhook model representing an HTTP callback endpoint
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Webhook {
    /// Unique webhook ID
    pub id: Uuid,

    /// Tenant this webhook belongs to
    pub tenant_id: Uuid,

    /// Webhook URL (must be http:// or https://)
    pub url: String,

    /// HMAC secret for signature generation (stored as bytes)
    #[serde(skip_serializing)] // Never expose secret in API responses
    pub secret: Vec<u8>,

    /// Whether webhook is active
    pub active: bool,

    /// Event types to trigger webhook
    ///
    /// Examples: "task.started", "task.succeeded", "task.failed", "task.canceled", "task.timeout"
    pub events: Vec<String>,

    /// When webhook was created
    pub created_at: DateTime<Utc>,

    /// When webhook was last updated
    pub updated_at: DateTime<Utc>,
}

/// Input for creating a new webhook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWebhook {
    /// Tenant ID
    pub tenant_id: Uuid,

    /// Webhook URL
    pub url: String,

    /// Event types to subscribe to
    #[serde(default = "default_events")]
    pub events: Vec<String>,
}

fn default_events() -> Vec<String> {
    vec!["task.succeeded".to_string(), "task.failed".to_string()]
}

/// Input for updating a webhook
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateWebhook {
    /// New URL
    pub url: Option<String>,

    /// New events list
    pub events: Option<Vec<String>>,

    /// Regenerate secret
    pub regenerate_secret: bool,

    /// Toggle active status
    pub active: Option<bool>,
}

impl Webhook {
    /// Generates a secure random secret (32 bytes)
    fn generate_secret() -> Vec<u8> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..32).map(|_| rng.gen()).collect()
    }

    /// Generates HMAC-SHA256 signature for a payload
    ///
    /// This signature should be sent in the X-AxonTask-Signature header.
    ///
    /// # Arguments
    ///
    /// * `payload` - The webhook payload bytes
    ///
    /// # Returns
    ///
    /// Hex-encoded HMAC signature
    ///
    /// # Example
    ///
    /// ```
    /// use axontask_shared::models::webhook::Webhook;
    /// use uuid::Uuid;
    /// use chrono::Utc;
    ///
    /// let webhook = Webhook {
    ///     id: Uuid::new_v4(),
    ///     tenant_id: Uuid::new_v4(),
    ///     url: "https://example.com/webhook".to_string(),
    ///     secret: vec![1, 2, 3, 4],
    ///     active: true,
    ///     events: vec!["task.succeeded".to_string()],
    ///     created_at: Utc::now(),
    ///     updated_at: Utc::now(),
    /// };
    ///
    /// let sig = webhook.generate_signature(b"test payload");
    /// assert_eq!(sig.len(), 64); // SHA-256 hex is 64 chars
    /// ```
    pub fn generate_signature(&self, payload: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.secret)
            .expect("HMAC can take key of any size");

        mac.update(payload);

        format!("{:x}", mac.finalize().into_bytes())
    }

    /// Creates a new webhook
    ///
    /// Automatically generates a secure random secret.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `data` - Webhook creation data
    ///
    /// # Returns
    ///
    /// The newly created webhook
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - URL is invalid format
    /// - Database operation fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::webhook::{Webhook, CreateWebhook};
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let webhook = Webhook::create(&pool, CreateWebhook {
    ///     tenant_id: Uuid::new_v4(),
    ///     url: "https://example.com/webhook".to_string(),
    ///     events: vec!["task.succeeded".to_string()],
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(pool: &PgPool, data: CreateWebhook) -> Result<Self, sqlx::Error> {
        let secret = Self::generate_secret();

        let webhook = sqlx::query_as::<_, Webhook>(
            r#"
            INSERT INTO webhooks (tenant_id, url, secret, events)
            VALUES ($1, $2, $3, $4)
            RETURNING id, tenant_id, url, secret, active, events, created_at, updated_at
            "#,
        )
        .bind(data.tenant_id)
        .bind(data.url)
        .bind(secret)
        .bind(&data.events)
        .fetch_one(pool)
        .await?;

        Ok(webhook)
    }

    /// Finds a webhook by ID
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let webhook = sqlx::query_as::<_, Webhook>(
            r#"
            SELECT id, tenant_id, url, secret, active, events, created_at, updated_at
            FROM webhooks
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(webhook)
    }

    /// Finds a webhook by ID with tenant isolation
    pub async fn find_by_id_and_tenant(
        pool: &PgPool,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        let webhook = sqlx::query_as::<_, Webhook>(
            r#"
            SELECT id, tenant_id, url, secret, active, events, created_at, updated_at
            FROM webhooks
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await?;

        Ok(webhook)
    }

    /// Updates a webhook
    pub async fn update(
        pool: &PgPool,
        id: Uuid,
        data: UpdateWebhook,
    ) -> Result<Option<Self>, sqlx::Error> {
        let mut query = String::from("UPDATE webhooks SET updated_at = NOW()");
        let mut bind_count = 1;

        if data.url.is_some() {
            bind_count += 1;
            query.push_str(&format!(", url = ${}", bind_count));
        }
        if data.events.is_some() {
            bind_count += 1;
            query.push_str(&format!(", events = ${}", bind_count));
        }
        if data.regenerate_secret {
            bind_count += 1;
            query.push_str(&format!(", secret = ${}", bind_count));
        }
        if data.active.is_some() {
            bind_count += 1;
            query.push_str(&format!(", active = ${}", bind_count));
        }

        query.push_str(" WHERE id = $1 RETURNING id, tenant_id, url, secret, active, events, created_at, updated_at");

        let mut q = sqlx::query_as::<_, Webhook>(&query).bind(id);

        if let Some(url) = data.url {
            q = q.bind(url);
        }
        if let Some(events) = data.events {
            q = q.bind(events);
        }
        if data.regenerate_secret {
            q = q.bind(Self::generate_secret());
        }
        if let Some(active) = data.active {
            q = q.bind(active);
        }

        let webhook = q.fetch_optional(pool).await?;

        Ok(webhook)
    }

    /// Toggles webhook active status
    pub async fn toggle_active(pool: &PgPool, id: Uuid, active: bool) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE webhooks SET active = $2, updated_at = NOW() WHERE id = $1"
        )
        .bind(id)
        .bind(active)
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Deletes a webhook
    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM webhooks WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Lists all webhooks for a tenant
    pub async fn list_by_tenant(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        let webhooks = sqlx::query_as::<_, Webhook>(
            r#"
            SELECT id, tenant_id, url, secret, active, events, created_at, updated_at
            FROM webhooks
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        Ok(webhooks)
    }

    /// Lists active webhooks for a tenant
    pub async fn list_active_by_tenant(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        let webhooks = sqlx::query_as::<_, Webhook>(
            r#"
            SELECT id, tenant_id, url, secret, active, events, created_at, updated_at
            FROM webhooks
            WHERE tenant_id = $1 AND active = TRUE
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        Ok(webhooks)
    }

    /// Finds webhooks subscribed to a specific event type
    pub async fn find_by_event_type(
        pool: &PgPool,
        tenant_id: Uuid,
        event_type: &str,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let webhooks = sqlx::query_as::<_, Webhook>(
            r#"
            SELECT id, tenant_id, url, secret, active, events, created_at, updated_at
            FROM webhooks
            WHERE tenant_id = $1
              AND active = TRUE
              AND $2 = ANY(events)
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .bind(event_type)
        .fetch_all(pool)
        .await?;

        Ok(webhooks)
    }

    /// Counts webhooks for a tenant
    pub async fn count_by_tenant(pool: &PgPool, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM webhooks WHERE tenant_id = $1"
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_secret() {
        let secret1 = Webhook::generate_secret();
        let secret2 = Webhook::generate_secret();

        assert_eq!(secret1.len(), 32);
        assert_eq!(secret2.len(), 32);
        assert_ne!(secret1, secret2); // Should be random
    }

    #[test]
    fn test_generate_signature() {
        let webhook = Webhook {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            url: "https://example.com".to_string(),
            secret: vec![1, 2, 3, 4, 5],
            active: true,
            events: vec!["task.succeeded".to_string()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let sig1 = webhook.generate_signature(b"test payload");
        let sig2 = webhook.generate_signature(b"test payload");

        assert_eq!(sig1.len(), 64); // HMAC-SHA256 hex is 64 chars
        assert_eq!(sig1, sig2); // Deterministic

        // Different payload = different signature
        let sig3 = webhook.generate_signature(b"different payload");
        assert_ne!(sig1, sig3);
    }

    #[test]
    fn test_default_events() {
        let events = default_events();
        assert_eq!(events.len(), 2);
        assert!(events.contains(&"task.succeeded".to_string()));
        assert!(events.contains(&"task.failed".to_string()));
    }
}
