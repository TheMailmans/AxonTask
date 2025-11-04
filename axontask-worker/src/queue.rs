/// Task queue reader
///
/// This module handles polling the database for pending tasks and providing
/// them to the worker orchestrator for execution.
///
/// # Architecture
///
/// The queue reader:
/// 1. Polls database for tasks in "pending" state
/// 2. Claims tasks atomically (updates state to "running")
/// 3. Returns claimed tasks to orchestrator
/// 4. Supports prioritization (future)
///
/// # Polling Strategy
///
/// - Poll interval: 1 second (configurable)
/// - Batch size: 10 tasks (configurable)
/// - Ordering: FIFO (created_at ASC)
///
/// # Example
///
/// ```no_run
/// use axontask_worker::queue::TaskQueue;
/// use sqlx::PgPool;
///
/// # async fn example(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
/// let queue = TaskQueue::new(pool);
///
/// loop {
///     let tasks = queue.claim_tasks(5).await?;
///     for task in tasks {
///         println!("Claimed task: {}", task.id);
///         // Execute task...
///     }
///     tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
/// }
/// # Ok(())
/// # }
/// ```

use axontask_shared::models::task::{Task, TaskState};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

/// Task queue error
#[derive(Debug, Error)]
pub enum QueueError {
    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Task not found
    #[error("Task not found: {0}")]
    TaskNotFound(Uuid),

    /// Invalid state transition
    #[error("Invalid state transition: {0}")]
    InvalidStateTransition(String),
}

/// Task queue reader
///
/// Polls database for pending tasks and claims them for execution.
pub struct TaskQueue {
    /// Database connection pool
    db: PgPool,

    /// Maximum tasks to claim in one batch
    batch_size: usize,
}

impl TaskQueue {
    /// Creates a new task queue
    ///
    /// # Arguments
    ///
    /// * `db` - Database connection pool
    pub fn new(db: PgPool) -> Self {
        TaskQueue { db, batch_size: 10 }
    }

    /// Creates a new task queue with custom batch size
    ///
    /// # Arguments
    ///
    /// * `db` - Database connection pool
    /// * `batch_size` - Maximum tasks to claim per batch
    pub fn with_batch_size(db: PgPool, batch_size: usize) -> Self {
        TaskQueue { db, batch_size }
    }

    /// Claims pending tasks for execution
    ///
    /// Atomically transitions tasks from "pending" to "running" state
    /// and returns them for execution.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of tasks to claim (defaults to batch_size if None)
    ///
    /// # Returns
    ///
    /// Vec of claimed tasks
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_worker::queue::TaskQueue;
    /// # use sqlx::PgPool;
    /// # async fn example(queue: TaskQueue) -> Result<(), Box<dyn std::error::Error>> {
    /// let tasks = queue.claim_tasks(Some(5)).await?;
    /// println!("Claimed {} tasks", tasks.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn claim_tasks(&self, limit: Option<usize>) -> Result<Vec<Task>, QueueError> {
        let limit = limit.unwrap_or(self.batch_size) as i64;

        // Use advisory locks to prevent race conditions
        // Each worker tries to claim different tasks
        let tasks = sqlx::query_as::<_, Task>(
            r#"
            WITH pending_tasks AS (
                SELECT id
                FROM tasks
                WHERE state = $1
                ORDER BY created_at ASC
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
            UPDATE tasks
            SET
                state = $3,
                started_at = NOW(),
                updated_at = NOW()
            FROM pending_tasks
            WHERE tasks.id = pending_tasks.id
            RETURNING
                tasks.id,
                tasks.tenant_id,
                tasks.name,
                tasks.adapter,
                tasks.args,
                tasks.state,
                tasks.timeout_seconds,
                tasks.tags,
                tasks.created_at,
                tasks.started_at,
                tasks.ended_at,
                tasks.last_seq,
                tasks.last_event_id,
                tasks.exit_code,
                tasks.error
            "#,
        )
        .bind(TaskState::Pending.as_str())
        .bind(limit)
        .bind(TaskState::Running.as_str())
        .fetch_all(&self.db)
        .await?;

        if !tasks.is_empty() {
            tracing::info!(count = tasks.len(), "Claimed tasks");
        }

        Ok(tasks)
    }

