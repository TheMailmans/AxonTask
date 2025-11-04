/// Task model and database operations
///
/// This module provides the Task model representing background tasks executed by workers.
/// Tasks are the core entity of the AxonTask system.
///
/// # State Machine
///
/// ```text
/// pending → running → succeeded
///                  → failed
///                  → timeout
/// pending → canceled
/// running → canceled
/// ```
///
/// # Schema
///
/// ```sql
/// CREATE TYPE task_state AS ENUM (
///     'pending', 'running', 'succeeded', 'failed', 'canceled', 'timeout'
/// );
///
/// CREATE TABLE tasks (
///     id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
///     tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
///     created_by UUID REFERENCES users(id) ON DELETE SET NULL,
///     name VARCHAR(255) NOT NULL,
///     adapter VARCHAR(50) NOT NULL,
///     args JSONB NOT NULL DEFAULT '{}',
///     state task_state NOT NULL DEFAULT 'pending',
///     started_at TIMESTAMPTZ,
///     ended_at TIMESTAMPTZ,
///     cursor BIGINT NOT NULL DEFAULT 0,
///     bytes_streamed BIGINT NOT NULL DEFAULT 0,
///     minutes_used INTEGER NOT NULL DEFAULT 0,
///     timeout_seconds INTEGER NOT NULL DEFAULT 3600,
///     error_message TEXT,
///     exit_code INTEGER,
///     created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
///     updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
/// );
/// ```
///
/// # Example
///
/// ```no_run
/// use axontask_shared::models::task::{Task, CreateTask, TaskState};
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
/// use serde_json::json;
/// use uuid::Uuid;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let pool = create_pool(DatabaseConfig::default()).await?;
///
/// let task = Task::create(&pool, CreateTask {
///     tenant_id: Uuid::new_v4(),
///     created_by: Some(Uuid::new_v4()),
///     name: "Deploy app".to_string(),
///     adapter: "fly".to_string(),
///     args: json!({"app": "myapp", "region": "iad"}),
///     timeout_seconds: 900,
/// }).await?;
///
/// // Start the task
/// Task::transition_to_running(&pool, task.id).await?;
/// # Ok(())
/// # }
/// ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

/// Task execution state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "task_state", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    /// Task is queued, waiting for a worker
    Pending,

    /// Task is currently being executed by a worker
    Running,

    /// Task completed successfully
    Succeeded,

    /// Task failed with an error
    Failed,

    /// Task was canceled by user or system
    Canceled,

    /// Task exceeded timeout limit
    Timeout,
}

impl TaskState {
    /// Converts state to string for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Pending => "pending",
            TaskState::Running => "running",
            TaskState::Succeeded => "succeeded",
            TaskState::Failed => "failed",
            TaskState::Canceled => "canceled",
            TaskState::Timeout => "timeout",
        }
    }

    /// Checks if state is terminal (task has finished)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Succeeded | TaskState::Failed | TaskState::Canceled | TaskState::Timeout
        )
    }

    /// Checks if state is active (task is in progress)
    pub fn is_active(&self) -> bool {
        matches!(self, TaskState::Pending | TaskState::Running)
    }

    /// Checks if transition to target state is valid
    pub fn can_transition_to(&self, target: TaskState) -> bool {
        match (self, target) {
            // Pending can go to running or canceled
            (TaskState::Pending, TaskState::Running) => true,
            (TaskState::Pending, TaskState::Canceled) => true,

            // Running can go to succeeded, failed, timeout, or canceled
            (TaskState::Running, TaskState::Succeeded) => true,
            (TaskState::Running, TaskState::Failed) => true,
            (TaskState::Running, TaskState::Timeout) => true,
            (TaskState::Running, TaskState::Canceled) => true,

            // Terminal states cannot transition
            _ => false,
        }
    }
}

