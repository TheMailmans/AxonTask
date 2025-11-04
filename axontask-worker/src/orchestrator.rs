/// Worker orchestrator
///
/// This module implements the main worker loop that coordinates task execution.
/// It polls the task queue, dispatches tasks to adapters, collects events,
/// emits them to Redis Streams, and updates task status.
///
/// # Architecture
///
/// ```text
/// Orchestrator
///   ├─> TaskQueue: Poll for pending tasks
///   ├─> AdapterRegistry: Get adapter for task
///   ├─> Adapter: Execute task
///   ├─> EventEmitter: Emit events to Redis
///   └─> TaskQueue: Update task status
/// ```
///
/// # Concurrency
///
/// The orchestrator runs multiple tasks concurrently using Tokio tasks.
/// Each task execution runs in its own async task.
///
/// # Example
///
/// ```no_run
/// use axontask_worker::orchestrator::WorkerOrchestrator;
/// use sqlx::PgPool;
/// use axontask_shared::redis::{RedisClient, RedisConfig};
///
/// # async fn example(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
/// let redis_config = RedisConfig::from_env()?;
/// let redis_client = RedisClient::new(redis_config).await?;
///
/// let orchestrator = WorkerOrchestrator::new(pool, redis_client);
///
/// // Start worker loop
/// orchestrator.run().await?;
/// # Ok(())
/// # }
/// ```

use crate::adapters::{Adapter, AdapterContext, AdapterEvent, MockAdapter};
use crate::events::EventEmitter;
use crate::queue::TaskQueue;
use axontask_shared::models::task::Task;
use axontask_shared::redis::RedisClient;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Worker orchestrator configuration
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Poll interval in seconds
    pub poll_interval_secs: u64,

    /// Maximum concurrent tasks
    pub max_concurrent_tasks: usize,

    /// Task claim batch size
    pub batch_size: usize,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        OrchestratorConfig {
            poll_interval_secs: 1,
            max_concurrent_tasks: 10,
            batch_size: 5,
        }
    }
}

/// Worker orchestrator
///
/// Coordinates task execution by polling the queue, dispatching to adapters,
/// and managing the task lifecycle.
pub struct WorkerOrchestrator {
    /// Task queue
    queue: TaskQueue,

    /// Event emitter
    emitter: Arc<EventEmitter>,

    /// Configuration
    config: OrchestratorConfig,

    /// Adapter registry
    adapters: HashMap<String, Arc<dyn Adapter>>,

    /// Shutdown token
    shutdown_token: CancellationToken,
}

impl WorkerOrchestrator {
    /// Creates a new worker orchestrator
    ///
    /// # Arguments
    ///
    /// * `db` - Database connection pool
    /// * `redis` - Redis client
    pub fn new(db: PgPool, redis: RedisClient) -> Self {
        let queue = TaskQueue::new(db);
        let emitter = Arc::new(EventEmitter::new(redis));
        let config = OrchestratorConfig::default();

        // Initialize adapter registry
        let mut adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".to_string(), Arc::new(MockAdapter::new()));

        WorkerOrchestrator {
            queue,
            emitter,
            config,
            adapters,
            shutdown_token: CancellationToken::new(),
        }
    }

    /// Creates a new worker orchestrator with custom configuration
    ///
    /// # Arguments
    ///
    /// * `db` - Database connection pool
    /// * `redis` - Redis client
    /// * `config` - Orchestrator configuration
    pub fn with_config(db: PgPool, redis: RedisClient, config: OrchestratorConfig) -> Self {
        let queue = TaskQueue::with_batch_size(db, config.batch_size);
        let emitter = Arc::new(EventEmitter::new(redis));

        // Initialize adapter registry
        let mut adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();
        adapters.insert("mock".to_string(), Arc::new(MockAdapter::new()));

        WorkerOrchestrator {
            queue,
            emitter,
            config,
            adapters,
            shutdown_token: CancellationToken::new(),
        }
    }

    /// Registers an adapter
    ///
    /// # Arguments
    ///
    /// * `adapter` - Adapter to register
    pub fn register_adapter(&mut self, adapter: Arc<dyn Adapter>) {
        let name = adapter.name().to_string();
        tracing::info!(adapter = %name, "Registering adapter");
        self.adapters.insert(name, adapter);
    }

    /// Gets shutdown token
    ///
    /// Used to signal graceful shutdown from external handlers.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    /// Runs the worker loop
    ///
    /// Continuously polls for tasks and executes them until shutdown.
    ///
    /// # Errors
    ///
    /// Returns error if fatal error occurs (database connection lost, etc.)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_worker::orchestrator::WorkerOrchestrator;
    /// # use sqlx::PgPool;
    /// # use axontask_shared::redis::RedisClient;
    /// # async fn example(orchestrator: WorkerOrchestrator) -> Result<(), Box<dyn std::error::Error>> {
    /// orchestrator.run().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("Worker orchestrator starting");

        // Track active tasks
        let mut active_tasks: HashMap<Uuid, CancellationToken> = HashMap::new();