    /// Gets pending task count
    ///
    /// Returns the number of tasks currently in "pending" state.
    ///
    /// # Returns
    ///
    /// Count of pending tasks
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn pending_count(&self) -> Result<i64, QueueError> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM tasks
            WHERE state = $1
            "#,
        )
        .bind(TaskState::Pending.as_str())
        .fetch_one(&self.db)
        .await?;

        Ok(count)
    }

    /// Gets running task count
    ///
    /// Returns the number of tasks currently in "running" state.
    ///
    /// # Returns
    ///
    /// Count of running tasks
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn running_count(&self) -> Result<i64, QueueError> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM tasks
            WHERE state = $1
            "#,
        )
        .bind(TaskState::Running.as_str())
        .fetch_one(&self.db)
        .await?;

        Ok(count)
    }

    /// Marks a task as succeeded
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `exit_code` - Optional exit code
    ///
    /// # Errors
    ///
    /// Returns error if task not found or database query fails
    pub async fn mark_succeeded(&self, task_id: Uuid, exit_code: Option<i32>) -> Result<(), QueueError> {
        let result = sqlx::query(
            r#"
            UPDATE tasks
            SET
                state = $2,
                ended_at = NOW(),
                updated_at = NOW(),
                exit_code = $3
            WHERE id = $1 AND state = $4
            "#,
        )
        .bind(task_id)
        .bind(TaskState::Succeeded.as_str())
        .bind(exit_code)
        .bind(TaskState::Running.as_str())
        .execute(&self.db)
        .await?;

        if result.rows_affected() == 0 {
            return Err(QueueError::TaskNotFound(task_id));
        }

        tracing::info!(task_id = %task_id, "Task marked as succeeded");
        Ok(())
    }

    /// Marks a task as failed
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `error` - Error message
    ///
    /// # Errors
    ///
    /// Returns error if task not found or database query fails
    pub async fn mark_failed(&self, task_id: Uuid, error: String) -> Result<(), QueueError> {
        let result = sqlx::query(
            r#"
            UPDATE tasks
            SET
                state = $2,
                ended_at = NOW(),
                updated_at = NOW(),
                error = $3
            WHERE id = $1 AND state = $4
            "#,
        )
        .bind(task_id)
        .bind(TaskState::Failed.as_str())
        .bind(error)
        .bind(TaskState::Running.as_str())
        .execute(&self.db)
        .await?;

        if result.rows_affected() == 0 {
            return Err(QueueError::TaskNotFound(task_id));
        }

        tracing::warn!(task_id = %task_id, "Task marked as failed");
        Ok(())
    }

    /// Marks a task as timed out
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    ///
    /// # Errors
    ///
    /// Returns error if task not found or database query fails
    pub async fn mark_timeout(&self, task_id: Uuid) -> Result<(), QueueError> {
        let result = sqlx::query(
            r#"
            UPDATE tasks
            SET
                state = $2,
                ended_at = NOW(),
                updated_at = NOW(),
                error = 'Task timed out'
            WHERE id = $1 AND state = $4
            "#,
        )
        .bind(task_id)
        .bind(TaskState::Timeout.as_str())
        .bind(TaskState::Running.as_str())
        .execute(&self.db)
        .await?;

        if result.rows_affected() == 0 {
            return Err(QueueError::TaskNotFound(task_id));
        }

        tracing::warn!(task_id = %task_id, "Task marked as timed out");
        Ok(())
    }

    /// Updates task last sequence number
    ///
    /// Called after each event emission to track progress.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `seq` - Sequence number
    /// * `event_id` - Redis Stream event ID
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn update_last_seq(
        &self,
        task_id: Uuid,
        seq: i64,
        event_id: String,
    ) -> Result<(), QueueError> {
        sqlx::query(
            r#"
            UPDATE tasks
            SET
                last_seq = $2,
                last_event_id = $3,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(task_id)
        .bind(seq)
        .bind(event_id)
        .execute(&self.db)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_queue_new() {
        // This is just a compile test
        // Real integration tests with database are in tests/queue_tests.rs
    }

    #[test]
    fn test_task_queue_with_batch_size() {
        // Compile test for custom batch size
    }

    // Integration tests with actual database are in tests/queue_tests.rs
}
