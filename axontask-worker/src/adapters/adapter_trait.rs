/// Core Adapter trait and types
///
/// This module defines the contract that all task adapters must implement.
/// Adapters are responsible for executing tasks in specific environments
/// and emitting events to track progress.
///
/// # Adapter Contract
///
/// All adapters must:
/// 1. Implement the `Adapter` trait (async)
/// 2. Accept an `AdapterContext` with task metadata and arguments
/// 3. Emit events via the provided event channel
/// 4. Support cancellation via the cancel token
/// 5. Clean up resources on completion or cancellation
///
/// # Event Flow
///
/// ```text
/// Adapter::execute()
///   ├─> Emit "started" event
///   ├─> Emit progress events (stdout, stderr, progress, etc.)
///   ├─> Emit "completed" event on success
///   └─> Emit "failed" event on error
/// ```
///
/// # Cancellation
///
/// Adapters must check the cancel token regularly and clean up resources
/// when cancellation is requested.
///
/// # Example
///
/// ```no_run
/// use axontask_worker::adapters::{Adapter, AdapterContext, AdapterResult, AdapterEvent, AdapterEventKind};
/// use async_trait::async_trait;
/// use tokio::sync::mpsc;
/// use tokio_util::sync::CancellationToken;
/// use uuid::Uuid;
///
/// struct MyAdapter;
///
/// #[async_trait]
/// impl Adapter for MyAdapter {
///     fn name(&self) -> &str {
///         "my_adapter"
///     }
///
///     async fn execute(&self, context: AdapterContext) -> AdapterResult<()> {
///         // Emit started event
///         context.emit(AdapterEvent::new(AdapterEventKind::Started, serde_json::json!({}))).await?;
///
///         // Do work...
///         if context.cancel_token.is_cancelled() {
///             return Ok(()); // Cancelled
///         }
///
///         // Emit completed event
///         context.emit(AdapterEvent::new(AdapterEventKind::Completed, serde_json::json!({}))).await?;
///         Ok(())
///     }
/// }
/// ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Adapter error types
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    /// Task execution failed
    #[error("Task execution failed: {0}")]
    ExecutionFailed(String),

    /// Task was cancelled
    #[error("Task was cancelled")]
    Cancelled,

    /// Invalid task arguments
    #[error("Invalid task arguments: {0}")]
    InvalidArguments(String),

    /// Timeout exceeded
    #[error("Task timeout exceeded")]
    Timeout,

    /// Resource limit exceeded
    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),

    /// Event emission failed
    #[error("Failed to emit event: {0}")]
    EventEmissionFailed(String),

    /// Internal adapter error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Adapter result type alias
pub type AdapterResult<T> = Result<T, AdapterError>;

/// Adapter event kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterEventKind {
    /// Task started
    Started,

    /// Progress update
    Progress,

    /// Standard output
    Stdout,

    /// Standard error
    Stderr,

    /// Task completed successfully
    Completed,

    /// Task failed
    Failed,

    /// Task cancelled
    Cancelled,

    /// Timeout exceeded
    Timeout,

    /// Custom adapter-specific event
    Custom,
}

impl fmt::Display for AdapterEventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterEventKind::Started => write!(f, "started"),
            AdapterEventKind::Progress => write!(f, "progress"),
            AdapterEventKind::Stdout => write!(f, "stdout"),
            AdapterEventKind::Stderr => write!(f, "stderr"),
            AdapterEventKind::Completed => write!(f, "completed"),
            AdapterEventKind::Failed => write!(f, "failed"),
            AdapterEventKind::Cancelled => write!(f, "cancelled"),
            AdapterEventKind::Timeout => write!(f, "timeout"),
            AdapterEventKind::Custom => write!(f, "custom"),
        }
    }
}

/// Adapter event
///
/// Events emitted by adapters during task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterEvent {
    /// Event kind
    pub kind: AdapterEventKind,

    /// Event payload (adapter-specific data)
    pub payload: JsonValue,
}

