/// Mock adapter for testing and demos
///
/// This adapter emits deterministic fake events to simulate task execution.
/// It's useful for:
/// - Testing the worker system without external dependencies
/// - Demonstrating the event streaming system
/// - Load testing
///
/// # Event Sequence
///
/// The mock adapter emits the following event sequence:
/// 1. **started**: Task begins
/// 2. **progress (25%)**: "Initializing..."
/// 3. **stdout**: "Mock task starting..."
/// 4. **progress (50%)**: "Processing..."
/// 5. **stdout**: "Processing data..."
/// 6. **progress (75%)**: "Finalizing..."
/// 7. **stdout**: "Task complete!"
/// 8. **progress (100%)**: "Done"
/// 9. **completed**: Task finished
///
/// # Configuration
///
/// Arguments (JSON):
/// ```json
/// {
///   "duration_ms": 5000,     // Total execution time (default: 5000ms)
///   "should_fail": false,    // Whether to simulate failure (default: false)
///   "failure_percent": 50    // If should_fail=true, fail at this % (default: 50)
/// }
/// ```
///
/// # Example
///
/// ```no_run
/// use axontask_worker::adapters::{MockAdapter, Adapter, AdapterContext};
/// use tokio::sync::mpsc;
/// use tokio_util::sync::CancellationToken;
/// use uuid::Uuid;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let adapter = MockAdapter::new();
///
/// let (tx, mut rx) = mpsc::unbounded_channel();
/// let cancel_token = CancellationToken::new();
///
/// let args = serde_json::json!({
///     "duration_ms": 1000,
///     "should_fail": false
/// });
///
/// let context = AdapterContext::new(Uuid::new_v4(), args, tx, cancel_token);
/// adapter.execute(context).await?;
///
/// // Collect events
/// while let Some(event) = rx.recv().await {
///     println!("Event: {:?}", event.kind);
/// }
/// # Ok(())
/// # }
/// ```

use crate::adapters::{Adapter, AdapterContext, AdapterError, AdapterEvent, AdapterResult};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::time::{sleep, Duration};

/// Mock adapter configuration
#[derive(Debug, Clone, Deserialize)]
struct MockConfig {
    /// Total execution duration in milliseconds
    #[serde(default = "default_duration")]
    duration_ms: u64,

    /// Whether to simulate a failure
    #[serde(default)]
    should_fail: bool,

    /// At what percentage to fail (if should_fail=true)
    #[serde(default = "default_failure_percent")]
    failure_percent: u8,
}

fn default_duration() -> u64 {
    5000 // 5 seconds
}

fn default_failure_percent() -> u8 {
    50
}

impl Default for MockConfig {
    fn default() -> Self {
        MockConfig {
            duration_ms: default_duration(),
            should_fail: false,
            failure_percent: default_failure_percent(),
        }
    }
}

/// Mock adapter implementation
pub struct MockAdapter;

impl MockAdapter {
    /// Creates a new mock adapter
    pub fn new() -> Self {
        MockAdapter
    }
}

impl Default for MockAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for MockAdapter {
    fn name(&self) -> &str {
        "mock"
    }

    fn validate_args(&self, args: &serde_json::Value) -> AdapterResult<()> {
        // Try to parse config
        let config: MockConfig = serde_json::from_value(args.clone())
            .map_err(|e| AdapterError::InvalidArguments(format!("Invalid mock config: {}", e)))?;

        // Validate ranges
        if config.duration_ms == 0 {
            return Err(AdapterError::InvalidArguments(
                "duration_ms must be > 0".to_string(),
            ));
        }

        if config.duration_ms > 3600000 {
            // Max 1 hour
            return Err(AdapterError::InvalidArguments(
                "duration_ms must be <= 3600000 (1 hour)".to_string(),
            ));
        }

        if config.failure_percent > 100 {
            return Err(AdapterError::InvalidArguments(
                "failure_percent must be 0-100".to_string(),
            ));
        }

        Ok(())
    }

    async fn execute(&self, context: AdapterContext) -> AdapterResult<()> {
        tracing::info!(task_id = %context.task_id, "Mock adapter starting");

        // Parse configuration
        let config: MockConfig = serde_json::from_value(context.args.clone())
            .unwrap_or_default();

        // Emit started event
        context
            .emit(AdapterEvent::started(serde_json::json!({
                "adapter": "mock",
                "duration_ms": config.duration_ms,
                "should_fail": config.should_fail,
            })))
            .await?;

        // Calculate step duration
        let step_duration = Duration::from_millis(config.duration_ms / 4);

        // Progress checkpoints
        let checkpoints = [
            (25, "Initializing...", "Mock task starting..."),
            (50, "Processing...", "Processing data..."),
            (75, "Finalizing...", "Task complete!"),
            (100, "Done", ""),
        ];

        for (i, (percent, progress_msg, stdout_msg)) in checkpoints.iter().enumerate() {
            // Check cancellation
            if context.is_cancelled() {
                tracing::info!(task_id = %context.task_id, "Mock adapter cancelled");
                context.emit(AdapterEvent::cancelled()).await?;
                return Ok(());
            }

            // Check if we should fail at this checkpoint
            if config.should_fail && *percent >= config.failure_percent {
                tracing::warn!(
                    task_id = %context.task_id,
                    percent = *percent,
                    "Mock adapter simulating failure"
                );
                context
                    .emit(AdapterEvent::failed(format!(
                        "Simulated failure at {}%",
                        percent
                    )))
                    .await?;
                return Err(AdapterError::ExecutionFailed(format!(
                    "Simulated failure at {}%",
                    percent
                )));
            }

            // Emit progress event
            context
                .emit(AdapterEvent::progress(*percent, Some(progress_msg.to_string())))
                .await?;

            // Emit stdout if present
            if !stdout_msg.is_empty() {
                context
                    .emit(AdapterEvent::stdout(stdout_msg.to_string()))
                    .await?;
            }

            // Sleep (except on last iteration)
            if i < checkpoints.len() - 1 {
                sleep(step_duration).await;
            }
        }

        // Emit completed event
        context
            .emit(AdapterEvent::completed(serde_json::json!({
                "exit_code": 0,
                "duration_ms": config.duration_ms,
            })))
            .await?;

        tracing::info!(task_id = %context.task_id, "Mock adapter completed");
        Ok(())
    }

    fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "mock",
            "version": "1.0.0",
            "description": "Deterministic mock adapter for testing",
            "capabilities": ["deterministic", "configurable_duration", "simulated_failure"]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    #[test]
    fn test_mock_config_defaults() {
        let config = MockConfig::default();
        assert_eq!(config.duration_ms, 5000);
        assert_eq!(config.should_fail, false);
        assert_eq!(config.failure_percent, 50);
    }

    #[test]
    fn test_mock_config_deserialization() {
        let json = serde_json::json!({
            "duration_ms": 1000,
            "should_fail": true,
            "failure_percent": 75
        });

        let config: MockConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.duration_ms, 1000);
        assert_eq!(config.should_fail, true);
        assert_eq!(config.failure_percent, 75);
    }

    #[test]
    fn test_adapter_name() {
        let adapter = MockAdapter::new();
        assert_eq!(adapter.name(), "mock");
    }

    #[test]
    fn test_validate_args_valid() {
        let adapter = MockAdapter::new();
        let args = serde_json::json!({
            "duration_ms": 1000,
            "should_fail": false
        });

        assert!(adapter.validate_args(&args).is_ok());
    }

    #[test]
    fn test_validate_args_zero_duration() {
        let adapter = MockAdapter::new();
        let args = serde_json::json!({
            "duration_ms": 0
        });

        let result = adapter.validate_args(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be > 0"));
    }

    #[test]
    fn test_validate_args_too_long() {
        let adapter = MockAdapter::new();
        let args = serde_json::json!({
            "duration_ms": 4000000  // > 1 hour
        });

        let result = adapter.validate_args(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_args_invalid_failure_percent() {
        let adapter = MockAdapter::new();
        let args = serde_json::json!({
            "duration_ms": 1000,
            "failure_percent": 150
        });

        let result = adapter.validate_args(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("0-100"));
    }

    #[tokio::test]
    async fn test_execute_success() {
        let adapter = MockAdapter::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let cancel_token = CancellationToken::new();

        let args = serde_json::json!({
            "duration_ms": 100,
            "should_fail": false
        });

        let context = AdapterContext::new(Uuid::new_v4(), args, tx, cancel_token);

        // Execute in background
        let handle = tokio::spawn(async move {
            adapter.execute(context).await
        });

        // Collect events
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }

        // Should succeed
        assert!(handle.await.unwrap().is_ok());

        // Check event sequence
        assert_eq!(events[0].kind, crate::adapters::AdapterEventKind::Started);
        assert_eq!(events.last().unwrap().kind, crate::adapters::AdapterEventKind::Completed);

        // Should have progress events
        let progress_count = events.iter().filter(|e| e.kind == crate::adapters::AdapterEventKind::Progress).count();
        assert_eq!(progress_count, 4); // 25%, 50%, 75%, 100%
    }

    #[tokio::test]
    async fn test_execute_failure() {
        let adapter = MockAdapter::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let cancel_token = CancellationToken::new();

        let args = serde_json::json!({
            "duration_ms": 100,
            "should_fail": true,
            "failure_percent": 50
        });

        let context = AdapterContext::new(Uuid::new_v4(), args, tx, cancel_token);

        // Execute in background
        let handle = tokio::spawn(async move {
            adapter.execute(context).await
        });

        // Collect events
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }

        // Should fail
        assert!(handle.await.unwrap().is_err());

        // Check that failure event was emitted
        let has_failed = events.iter().any(|e| e.kind == crate::adapters::AdapterEventKind::Failed);
        assert!(has_failed);
    }

    #[tokio::test]
    async fn test_execute_cancellation() {
        let adapter = MockAdapter::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let cancel_token = CancellationToken::new();

        let args = serde_json::json!({
            "duration_ms": 5000  // Long running
        });

        let context = AdapterContext::new(Uuid::new_v4(), args, tx, cancel_token.clone());

        // Execute in background
        let handle = tokio::spawn(async move {
            adapter.execute(context).await
        });

        // Wait a bit then cancel
        sleep(Duration::from_millis(50)).await;
        cancel_token.cancel();

        // Collect events
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }

        // Should succeed (cancellation is not an error)
        assert!(handle.await.unwrap().is_ok());

        // Check that cancelled event was emitted
        let has_cancelled = events.iter().any(|e| e.kind == crate::adapters::AdapterEventKind::Cancelled);
        assert!(has_cancelled);
    }

    #[test]
    fn test_adapter_metadata() {
        let adapter = MockAdapter::new();
        let metadata = adapter.metadata();

        assert_eq!(metadata["name"], "mock");
        assert_eq!(metadata["version"], "1.0.0");
        assert!(metadata["capabilities"].is_array());
    }
}
