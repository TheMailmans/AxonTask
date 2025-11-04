/// Usage Counter model and database operations
///
/// This module provides the UsageCounter model for tracking tenant usage metrics.
/// Counters are used for billing, quota enforcement, and usage analytics.
///
/// # Metrics Tracked
///
/// - **task_minutes**: Total task execution time in minutes (rounded up)
/// - **streams**: Total SSE stream connections
/// - **bytes**: Total bytes streamed via SSE
/// - **tasks_created**: Total tasks created
///
/// # Schema
///
/// ```sql
/// CREATE TABLE usage_counters (
///     tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
///     period DATE NOT NULL,
///     task_minutes INTEGER NOT NULL DEFAULT 0,
///     streams INTEGER NOT NULL DEFAULT 0,
///     bytes BIGINT NOT NULL DEFAULT 0,
///     tasks_created INTEGER NOT NULL DEFAULT 0,
///     PRIMARY KEY (tenant_id, period)
/// );
/// ```
///
/// # Example
///
/// ```no_run
/// use axontask_shared::models::usage::UsageCounter;
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
/// use chrono::Utc;
/// use uuid::Uuid;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let pool = create_pool(DatabaseConfig::default()).await?;
/// let tenant_id = Uuid::new_v4();
///
/// // Increment task minutes
/// UsageCounter::increment_task_minutes(&pool, tenant_id, 5).await?;
///
/// // Get current usage
/// let usage = UsageCounter::get_current(&pool, tenant_id).await?;
/// println!("Task minutes used: {}", usage.task_minutes);
/// # Ok(())
/// # }
/// ```