        loop {
            // Check for shutdown
            if self.shutdown_token.is_cancelled() {
                tracing::info!("Shutdown requested, waiting for active tasks to complete");

                // Cancel all active tasks
                for (task_id, token) in active_tasks.iter() {
                    tracing::info!(task_id = %task_id, "Cancelling task for shutdown");
                    token.cancel();
                }

                // Wait for all tasks to complete (with timeout)
                let mut remaining = 30; // 30 seconds timeout
                while !active_tasks.is_empty() && remaining > 0 {
                    sleep(Duration::from_secs(1)).await;
                    remaining -= 1;

                    // Clean up completed tasks
                    active_tasks.retain(|_, token| !token.is_cancelled());
                }

                if !active_tasks.is_empty() {
                    tracing::warn!(count = active_tasks.len(), "Force shutdown with tasks still running");
                }

                tracing::info!("Worker orchestrator shut down");
                break;
            }

            // Check if we can claim more tasks
            let available_slots = self.config.max_concurrent_tasks.saturating_sub(active_tasks.len());
            if available_slots == 0 {
                // At capacity, wait before checking again
                sleep(Duration::from_millis(100)).await;
                continue;
            }

            // Claim tasks
            let tasks = match self.queue.claim_tasks(Some(available_slots)).await {
                Ok(tasks) => tasks,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to claim tasks");
                    sleep(Duration::from_secs(self.config.poll_interval_secs)).await;
                    continue;
                }
            };

            if tasks.is_empty() {
                // No tasks available, wait before polling again
                sleep(Duration::from_secs(self.config.poll_interval_secs)).await;
                continue;
            }

            // Dispatch tasks
            for task in tasks {
                let cancel_token = CancellationToken::new();
                active_tasks.insert(task.id, cancel_token.clone());

                self.dispatch_task(task, cancel_token).await;
            }

            // Clean up completed tasks
            active_tasks.retain(|_, token| !token.is_cancelled());
        }

        Ok(())
    }

    /// Dispatches a task for execution
    ///
    /// Spawns a Tokio task to execute the task asynchronously.
    async fn dispatch_task(&self, task: Task, cancel_token: CancellationToken) {
        let adapter = match self.adapters.get(&task.adapter) {
            Some(adapter) => adapter.clone(),
            None => {
                tracing::error!(
                    task_id = %task.id,
                    adapter = %task.adapter,
                    "Adapter not found"
                );
                // Mark task as failed
                if let Err(e) = self
                    .queue
                    .mark_failed(task.id, format!("Adapter not found: {}", task.adapter))
                    .await
                {
                    tracing::error!(error = %e, "Failed to mark task as failed");
                }
                return;
            }
        };

        let emitter = self.emitter.clone();
        let queue = self.queue.clone();

        // Spawn task execution
        tokio::spawn(async move {
            if let Err(e) = execute_task(task, adapter, emitter, queue, cancel_token).await {
                tracing::error!(error = %e, "Task execution failed");
            }
        });
    }
}

// Clone impl for TaskQueue (needed for spawning tasks)
impl Clone for TaskQueue {
    fn clone(&self) -> Self {
        TaskQueue {
            db: self.db.clone(),
            batch_size: self.batch_size,
        }
    }
}

/// Executes a single task
///
/// This function runs in its own Tokio task and handles the full lifecycle:
/// 1. Validate adapter arguments
/// 2. Create event channel
/// 3. Execute adapter
/// 4. Emit events to Redis
/// 5. Update task status
async fn execute_task(
    task: Task,
    adapter: Arc<dyn Adapter>,
    emitter: Arc<EventEmitter>,
    queue: TaskQueue,
    cancel_token: CancellationToken,
) -> anyhow::Result<()> {
    let task_id = task.id;
    let adapter_name = adapter.name();

    tracing::info!(
        task_id = %task_id,
        adapter = %adapter_name,
        "Executing task"
    );

    // Validate adapter arguments
    if let Err(e) = adapter.validate_args(&task.args) {
        tracing::error!(task_id = %task_id, error = %e, "Invalid adapter arguments");
        queue
            .mark_failed(task_id, format!("Invalid arguments: {}", e))
            .await?;
        return Ok(());
    }

    // Create event channel
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    // Create adapter context
    let context = AdapterContext::new(task_id, task.args.clone(), event_tx, cancel_token.clone());

    // Spawn adapter execution
    let adapter_handle = tokio::spawn(async move {
        adapter.execute(context).await
    });

    // Spawn event emission
    let emitter_clone = emitter.clone();
    let queue_clone = queue.clone();
    let event_handle = tokio::spawn(async move {
        let mut seq = 0;
        while let Some(event) = event_rx.recv().await {
            // Emit event to Redis
            match emitter_clone.emit(task_id, event).await {
                Ok(stream_id) => {
                    // Update last_seq in database
                    if let Err(e) = queue_clone.update_last_seq(task_id, seq, stream_id).await {
                        tracing::error!(error = %e, "Failed to update last_seq");
                    }
                    seq += 1;
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to emit event");
                }
            }
        }
    });

    // Wait for adapter to complete
    let adapter_result = adapter_handle.await?;

    // Wait for all events to be emitted
    drop(event_handle); // Close event channel
    sleep(Duration::from_millis(100)).await; // Give time for final events

    // Update task status based on result
    match adapter_result {
        Ok(()) => {
            // Check if cancelled
            if cancel_token.is_cancelled() {
                tracing::info!(task_id = %task_id, "Task cancelled");
                // Task state should already be updated by cancel handler
            } else {
                tracing::info!(task_id = %task_id, "Task succeeded");
                queue.mark_succeeded(task_id, Some(0)).await?;
            }
        }
        Err(e) => {
            tracing::error!(task_id = %task_id, error = %e, "Task failed");
            queue.mark_failed(task_id, e.to_string()).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_config_default() {
        let config = OrchestratorConfig::default();
        assert_eq!(config.poll_interval_secs, 1);
        assert_eq!(config.max_concurrent_tasks, 10);
        assert_eq!(config.batch_size, 5);
    }

    // Integration tests with actual database and Redis are in tests/orchestrator_tests.rs
}
