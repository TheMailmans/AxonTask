/// Tenant model and database operations
///
/// This module provides the Tenant model for multi-tenant isolation.
/// Every user belongs to one or more tenants via the Membership model.
///
/// # Schema
///
/// ```sql
/// CREATE TABLE tenants (
///     id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
///     name VARCHAR(255) NOT NULL,
///     plan VARCHAR(50) NOT NULL DEFAULT 'trial',
///     stripe_customer_id VARCHAR(255),
///     stripe_subscription_id VARCHAR(255),
///     settings JSONB NOT NULL DEFAULT '{}',
///     created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
///     updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
///     CONSTRAINT tenants_plan_check CHECK (
///         plan IN ('trial', 'entry', 'pro', 'enterprise')
///     )
/// );
/// ```
///
/// # Example
///
/// ```no_run
/// use axontask_shared::models::tenant::{Tenant, CreateTenant, TenantPlan};
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let pool = create_pool(DatabaseConfig::default()).await?;
///
/// // Create a new tenant
/// let new_tenant = CreateTenant {
///     name: "Acme Corp".to_string(),
///     plan: TenantPlan::Trial,
/// };
///
/// let tenant = Tenant::create(&pool, new_tenant).await?;
/// println!("Created tenant: {}", tenant.id);
///
/// // Upgrade plan
/// tenant.update_plan(&pool, TenantPlan::Pro).await?;
/// # Ok(())
/// # }
/// ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

/// Billing plan types
///
/// Plans determine quotas, features, and pricing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[serde(rename_all = "lowercase")]
pub enum TenantPlan {
    /// Trial plan (7 days, limited features)
    #[serde(rename = "trial")]
    Trial,

    /// Entry plan ($9.99/month, 1000 tasks/day)
    #[serde(rename = "entry")]
    Entry,

    /// Professional plan ($29/month, unlimited tasks)
    #[serde(rename = "pro")]
    Pro,

    /// Enterprise plan (custom pricing, dedicated support)
    #[serde(rename = "enterprise")]
    Enterprise,
}

impl TenantPlan {
    /// Converts plan to string for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            TenantPlan::Trial => "trial",
            TenantPlan::Entry => "entry",
            TenantPlan::Pro => "pro",
            TenantPlan::Enterprise => "enterprise",
        }
    }

    /// Parses plan from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "trial" => Some(TenantPlan::Trial),
            "entry" => Some(TenantPlan::Entry),
            "pro" => Some(TenantPlan::Pro),
            "enterprise" => Some(TenantPlan::Enterprise),
            _ => None,
        }
    }
}

/// Tenant model representing an organization/account
///
/// Tenants are the top-level entity for multi-tenant isolation.
/// All resources (tasks, API keys, etc.) belong to a tenant.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tenant {
    /// Unique tenant ID (UUID v4)
    pub id: Uuid,

    /// Organization/account name
    pub name: String,

    /// Current billing plan
    #[sqlx(try_from = "String")]
    pub plan: String,

    /// Stripe customer ID (if billing enabled)
    pub stripe_customer_id: Option<String>,

    /// Stripe subscription ID (if billing enabled)
    pub stripe_subscription_id: Option<String>,

    /// Tenant-specific configuration (JSONB)
    ///
    /// Example: {"quotas": {"concurrent_tasks": 100}, "retention_days": 30}
    pub settings: JsonValue,

    /// When the tenant was created
    pub created_at: DateTime<Utc>,

    /// When the tenant was last updated
    pub updated_at: DateTime<Utc>,
}

impl Tenant {
    /// Gets the parsed plan enum
    pub fn get_plan(&self) -> Option<TenantPlan> {
        TenantPlan::from_str(&self.plan)
    }
}

/// Input for creating a new tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTenant {
    /// Organization/account name
    pub name: String,

    /// Initial billing plan (defaults to Trial)
    #[serde(default = "default_plan")]
    pub plan: TenantPlan,
}

fn default_plan() -> TenantPlan {
    TenantPlan::Trial
}

/// Input for updating an existing tenant
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateTenant {
    /// New name
    pub name: Option<String>,

    /// New plan
    pub plan: Option<TenantPlan>,

    /// New Stripe customer ID
    pub stripe_customer_id: Option<String>,

    /// New Stripe subscription ID
    pub stripe_subscription_id: Option<String>,

    /// Update settings (will be merged with existing settings)
    pub settings: Option<JsonValue>,
}

