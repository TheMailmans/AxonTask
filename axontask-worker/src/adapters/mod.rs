/// Adapter system for task execution
///
/// This module defines the adapter trait and provides implementations for
/// various execution environments (shell, docker, fly.io, etc.).
///
/// # Architecture
///
/// Adapters are the execution layer of AxonTask. Each adapter:
/// - Implements the `Adapter` trait
/// - Executes tasks in a specific environment
/// - Emits events to track progress
/// - Supports cancellation and cleanup
///
/// # Adapter Types
///
/// - **Mock**: Deterministic fake events for testing/demo
/// - **Shell**: Execute shell commands (sandboxed)
/// - **Docker**: Build and run Docker containers
/// - **Fly**: Deploy to Fly.io
///
/// # Example
///
/// ```no_run
/// use axontask_worker::adapters::{Adapter, AdapterContext};
/// use uuid::Uuid;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Get adapter from registry
/// let adapter = get_adapter("shell")?;
///
/// // Execute task
/// let task_id = Uuid::new_v4();
/// let args = serde_json::json!({"command": "echo hello"});
/// let context = AdapterContext::new(task_id, args);
///
/// adapter.execute(context).await?;
/// # Ok(())
/// # }
/// # fn get_adapter(name: &str) -> Result<Box<dyn Adapter>, Box<dyn std::error::Error>> { unimplemented!() }
/// ```

pub mod adapter_trait;
// pub mod docker;
// pub mod fly;
pub mod mock;
// pub mod registry;
// pub mod shell;

// Re-export main types
pub use adapter_trait::{
    Adapter, AdapterContext, AdapterError, AdapterEvent, AdapterEventKind, AdapterResult,
};
pub use mock::MockAdapter;