impl AdapterEvent {
    /// Creates a new adapter event
    pub fn new(kind: AdapterEventKind, payload: JsonValue) -> Self {
        AdapterEvent { kind, payload }
    }

    /// Creates a started event
    pub fn started(metadata: JsonValue) -> Self {
        AdapterEvent::new(AdapterEventKind::Started, metadata)
    }

    /// Creates a progress event
    pub fn progress(percent: u8, message: Option<String>) -> Self {
        AdapterEvent::new(
            AdapterEventKind::Progress,
            serde_json::json!({
                "percent": percent,
                "message": message,
            }),
        )
    }

    /// Creates a stdout event
    pub fn stdout(data: String) -> Self {
        AdapterEvent::new(AdapterEventKind::Stdout, serde_json::json!({ "data": data }))
    }

    /// Creates a stderr event
    pub fn stderr(data: String) -> Self {
        AdapterEvent::new(AdapterEventKind::Stderr, serde_json::json!({ "data": data }))
    }

    /// Creates a completed event
    pub fn completed(metadata: JsonValue) -> Self {
        AdapterEvent::new(AdapterEventKind::Completed, metadata)
    }

    /// Creates a failed event
    pub fn failed(error: String) -> Self {
        AdapterEvent::new(
            AdapterEventKind::Failed,
            serde_json::json!({ "error": error }),
        )
    }

    /// Creates a cancelled event
    pub fn cancelled() -> Self {
        AdapterEvent::new(AdapterEventKind::Cancelled, serde_json::json!({}))
    }

    /// Creates a timeout event
    pub fn timeout() -> Self {
        AdapterEvent::new(AdapterEventKind::Timeout, serde_json::json!({}))
    }
}

/// Adapter execution context
///
/// Provides task metadata, arguments, and communication channels to the adapter.
pub struct AdapterContext {
    /// Task ID
    pub task_id: Uuid,

    /// Task arguments (adapter-specific)
    pub args: JsonValue,

    /// Event sender
    event_tx: mpsc::UnboundedSender<AdapterEvent>,

    /// Cancellation token
    pub cancel_token: CancellationToken,
}

impl AdapterContext {
    /// Creates a new adapter context
    pub fn new(
        task_id: Uuid,
        args: JsonValue,
        event_tx: mpsc::UnboundedSender<AdapterEvent>,
        cancel_token: CancellationToken,
    ) -> Self {
        AdapterContext {
            task_id,
            args,
            event_tx,
            cancel_token,
        }
    }

    /// Emits an event
    ///
    /// # Errors
    ///
    /// Returns error if event channel is closed
    pub async fn emit(&self, event: AdapterEvent) -> AdapterResult<()> {
        self.event_tx
            .send(event)
            .map_err(|_| AdapterError::EventEmissionFailed("Event channel closed".to_string()))
    }

    /// Checks if cancellation was requested
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Waits for cancellation
    pub async fn cancelled(&self) {
        self.cancel_token.cancelled().await
    }
}

/// Core Adapter trait
///
/// All task adapters must implement this trait.
#[async_trait]
pub trait Adapter: Send + Sync {
    /// Returns the adapter name
    ///
    /// Used for registry lookup and logging.
    fn name(&self) -> &str;

    /// Executes a task
    ///
    /// The adapter should:
    /// 1. Validate task arguments
    /// 2. Emit a "started" event
    /// 3. Execute the task
    /// 4. Emit progress events
    /// 5. Check cancellation token regularly
    /// 6. Emit "completed" or "failed" event
    /// 7. Clean up resources
    ///
    /// # Arguments
    ///
    /// * `context` - Task context with metadata and event channel
    ///
    /// # Returns
    ///
    /// Ok(()) if task completed successfully or was cancelled
    /// Err if task execution failed
    ///
    /// # Cancellation
    ///
    /// The adapter should check `context.is_cancelled()` regularly and
    /// return `Ok(())` when cancellation is detected.
    async fn execute(&self, context: AdapterContext) -> AdapterResult<()>;

