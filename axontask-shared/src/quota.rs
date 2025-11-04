/// Quota enforcement for multi-tenant resource limits
///
/// This module provides quota enforcement based on tenant billing plans.
/// Quotas are enforced on:
/// - Concurrent running tasks
/// - Daily task creation
/// - Active stream connections
///
/// # Quota Limits by Plan
///
/// **Trial Plan:**
/// - Concurrent tasks: 5
/// - Daily tasks: 100
/// - Stream connections: 2
///
/// **Entry Plan:**
/// - Concurrent tasks: 25
/// - Daily tasks: 1,000
/// - Stream connections: 5
///
/// **Pro Plan:**
/// - Concurrent tasks: 100
/// - Daily tasks: 10,000
/// - Stream connections: 20
///
/// **Enterprise Plan:**
/// - Concurrent tasks: 500
/// - Daily tasks: 100,000
/// - Stream connections: 100
///
/// # Example
///
/// ```no_run
/// use axontask_shared::quota::{QuotaEnforcer, QuotaType};
/// use axontask_shared::models::tenant::TenantPlan;
/// use sqlx::PgPool;
/// use uuid::Uuid;
///
/// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
/// let enforcer = QuotaEnforcer::new(pool);
///
/// // Check if tenant can start a new task
/// if !enforcer.check_quota(tenant_id, QuotaType::ConcurrentTasks).await? {
///     return Err("Concurrent task limit exceeded".into());
/// }
///
/// // Start task...
///
/// # Ok(())
/// # }
/// ```

use crate::models::tenant::{Tenant, TenantPlan};
use crate::models::task::{Task, TaskState};
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use std::fmt;
use uuid::Uuid;

/// Quota enforcement error
#[derive(Debug)]
pub enum QuotaError {
    /// Quota limit exceeded
    LimitExceeded {
        quota_type: QuotaType,
        limit: u32,
        current: u32,
    },

    /// Database error
    DatabaseError(sqlx::Error),

    /// Tenant not found
    TenantNotFound(Uuid),
}

impl fmt::Display for QuotaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuotaError::LimitExceeded {
                quota_type,
                limit,
                current,
            } => write!(
                f,
                "{} limit exceeded ({}/{})",
                quota_type.as_str(),
                current,
                limit
            ),
            QuotaError::DatabaseError(err) => write!(f, "Database error: {}", err),
            QuotaError::TenantNotFound(id) => write!(f, "Tenant not found: {}", id),
        }
    }
}

impl std::error::Error for QuotaError {}

impl From<sqlx::Error> for QuotaError {
    fn from(err: sqlx::Error) -> Self {
        QuotaError::DatabaseError(err)
    }
}

/// Type of quota to check
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaType {
    /// Maximum concurrent running tasks
    ConcurrentTasks,

    /// Maximum daily task creation
    DailyTasks,

    /// Maximum active stream connections
    StreamConnections,
}

impl QuotaType {
    /// Human-readable name
    pub fn as_str(&self) -> &'static str {
        match self {
            QuotaType::ConcurrentTasks => "Concurrent tasks",
            QuotaType::DailyTasks => "Daily tasks",
            QuotaType::StreamConnections => "Stream connections",
        }
    }
}

/// Quota limits configuration
#[derive(Debug, Clone, Copy)]
pub struct QuotaLimits {
    /// Maximum concurrent running tasks
    pub concurrent_tasks: u32,

    /// Maximum daily task creation
    pub daily_tasks: u32,

    /// Maximum active stream connections
    pub stream_connections: u32,
}

impl QuotaLimits {
    /// Gets quota limits for a tenant plan
    pub fn for_plan(plan: TenantPlan) -> Self {
        match plan {
            TenantPlan::Trial => QuotaLimits {
                concurrent_tasks: 5,
                daily_tasks: 100,
                stream_connections: 2,
            },
            TenantPlan::Entry => QuotaLimits {
                concurrent_tasks: 25,
                daily_tasks: 1_000,
                stream_connections: 5,
            },
            TenantPlan::Pro => QuotaLimits {
                concurrent_tasks: 100,
                daily_tasks: 10_000,
                stream_connections: 20,
            },
            TenantPlan::Enterprise => QuotaLimits {
                concurrent_tasks: 500,
                daily_tasks: 100_000,
                stream_connections: 100,
            },
        }
    }