use chrono::{Date, DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Usage counter model tracking tenant usage per day
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UsageCounter {
    /// Tenant ID
    pub tenant_id: Uuid,

    /// Usage period (date in YYYY-MM-DD format)
    pub period: NaiveDate,

    /// Total task execution minutes (rounded up, for billing)
    pub task_minutes: i32,

    /// Total SSE stream connections
    pub streams: i32,

    /// Total bytes streamed via SSE
    pub bytes: i64,

    /// Total tasks created
    pub tasks_created: i32,
}

impl UsageCounter {
    /// Gets today's date in UTC
    fn today() -> NaiveDate {
        Utc::now().date_naive()
    }

    /// Gets current usage for a tenant (today's period)
    ///
    /// If no usage exists for today, returns zero counters.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `tenant_id` - Tenant ID
    ///
    /// # Returns
    ///
    /// Current usage counters
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::usage::UsageCounter;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    /// let usage = UsageCounter::get_current(&pool, tenant_id).await?;
    /// println!("Today's usage: {} task minutes", usage.task_minutes);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_current(pool: &PgPool, tenant_id: Uuid) -> Result<Self, sqlx::Error> {
        let today = Self::today();

        // Try to get existing record
        let usage = sqlx::query_as::<_, UsageCounter>(
            r#"
            SELECT tenant_id, period, task_minutes, streams, bytes, tasks_created
            FROM usage_counters
            WHERE tenant_id = $1 AND period = $2
            "#,
        )
        .bind(tenant_id)
        .bind(today)
        .fetch_optional(pool)
        .await?;

        // If no record exists, return zeros
        Ok(usage.unwrap_or(UsageCounter {
            tenant_id,
            period: today,
            task_minutes: 0,
            streams: 0,
            bytes: 0,
            tasks_created: 0,
        }))
    }

    /// Gets usage for a specific period
    pub async fn get_for_period(
        pool: &PgPool,
        tenant_id: Uuid,
        period: NaiveDate,
    ) -> Result<Option<Self>, sqlx::Error> {
        let usage = sqlx::query_as::<_, UsageCounter>(
            r#"
            SELECT tenant_id, period, task_minutes, streams, bytes, tasks_created
            FROM usage_counters
            WHERE tenant_id = $1 AND period = $2
            "#,
        )
        .bind(tenant_id)
        .bind(period)
        .fetch_optional(pool)
        .await?;

        Ok(usage)
    }

    /// Increments task minutes counter
    ///
    /// Creates a new record if one doesn't exist for today.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `tenant_id` - Tenant ID
    /// * `minutes` - Number of minutes to add
    ///
    /// # Returns
    ///
    /// Updated usage counter
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::usage::UsageCounter;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    /// // Task ran for 5 minutes
    /// UsageCounter::increment_task_minutes(&pool, tenant_id, 5).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn increment_task_minutes(
        pool: &PgPool,
        tenant_id: Uuid,
        minutes: i32,
    ) -> Result<Self, sqlx::Error> {
        let today = Self::today();

        let usage = sqlx::query_as::<_, UsageCounter>(
            r#"
            INSERT INTO usage_counters (tenant_id, period, task_minutes)
            VALUES ($1, $2, $3)
            ON CONFLICT (tenant_id, period)
            DO UPDATE SET task_minutes = usage_counters.task_minutes + EXCLUDED.task_minutes
            RETURNING tenant_id, period, task_minutes, streams, bytes, tasks_created
            "#,
        )
        .bind(tenant_id)
        .bind(today)
        .bind(minutes)
        .fetch_one(pool)
        .await?;

        Ok(usage)
    }

    /// Increments streams counter
    pub async fn increment_streams(pool: &PgPool, tenant_id: Uuid) -> Result<Self, sqlx::Error> {
        let today = Self::today();

        let usage = sqlx::query_as::<_, UsageCounter>(
            r#"
            INSERT INTO usage_counters (tenant_id, period, streams)
            VALUES ($1, $2, 1)
            ON CONFLICT (tenant_id, period)
            DO UPDATE SET streams = usage_counters.streams + 1
            RETURNING tenant_id, period, task_minutes, streams, bytes, tasks_created
            "#,
        )
        .bind(tenant_id)
        .bind(today)
        .fetch_one(pool)
        .await?;

        Ok(usage)
    }

    /// Increments bytes counter
    pub async fn increment_bytes(
        pool: &PgPool,
        tenant_id: Uuid,
        bytes: i64,
    ) -> Result<Self, sqlx::Error> {
        let today = Self::today();

        let usage = sqlx::query_as::<_, UsageCounter>(
            r#"
            INSERT INTO usage_counters (tenant_id, period, bytes)
            VALUES ($1, $2, $3)
            ON CONFLICT (tenant_id, period)
            DO UPDATE SET bytes = usage_counters.bytes + EXCLUDED.bytes
            RETURNING tenant_id, period, task_minutes, streams, bytes, tasks_created
            "#,
        )
        .bind(tenant_id)
        .bind(today)
        .bind(bytes)
        .fetch_one(pool)
        .await?;

        Ok(usage)
    }

    /// Increments tasks_created counter
    pub async fn increment_tasks_created(pool: &PgPool, tenant_id: Uuid) -> Result<Self, sqlx::Error> {
        let today = Self::today();

        let usage = sqlx::query_as::<_, UsageCounter>(
            r#"
            INSERT INTO usage_counters (tenant_id, period, tasks_created)
            VALUES ($1, $2, 1)
            ON CONFLICT (tenant_id, period)
            DO UPDATE SET tasks_created = usage_counters.tasks_created + 1
            RETURNING tenant_id, period, task_minutes, streams, bytes, tasks_created
            "#,
        )
        .bind(tenant_id)
        .bind(today)
        .fetch_one(pool)
        .await?;

        Ok(usage)
    }

    /// Gets usage history for a tenant (last N days)
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `tenant_id` - Tenant ID
    /// * `days` - Number of days to retrieve (default 30)
    ///
    /// # Returns
    ///
    /// Vector of usage counters ordered by period descending (most recent first)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::usage::UsageCounter;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    /// // Get last 30 days of usage
    /// let history = UsageCounter::get_history(&pool, tenant_id, 30).await?;
    /// for usage in history {
    ///     println!("{}: {} minutes", usage.period, usage.task_minutes);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_history(
        pool: &PgPool,
        tenant_id: Uuid,
        days: i32,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let usage = sqlx::query_as::<_, UsageCounter>(
            r#"
            SELECT tenant_id, period, task_minutes, streams, bytes, tasks_created
            FROM usage_counters
            WHERE tenant_id = $1
              AND period >= CURRENT_DATE - $2::INTEGER
            ORDER BY period DESC
            "#,
        )
        .bind(tenant_id)
        .bind(days)
        .fetch_all(pool)
        .await?;

        Ok(usage)
    }

    /// Gets aggregated usage for a date range
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `tenant_id` - Tenant ID
    /// * `start_date` - Start date (inclusive)
    /// * `end_date` - End date (inclusive)
    ///
    /// # Returns
    ///
    /// Aggregated totals for the date range
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::usage::{UsageCounter, UsageAggregate};
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # use chrono::NaiveDate;
    /// # async fn example(pool: PgPool, tenant_id: Uuid) -> Result<(), sqlx::Error> {
    /// let start = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
    /// let end = NaiveDate::from_ymd_opt(2025, 1, 31).unwrap();
    ///
    /// let total = UsageCounter::get_aggregate(&pool, tenant_id, start, end).await?;
    /// println!("Total minutes in January: {}", total.total_task_minutes);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_aggregate(
        pool: &PgPool,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<UsageAggregate, sqlx::Error> {
        let aggregate: UsageAggregate = sqlx::query_as(
            r#"
            SELECT
                COALESCE(SUM(task_minutes), 0) as total_task_minutes,
                COALESCE(SUM(streams), 0) as total_streams,
                COALESCE(SUM(bytes), 0) as total_bytes,
                COALESCE(SUM(tasks_created), 0) as total_tasks_created
            FROM usage_counters
            WHERE tenant_id = $1
              AND period >= $2
              AND period <= $3
            "#,
        )
        .bind(tenant_id)
        .bind(start_date)
        .bind(end_date)
        .fetch_one(pool)
        .await?;

        Ok(aggregate)
    }

    /// Resets usage counters for a specific period
    ///
    /// ⚠️  Use with caution! This is primarily for testing or correcting errors.
    pub async fn reset_period(
        pool: &PgPool,
        tenant_id: Uuid,
        period: NaiveDate,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM usage_counters WHERE tenant_id = $1 AND period = $2"
        )
        .bind(tenant_id)
        .bind(period)
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Deletes old usage records (for data retention policies)
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `before_date` - Delete records before this date
    ///
    /// # Returns
    ///
    /// Number of records deleted
    pub async fn delete_before(pool: &PgPool, before_date: NaiveDate) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM usage_counters WHERE period < $1")
            .bind(before_date)
            .execute(pool)
            .await?;

        Ok(result.rows_affected())
    }
}

/// Aggregated usage statistics
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UsageAggregate {
    /// Total task minutes
    pub total_task_minutes: i64,

    /// Total streams
    pub total_streams: i64,

    /// Total bytes
    pub total_bytes: i64,

    /// Total tasks created
    pub total_tasks_created: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_today() {
        let today = UsageCounter::today();
        let expected = Utc::now().date_naive();
        assert_eq!(today, expected);
    }

    // Integration tests for database operations are in tests/models/usage_tests.rs
}