/// Task model representing a background task
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Task {
    /// Unique task ID
    pub id: Uuid,

    /// Tenant this task belongs to
    pub tenant_id: Uuid,

    /// User who created the task (nullable if user deleted)
    pub created_by: Option<Uuid>,

    /// Human-readable task name
    pub name: String,

    /// Adapter to execute the task (e.g., "shell", "docker", "fly")
    pub adapter: String,

    /// Adapter-specific arguments (JSON)
    pub args: JsonValue,

    /// Current execution state
    #[sqlx(try_from = "String")]
    pub state: String,

    /// When the task started executing (null if not started)
    pub started_at: Option<DateTime<Utc>>,

    /// When the task finished (null if not finished)
    pub ended_at: Option<DateTime<Utc>>,

    /// Last event sequence number (for resumable streaming)
    pub cursor: i64,

    /// Total bytes streamed via SSE (for usage tracking)
    pub bytes_streamed: i64,

    /// Task execution time in minutes (rounded up, for billing)
    pub minutes_used: i32,

    /// Timeout in seconds (default 3600 = 1 hour)
    pub timeout_seconds: i32,

    /// Error message (if state is Failed)
    pub error_message: Option<String>,

    /// Exit code (if applicable)
    pub exit_code: Option<i32>,

    /// When the task was created
    pub created_at: DateTime<Utc>,

    /// When the task was last updated
    pub updated_at: DateTime<Utc>,
}

/// Input for creating a new task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTask {
    /// Tenant ID
    pub tenant_id: Uuid,

    /// User who created the task
    pub created_by: Option<Uuid>,

    /// Task name
    pub name: String,

    /// Adapter to use
    pub adapter: String,

    /// Adapter-specific arguments
    pub args: JsonValue,

    /// Timeout in seconds (default 3600)
    #[serde(default = "default_timeout")]
    pub timeout_seconds: i32,
}

fn default_timeout() -> i32 {
    3600 // 1 hour
}

/// Input for updating a task
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateTask {
    /// Update cursor (last event seq)
    pub cursor: Option<i64>,

    /// Update bytes streamed
    pub bytes_streamed: Option<i64>,

    /// Update minutes used
    pub minutes_used: Option<i32>,

    /// Set error message
    pub error_message: Option<String>,

    /// Set exit code
    pub exit_code: Option<i32>,
}

impl Task {
    /// Creates a new task in pending state
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `data` - Task creation data
    ///
    /// # Returns
    ///
    /// The newly created task
    ///
    /// # Errors
    ///
    /// Returns an error if database operation fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::task::{Task, CreateTask};
    /// # use sqlx::PgPool;
    /// # use serde_json::json;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let task = Task::create(&pool, CreateTask {
    ///     tenant_id: Uuid::new_v4(),
    ///     created_by: Some(Uuid::new_v4()),
    ///     name: "Deploy app".to_string(),
    ///     adapter: "fly".to_string(),
    ///     args: json!({"app": "myapp"}),
    ///     timeout_seconds: 900,
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(pool: &PgPool, data: CreateTask) -> Result<Self, sqlx::Error> {
        let task = sqlx::query_as::<_, Task>(
            r#"
            INSERT INTO tasks (tenant_id, created_by, name, adapter, args, timeout_seconds)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, tenant_id, created_by, name, adapter, args, state,
                      started_at, ended_at, cursor, bytes_streamed, minutes_used,
                      timeout_seconds, error_message, exit_code, created_at, updated_at
            "#,
        )
        .bind(data.tenant_id)
        .bind(data.created_by)
        .bind(data.name)
        .bind(data.adapter)
        .bind(data.args)
        .bind(data.timeout_seconds)
        .fetch_one(pool)
        .await?;

        Ok(task)
    }