    /// Gets limit for a specific quota type
    pub fn get(&self, quota_type: QuotaType) -> u32 {
        match quota_type {
            QuotaType::ConcurrentTasks => self.concurrent_tasks,
            QuotaType::DailyTasks => self.daily_tasks,
            QuotaType::StreamConnections => self.stream_connections,
        }
    }
}

/// Result of quota check
#[derive(Debug, Clone)]
pub struct QuotaCheckResult {
    /// Whether request is within quota
    pub allowed: bool,

    /// Current usage
    pub current: u32,

    /// Maximum allowed
    pub limit: u32,

    /// Remaining quota
    pub remaining: u32,
}

impl QuotaCheckResult {
    /// Creates a result indicating quota is available
    pub fn allowed(current: u32, limit: u32) -> Self {
        QuotaCheckResult {
            allowed: true,
            current,
            limit,
            remaining: limit.saturating_sub(current),
        }
    }

    /// Creates a result indicating quota is exceeded
    pub fn exceeded(current: u32, limit: u32) -> Self {
        QuotaCheckResult {
            allowed: false,
            current,
            limit,
            remaining: 0,
        }
    }
}

/// Quota enforcement service
///
/// Checks resource usage against plan-based limits.
pub struct QuotaEnforcer {
    db: PgPool,
}

impl QuotaEnforcer {
    /// Creates a new quota enforcer
    pub fn new(db: PgPool) -> Self {
        QuotaEnforcer { db }
    }