impl Tenant {
    /// Creates a new tenant in the database
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `data` - Tenant creation data
    ///
    /// # Returns
    ///
    /// The newly created tenant with generated ID and timestamps
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database connection fails
    /// - Required fields are missing
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::tenant::{Tenant, CreateTenant, TenantPlan};
    /// # use sqlx::PgPool;
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let new_tenant = CreateTenant {
    ///     name: "Acme Corp".to_string(),
    ///     plan: TenantPlan::Trial,
    /// };
    ///
    /// let tenant = Tenant::create(&pool, new_tenant).await?;
    /// println!("Created tenant: {}", tenant.id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(pool: &PgPool, data: CreateTenant) -> Result<Self, sqlx::Error> {
        let tenant = sqlx::query_as::<_, Tenant>(
            r#"
            INSERT INTO tenants (name, plan)
            VALUES ($1, $2)
            RETURNING id, name, plan, stripe_customer_id, stripe_subscription_id,
                      settings, created_at, updated_at
            "#,
        )
        .bind(data.name)
        .bind(data.plan.as_str())
        .fetch_one(pool)
        .await?;

        Ok(tenant)
    }

    /// Finds a tenant by ID
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `id` - Tenant ID to search for
    ///
    /// # Returns
    ///
    /// The tenant if found, None otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::tenant::Tenant;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    /// if let Some(tenant) = Tenant::find_by_id(&pool, tenant_id).await? {
    ///     println!("Found tenant: {}", tenant.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let tenant = sqlx::query_as::<_, Tenant>(
            r#"
            SELECT id, name, plan, stripe_customer_id, stripe_subscription_id,
                   settings, created_at, updated_at
            FROM tenants
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(tenant)
    }

    /// Finds a tenant by name (case-sensitive)
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `name` - Tenant name to search for
    ///
    /// # Returns
    ///
    /// The tenant if found, None otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::tenant::Tenant;
    /// # use sqlx::PgPool;
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let tenant = Tenant::find_by_name(&pool, "Acme Corp").await?;
    /// if let Some(t) = tenant {
    ///     println!("Found tenant: {}", t.id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn find_by_name(pool: &PgPool, name: &str) -> Result<Option<Self>, sqlx::Error> {
        let tenant = sqlx::query_as::<_, Tenant>(
            r#"
            SELECT id, name, plan, stripe_customer_id, stripe_subscription_id,
                   settings, created_at, updated_at
            FROM tenants
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(pool)
        .await?;

        Ok(tenant)
    }

    /// Updates an existing tenant
    ///
    /// Only non-None fields in `data` will be updated. Settings are merged
    /// with existing settings (not replaced).
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `id` - ID of tenant to update
    /// * `data` - Fields to update
    ///
    /// # Returns
    ///
    /// The updated tenant if found, None if tenant doesn't exist
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::tenant::{Tenant, UpdateTenant, TenantPlan};
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    /// let update = UpdateTenant {
    ///     name: Some("New Name".to_string()),
    ///     plan: Some(TenantPlan::Pro),
    ///     ..Default::default()
    /// };
    ///
    /// if let Some(tenant) = Tenant::update(&pool, tenant_id, update).await? {
    ///     println!("Updated tenant: {}", tenant.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update(
        pool: &PgPool,
        id: Uuid,
        data: UpdateTenant,
    ) -> Result<Option<Self>, sqlx::Error> {
        let mut query = String::from("UPDATE tenants SET updated_at = NOW()");
        let mut bind_count = 1;

        if data.name.is_some() {
            bind_count += 1;
            query.push_str(&format!(", name = ${}", bind_count));
        }
        if data.plan.is_some() {
            bind_count += 1;
            query.push_str(&format!(", plan = ${}", bind_count));
        }
        if data.stripe_customer_id.is_some() {
            bind_count += 1;
            query.push_str(&format!(", stripe_customer_id = ${}", bind_count));
        }
        if data.stripe_subscription_id.is_some() {
            bind_count += 1;
            query.push_str(&format!(", stripe_subscription_id = ${}", bind_count));
        }
        if data.settings.is_some() {
            bind_count += 1;
            // Merge settings with existing (jsonb || operator)
            query.push_str(&format!(", settings = settings || ${}", bind_count));
        }

        query.push_str(" WHERE id = $1 RETURNING id, name, plan, stripe_customer_id, stripe_subscription_id, settings, created_at, updated_at");

        let mut q = sqlx::query_as::<_, Tenant>(&query).bind(id);

        if let Some(name) = data.name {
            q = q.bind(name);
        }
        if let Some(plan) = data.plan {
            q = q.bind(plan.as_str());
        }
        if let Some(customer_id) = data.stripe_customer_id {
            q = q.bind(customer_id);
        }
        if let Some(sub_id) = data.stripe_subscription_id {
            q = q.bind(sub_id);
        }
        if let Some(settings) = data.settings {
            q = q.bind(settings);
        }

        let tenant = q.fetch_optional(pool).await?;

        Ok(tenant)
    }

    /// Updates a tenant's plan
    ///
    /// This is a convenience method for the common operation of upgrading/downgrading plans.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `id` - ID of tenant to update
    /// * `plan` - New plan
    ///
    /// # Returns
    ///
    /// The updated tenant if found, None if tenant doesn't exist
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::tenant::{Tenant, TenantPlan};
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    /// Tenant::update_plan(&pool, tenant_id, TenantPlan::Pro).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_plan(
        pool: &PgPool,
        id: Uuid,
        plan: TenantPlan,
    ) -> Result<Option<Self>, sqlx::Error> {
        let tenant = sqlx::query_as::<_, Tenant>(
            r#"
            UPDATE tenants
            SET plan = $2, updated_at = NOW()
            WHERE id = $1
            RETURNING id, name, plan, stripe_customer_id, stripe_subscription_id,
                      settings, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(plan.as_str())
        .fetch_optional(pool)
        .await?;

        Ok(tenant)
    }

    /// Deletes a tenant by ID
    ///
    /// ⚠️  **WARNING**: This cascades to all related data (tasks, API keys, etc.).
    /// Use with extreme caution!
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `id` - ID of tenant to delete
    ///
    /// # Returns
    ///
    /// True if tenant was deleted, false if tenant didn't exist
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::tenant::Tenant;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    /// let deleted = Tenant::delete(&pool, tenant_id).await?;
    /// if deleted {
    ///     println!("Tenant and all related data deleted");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Lists all tenants with pagination
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `limit` - Maximum number of tenants to return
    /// * `offset` - Number of tenants to skip (for pagination)
    ///
    /// # Returns
    ///
    /// Vector of tenants, ordered by creation date (newest first)
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::tenant::Tenant;
    /// # use sqlx::PgPool;
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// // Get first page (10 tenants)
    /// let page1 = Tenant::list(&pool, 10, 0).await?;
    ///
    /// // Get second page
    /// let page2 = Tenant::list(&pool, 10, 10).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<Self>, sqlx::Error> {
        let tenants = sqlx::query_as::<_, Tenant>(
            r#"
            SELECT id, name, plan, stripe_customer_id, stripe_subscription_id,
                   settings, created_at, updated_at
            FROM tenants
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok(tenants)
    }

    /// Counts total number of tenants
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    ///
    /// # Returns
    ///
    /// Total number of tenants in the database
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::tenant::Tenant;
    /// # use sqlx::PgPool;
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let total = Tenant::count(&pool).await?;
    /// println!("Total tenants: {}", total);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn count(pool: &PgPool) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tenants")
            .fetch_one(pool)
            .await?;

        Ok(count)
    }

    /// Lists tenants by plan
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `plan` - Plan to filter by
    /// * `limit` - Maximum number of tenants to return
    /// * `offset` - Number of tenants to skip
    ///
    /// # Returns
    ///
    /// Vector of tenants on the specified plan
    ///
    /// # Errors
    ///
    /// Returns an error if database connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::tenant::{Tenant, TenantPlan};
    /// # use sqlx::PgPool;
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let pro_tenants = Tenant::list_by_plan(&pool, TenantPlan::Pro, 100, 0).await?;
    /// println!("Found {} Pro plan tenants", pro_tenants.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_by_plan(
        pool: &PgPool,
        plan: TenantPlan,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let tenants = sqlx::query_as::<_, Tenant>(
            r#"
            SELECT id, name, plan, stripe_customer_id, stripe_subscription_id,
                   settings, created_at, updated_at
            FROM tenants
            WHERE plan = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(plan.as_str())
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok(tenants)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_plan_as_str() {
        assert_eq!(TenantPlan::Trial.as_str(), "trial");
        assert_eq!(TenantPlan::Entry.as_str(), "entry");
        assert_eq!(TenantPlan::Pro.as_str(), "pro");
        assert_eq!(TenantPlan::Enterprise.as_str(), "enterprise");
    }

    #[test]
    fn test_tenant_plan_from_str() {
        assert_eq!(TenantPlan::from_str("trial"), Some(TenantPlan::Trial));
        assert_eq!(TenantPlan::from_str("entry"), Some(TenantPlan::Entry));
        assert_eq!(TenantPlan::from_str("pro"), Some(TenantPlan::Pro));
        assert_eq!(TenantPlan::from_str("enterprise"), Some(TenantPlan::Enterprise));
        assert_eq!(TenantPlan::from_str("invalid"), None);
    }

    #[test]
    fn test_create_tenant_default_plan() {
        let create = CreateTenant {
            name: "Test Corp".to_string(),
            plan: default_plan(),
        };
        assert_eq!(create.plan, TenantPlan::Trial);
    }

    #[test]
    fn test_update_tenant_default() {
        let update = UpdateTenant::default();
        assert!(update.name.is_none());
        assert!(update.plan.is_none());
        assert!(update.settings.is_none());
    }

    // Integration tests for database operations are in tests/models/tenant_tests.rs
}