    /// Finds a task by ID
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let task = sqlx::query_as::<_, Task>(
            r#"
            SELECT id, tenant_id, created_by, name, adapter, args, state,
                   started_at, ended_at, cursor, bytes_streamed, minutes_used,
                   timeout_seconds, error_message, exit_code, created_at, updated_at
            FROM tasks
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Finds a task by ID with tenant isolation
    ///
    /// This is the preferred method for API endpoints to ensure tenant isolation.
    pub async fn find_by_id_and_tenant(
        pool: &PgPool,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        let task = sqlx::query_as::<_, Task>(
            r#"
            SELECT id, tenant_id, created_by, name, adapter, args, state,
                   started_at, ended_at, cursor, bytes_streamed, minutes_used,
                   timeout_seconds, error_message, exit_code, created_at, updated_at
            FROM tasks
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Transitions task to running state
    ///
    /// Sets started_at timestamp. Validates state transition.
    pub async fn transition_to_running(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let task = sqlx::query_as::<_, Task>(
            r#"
            UPDATE tasks
            SET state = 'running',
                started_at = NOW(),
                updated_at = NOW()
            WHERE id = $1 AND state = 'pending'
            RETURNING id, tenant_id, created_by, name, adapter, args, state,
                      started_at, ended_at, cursor, bytes_streamed, minutes_used,
                      timeout_seconds, error_message, exit_code, created_at, updated_at
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Transitions task to succeeded state
    ///
    /// Sets ended_at timestamp. Validates state transition.
    pub async fn transition_to_succeeded(
        pool: &PgPool,
        id: Uuid,
        exit_code: Option<i32>,
    ) -> Result<Option<Self>, sqlx::Error> {
        let task = sqlx::query_as::<_, Task>(
            r#"
            UPDATE tasks
            SET state = 'succeeded',
                ended_at = NOW(),
                exit_code = $2,
                updated_at = NOW()
            WHERE id = $1 AND state = 'running'
            RETURNING id, tenant_id, created_by, name, adapter, args, state,
                      started_at, ended_at, cursor, bytes_streamed, minutes_used,
                      timeout_seconds, error_message, exit_code, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(exit_code)
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Transitions task to failed state
    ///
    /// Sets ended_at timestamp and error message. Validates state transition.
    pub async fn transition_to_failed(
        pool: &PgPool,
        id: Uuid,
        error_message: &str,
        exit_code: Option<i32>,
    ) -> Result<Option<Self>, sqlx::Error> {
        let task = sqlx::query_as::<_, Task>(
            r#"
            UPDATE tasks
            SET state = 'failed',
                ended_at = NOW(),
                error_message = $2,
                exit_code = $3,
                updated_at = NOW()
            WHERE id = $1 AND state = 'running'
            RETURNING id, tenant_id, created_by, name, adapter, args, state,
                      started_at, ended_at, cursor, bytes_streamed, minutes_used,
                      timeout_seconds, error_message, exit_code, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(error_message)
        .bind(exit_code)
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Transitions task to canceled state
    ///
    /// Can be called from pending or running state.
    pub async fn transition_to_canceled(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let task = sqlx::query_as::<_, Task>(
            r#"
            UPDATE tasks
            SET state = 'canceled',
                ended_at = NOW(),
                updated_at = NOW()
            WHERE id = $1 AND state IN ('pending', 'running')
            RETURNING id, tenant_id, created_by, name, adapter, args, state,
                      started_at, ended_at, cursor, bytes_streamed, minutes_used,
                      timeout_seconds, error_message, exit_code, created_at, updated_at
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Transitions task to timeout state
    pub async fn transition_to_timeout(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let task = sqlx::query_as::<_, Task>(
            r#"
            UPDATE tasks
            SET state = 'timeout',
                ended_at = NOW(),
                error_message = 'Task exceeded timeout limit',
                updated_at = NOW()
            WHERE id = $1 AND state = 'running'
            RETURNING id, tenant_id, created_by, name, adapter, args, state,
                      started_at, ended_at, cursor, bytes_streamed, minutes_used,
                      timeout_seconds, error_message, exit_code, created_at, updated_at
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Updates task statistics (cursor, bytes, minutes)
    pub async fn update_stats(
        pool: &PgPool,
        id: Uuid,
        data: UpdateTask,
    ) -> Result<Option<Self>, sqlx::Error> {
        let mut query = String::from("UPDATE tasks SET updated_at = NOW()");
        let mut bind_count = 1;

        if data.cursor.is_some() {
            bind_count += 1;
            query.push_str(&format!(", cursor = ${}", bind_count));
        }
        if data.bytes_streamed.is_some() {
            bind_count += 1;
            query.push_str(&format!(", bytes_streamed = ${}", bind_count));
        }
        if data.minutes_used.is_some() {
            bind_count += 1;
            query.push_str(&format!(", minutes_used = ${}", bind_count));
        }

        query.push_str(" WHERE id = $1 RETURNING id, tenant_id, created_by, name, adapter, args, state, started_at, ended_at, cursor, bytes_streamed, minutes_used, timeout_seconds, error_message, exit_code, created_at, updated_at");

        let mut q = sqlx::query_as::<_, Task>(&query).bind(id);

        if let Some(cursor) = data.cursor {
            q = q.bind(cursor);
        }
        if let Some(bytes) = data.bytes_streamed {
            q = q.bind(bytes);
        }
        if let Some(minutes) = data.minutes_used {
            q = q.bind(minutes);
        }

        let task = q.fetch_optional(pool).await?;

        Ok(task)
    }

    /// Lists tasks for a tenant with pagination
    pub async fn list_by_tenant(
        pool: &PgPool,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let tasks = sqlx::query_as::<_, Task>(
            r#"
            SELECT id, tenant_id, created_by, name, adapter, args, state,
                   started_at, ended_at, cursor, bytes_streamed, minutes_used,
                   timeout_seconds, error_message, exit_code, created_at, updated_at
            FROM tasks
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Lists tasks by state
    pub async fn list_by_state(
        pool: &PgPool,
        tenant_id: Uuid,
        state: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let tasks = sqlx::query_as::<_, Task>(
            r#"
            SELECT id, tenant_id, created_by, name, adapter, args, state,
                   started_at, ended_at, cursor, bytes_streamed, minutes_used,
                   timeout_seconds, error_message, exit_code, created_at, updated_at
            FROM tasks
            WHERE tenant_id = $1 AND state = $2
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(tenant_id)
        .bind(state)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Gets pending tasks for worker to process
    ///
    /// Returns tasks in pending state ordered by creation (FIFO).
    pub async fn get_pending_tasks(pool: &PgPool, limit: i64) -> Result<Vec<Self>, sqlx::Error> {
        let tasks = sqlx::query_as::<_, Task>(
            r#"
            SELECT id, tenant_id, created_by, name, adapter, args, state,
                   started_at, ended_at, cursor, bytes_streamed, minutes_used,
                   timeout_seconds, error_message, exit_code, created_at, updated_at
            FROM tasks
            WHERE state = 'pending'
            ORDER BY created_at ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Counts tasks by tenant
    pub async fn count_by_tenant(pool: &PgPool, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM tasks WHERE tenant_id = $1"
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;

        Ok(count)
    }

    /// Counts tasks by state and tenant
    pub async fn count_by_state(
        pool: &PgPool,
        tenant_id: Uuid,
        state: &str,
    ) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM tasks WHERE tenant_id = $1 AND state = $2"
        )
        .bind(tenant_id)
        .bind(state)
        .fetch_one(pool)
        .await?;

        Ok(count)
    }

    /// Deletes a task
    ///
    /// ⚠️  This also deletes all related events due to CASCADE.
    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM tasks WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_state_as_str() {
        assert_eq!(TaskState::Pending.as_str(), "pending");
        assert_eq!(TaskState::Running.as_str(), "running");
        assert_eq!(TaskState::Succeeded.as_str(), "succeeded");
        assert_eq!(TaskState::Failed.as_str(), "failed");
        assert_eq!(TaskState::Canceled.as_str(), "canceled");
        assert_eq!(TaskState::Timeout.as_str(), "timeout");
    }

    #[test]
    fn test_task_state_is_terminal() {
        assert!(!TaskState::Pending.is_terminal());
        assert!(!TaskState::Running.is_terminal());
        assert!(TaskState::Succeeded.is_terminal());
        assert!(TaskState::Failed.is_terminal());
        assert!(TaskState::Canceled.is_terminal());
        assert!(TaskState::Timeout.is_terminal());
    }

    #[test]
    fn test_task_state_is_active() {
        assert!(TaskState::Pending.is_active());
        assert!(TaskState::Running.is_active());
        assert!(!TaskState::Succeeded.is_active());
        assert!(!TaskState::Failed.is_active());
        assert!(!TaskState::Canceled.is_active());
        assert!(!TaskState::Timeout.is_active());
    }

    #[test]
    fn test_task_state_transitions() {
        // Pending transitions
        assert!(TaskState::Pending.can_transition_to(TaskState::Running));
        assert!(TaskState::Pending.can_transition_to(TaskState::Canceled));
        assert!(!TaskState::Pending.can_transition_to(TaskState::Succeeded));

        // Running transitions
        assert!(TaskState::Running.can_transition_to(TaskState::Succeeded));
        assert!(TaskState::Running.can_transition_to(TaskState::Failed));
        assert!(TaskState::Running.can_transition_to(TaskState::Timeout));
        assert!(TaskState::Running.can_transition_to(TaskState::Canceled));

        // Terminal states cannot transition
        assert!(!TaskState::Succeeded.can_transition_to(TaskState::Running));
        assert!(!TaskState::Failed.can_transition_to(TaskState::Running));
        assert!(!TaskState::Canceled.can_transition_to(TaskState::Running));
    }

    #[test]
    fn test_default_timeout() {
        assert_eq!(default_timeout(), 3600);
    }
}