    /// Validates task arguments
    ///
    /// Called before task execution to ensure arguments are valid.
    /// Default implementation accepts all arguments.
    ///
    /// # Arguments
    ///
    /// * `args` - Task arguments to validate
    ///
    /// # Returns
    ///
    /// Ok(()) if arguments are valid, Err otherwise
    fn validate_args(&self, _args: &JsonValue) -> AdapterResult<()> {
        Ok(())
    }

    /// Returns adapter metadata
    ///
    /// Optional method to provide adapter-specific metadata (version, capabilities, etc.)
    fn metadata(&self) -> JsonValue {
        serde_json::json!({
            "name": self.name(),
            "version": "1.0.0",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_event_kind_display() {
        assert_eq!(AdapterEventKind::Started.to_string(), "started");
        assert_eq!(AdapterEventKind::Progress.to_string(), "progress");
        assert_eq!(AdapterEventKind::Stdout.to_string(), "stdout");
        assert_eq!(AdapterEventKind::Stderr.to_string(), "stderr");
        assert_eq!(AdapterEventKind::Completed.to_string(), "completed");
        assert_eq!(AdapterEventKind::Failed.to_string(), "failed");
        assert_eq!(AdapterEventKind::Cancelled.to_string(), "cancelled");
        assert_eq!(AdapterEventKind::Timeout.to_string(), "timeout");
    }

    #[test]
    fn test_adapter_event_constructors() {
        let started = AdapterEvent::started(serde_json::json!({"adapter": "test"}));
        assert_eq!(started.kind, AdapterEventKind::Started);

        let progress = AdapterEvent::progress(50, Some("Halfway done".to_string()));
        assert_eq!(progress.kind, AdapterEventKind::Progress);
        assert_eq!(progress.payload["percent"], 50);

        let stdout = AdapterEvent::stdout("Hello World".to_string());
        assert_eq!(stdout.kind, AdapterEventKind::Stdout);
        assert_eq!(stdout.payload["data"], "Hello World");

        let stderr = AdapterEvent::stderr("Error message".to_string());
        assert_eq!(stderr.kind, AdapterEventKind::Stderr);

        let completed = AdapterEvent::completed(serde_json::json!({"exit_code": 0}));
        assert_eq!(completed.kind, AdapterEventKind::Completed);

        let failed = AdapterEvent::failed("Something went wrong".to_string());
        assert_eq!(failed.kind, AdapterEventKind::Failed);
        assert_eq!(failed.payload["error"], "Something went wrong");

        let cancelled = AdapterEvent::cancelled();
        assert_eq!(cancelled.kind, AdapterEventKind::Cancelled);

        let timeout = AdapterEvent::timeout();
        assert_eq!(timeout.kind, AdapterEventKind::Timeout);
    }

    #[test]
    fn test_adapter_event_serialization() {
        let event = AdapterEvent::progress(75, Some("Almost done".to_string()));
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"kind\":\"progress\""));
        assert!(json.contains("\"percent\":75"));

        let deserialized: AdapterEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.kind, AdapterEventKind::Progress);
    }

    #[test]
    fn test_adapter_context_cancellation() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let cancel_token = CancellationToken::new();
        let context = AdapterContext::new(Uuid::new_v4(), serde_json::json!({}), tx, cancel_token.clone());

        assert!(!context.is_cancelled());

        cancel_token.cancel();
        assert!(context.is_cancelled());
    }

    #[tokio::test]
    async fn test_adapter_context_emit() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let cancel_token = CancellationToken::new();
        let context = AdapterContext::new(
            Uuid::new_v4(),
            serde_json::json!({}),
            tx,
            cancel_token,
        );

        let event = AdapterEvent::stdout("test".to_string());
        context.emit(event.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.kind, AdapterEventKind::Stdout);
        assert_eq!(received.payload["data"], "test");
    }
}