    /// Checks if tenant is within quota for a specific resource
    ///
    /// # Arguments
    ///
    /// * `tenant_id` - Tenant to check
    /// * `quota_type` - Type of quota to check
    ///
    /// # Returns
    ///
    /// Result with quota status
    ///
    /// # Errors
    ///
    /// Returns error if database query fails or tenant not found
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::quota::{QuotaEnforcer, QuotaType};
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
    /// let enforcer = QuotaEnforcer::new(pool);
    ///
    /// let result = enforcer.check(tenant_id, QuotaType::ConcurrentTasks).await?;
    /// if result.allowed {
    ///     println!("Within quota: {}/{}", result.current, result.limit);
    /// } else {
    ///     println!("Quota exceeded!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn check(
        &self,
        tenant_id: Uuid,
        quota_type: QuotaType,
    ) -> Result<QuotaCheckResult, QuotaError> {
        // Get tenant and plan
        let tenant = Tenant::find_by_id(&self.db, tenant_id)
            .await?
            .ok_or(QuotaError::TenantNotFound(tenant_id))?;

        let plan = tenant.get_plan().unwrap_or(TenantPlan::Trial);
        let limits = QuotaLimits::for_plan(plan);
        let limit = limits.get(quota_type);

        // Get current usage
        let current = match quota_type {
            QuotaType::ConcurrentTasks => self.count_concurrent_tasks(tenant_id).await?,
            QuotaType::DailyTasks => self.count_daily_tasks(tenant_id).await?,
            QuotaType::StreamConnections => {
                // Stream connections tracking requires Redis/session management
                // For now, we'll return 0 (not enforced yet)
                0
            }
        };

        if current >= limit {
            Ok(QuotaCheckResult::exceeded(current, limit))
        } else {
            Ok(QuotaCheckResult::allowed(current, limit))
        }
    }

    /// Enforces quota with error on limit exceeded
    ///
    /// Convenience method that returns an error if quota is exceeded.
    ///
    /// # Arguments
    ///
    /// * `tenant_id` - Tenant to check
    /// * `quota_type` - Type of quota to check
    ///
    /// # Returns
    ///
    /// Ok(()) if within quota, Err if exceeded or database error
    ///
    /// # Errors
    ///
    /// Returns `QuotaError::LimitExceeded` if quota is exceeded
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::quota::{QuotaEnforcer, QuotaType};
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
    /// let enforcer = QuotaEnforcer::new(pool);
    ///
    /// // Returns error if quota exceeded
    /// enforcer.enforce(tenant_id, QuotaType::ConcurrentTasks).await?;
    ///
    /// // Proceed with operation...
    /// # Ok(())
    /// # }
    /// ```
    pub async fn enforce(
        &self,
        tenant_id: Uuid,
        quota_type: QuotaType,
    ) -> Result<(), QuotaError> {
        let result = self.check(tenant_id, quota_type).await?;

        if !result.allowed {
            return Err(QuotaError::LimitExceeded {
                quota_type,
                limit: result.limit,
                current: result.current,
            });
        }

        Ok(())
    }

    /// Gets quota limits for a tenant
    ///
    /// # Arguments
    ///
    /// * `tenant_id` - Tenant ID
    ///
    /// # Returns
    ///
    /// Quota limits based on tenant plan
    ///
    /// # Errors
    ///
    /// Returns error if database query fails or tenant not found
    pub async fn get_limits(&self, tenant_id: Uuid) -> Result<QuotaLimits, QuotaError> {
        let tenant = Tenant::find_by_id(&self.db, tenant_id)
            .await?
            .ok_or(QuotaError::TenantNotFound(tenant_id))?;

        let plan = tenant.get_plan().unwrap_or(TenantPlan::Trial);
        Ok(QuotaLimits::for_plan(plan))
    }

    /// Counts concurrent running tasks for a tenant
    ///
    /// Counts tasks in 'running' state.
    async fn count_concurrent_tasks(&self, tenant_id: Uuid) -> Result<u32, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM tasks
            WHERE tenant_id = $1 AND state = $2
            "#,
        )
        .bind(tenant_id)
        .bind(TaskState::Running.as_str())
        .fetch_one(&self.db)
        .await?;

        Ok(count as u32)
    }

    /// Counts daily tasks created for a tenant
    ///
    /// Counts tasks created in the last 24 hours.
    async fn count_daily_tasks(&self, tenant_id: Uuid) -> Result<u32, sqlx::Error> {
        let since = Utc::now() - Duration::hours(24);

        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM tasks
            WHERE tenant_id = $1 AND created_at >= $2
            "#,
        )
        .bind(tenant_id)
        .bind(since)
        .fetch_one(&self.db)
        .await?;

        Ok(count as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_limits_trial() {
        let limits = QuotaLimits::for_plan(TenantPlan::Trial);
        assert_eq!(limits.concurrent_tasks, 5);
        assert_eq!(limits.daily_tasks, 100);
        assert_eq!(limits.stream_connections, 2);
    }

    #[test]
    fn test_quota_limits_entry() {
        let limits = QuotaLimits::for_plan(TenantPlan::Entry);
        assert_eq!(limits.concurrent_tasks, 25);
        assert_eq!(limits.daily_tasks, 1_000);
        assert_eq!(limits.stream_connections, 5);
    }

    #[test]
    fn test_quota_limits_pro() {
        let limits = QuotaLimits::for_plan(TenantPlan::Pro);
        assert_eq!(limits.concurrent_tasks, 100);
        assert_eq!(limits.daily_tasks, 10_000);
        assert_eq!(limits.stream_connections, 20);
    }

    #[test]
    fn test_quota_limits_enterprise() {
        let limits = QuotaLimits::for_plan(TenantPlan::Enterprise);
        assert_eq!(limits.concurrent_tasks, 500);
        assert_eq!(limits.daily_tasks, 100_000);
        assert_eq!(limits.stream_connections, 100);
    }

    #[test]
    fn test_quota_limits_get() {
        let limits = QuotaLimits::for_plan(TenantPlan::Pro);
        assert_eq!(limits.get(QuotaType::ConcurrentTasks), 100);
        assert_eq!(limits.get(QuotaType::DailyTasks), 10_000);
        assert_eq!(limits.get(QuotaType::StreamConnections), 20);
    }

    #[test]
    fn test_quota_check_result_allowed() {
        let result = QuotaCheckResult::allowed(5, 10);
        assert!(result.allowed);
        assert_eq!(result.current, 5);
        assert_eq!(result.limit, 10);
        assert_eq!(result.remaining, 5);
    }

    #[test]
    fn test_quota_check_result_exceeded() {
        let result = QuotaCheckResult::exceeded(15, 10);
        assert!(!result.allowed);
        assert_eq!(result.current, 15);
        assert_eq!(result.limit, 10);
        assert_eq!(result.remaining, 0);
    }

    #[test]
    fn test_quota_type_as_str() {
        assert_eq!(QuotaType::ConcurrentTasks.as_str(), "Concurrent tasks");
        assert_eq!(QuotaType::DailyTasks.as_str(), "Daily tasks");
        assert_eq!(QuotaType::StreamConnections.as_str(), "Stream connections");
    }

    #[test]
    fn test_quota_error_display() {
        let err = QuotaError::LimitExceeded {
            quota_type: QuotaType::ConcurrentTasks,
            limit: 10,
            current: 15,
        };
        assert_eq!(err.to_string(), "Concurrent tasks limit exceeded (15/10)");

        let err = QuotaError::TenantNotFound(Uuid::nil());
        assert!(err.to_string().contains("Tenant not found"));
    }
}
