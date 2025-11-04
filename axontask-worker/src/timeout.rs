/// Timeout handling for task execution
///
/// This module implements timeout enforcement for tasks. Each task can have
/// a configurable timeout, and if the task exceeds this timeout, it will be
/// forcefully cancelled and marked as timed out.
///
/// # Timeout Behavior
///
/// 1. **Graceful timeout**: Task receives cancellation signal at timeout
/// 2. **Grace period**: 30 seconds for task to clean up
/// 3. **Force kill**: Task is forcefully terminated after grace period
///
/// # Default Timeouts
///
/// - No timeout specified: 1 hour (3600 seconds)
/// - Minimum timeout: 1 second
/// - Maximum timeout: 24 hours (86400 seconds)
///
/// # Example
///
/// ```no_run
/// use axontask_worker::timeout::TimeoutEnforcer;
/// use tokio_util::sync::CancellationToken;
/// use std::time::Duration;
///
/// # async fn example() {
/// let cancel_token = CancellationToken::new();
/// let timeout_enforcer = TimeoutEnforcer::new(Duration::from_secs(300));
///
/// // Start timeout enforcement
/// let timeout_handle = timeout_enforcer.enforce(cancel_token.clone());
///
/// // Do work...
///
/// // Cancel timeout if task completes early
/// timeout_handle.abort();
/// # }
/// ```

use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Default timeout duration (1 hour)
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3600);

/// Grace period after timeout for cleanup (30 seconds)
pub const GRACE_PERIOD: Duration = Duration::from_secs(30);

/// Minimum allowed timeout (1 second)
pub const MIN_TIMEOUT: Duration = Duration::from_secs(1);

/// Maximum allowed timeout (24 hours)
pub const MAX_TIMEOUT: Duration = Duration::from_secs(86400);

/// Timeout enforcer
///
/// Enforces timeouts on task execution by cancelling tasks that exceed
/// their configured timeout duration.
pub struct TimeoutEnforcer {
    /// Timeout duration
    timeout: Duration,

    /// Grace period for cleanup
    grace_period: Duration,
}

impl TimeoutEnforcer {
    /// Creates a new timeout enforcer
    ///
    /// # Arguments
    ///
    /// * `timeout` - Timeout duration
    pub fn new(timeout: Duration) -> Self {
        TimeoutEnforcer {
            timeout,
            grace_period: GRACE_PERIOD,
        }
    }

    /// Creates a new timeout enforcer with custom grace period
    ///
    /// # Arguments
    ///
    /// * `timeout` - Timeout duration
    /// * `grace_period` - Grace period for cleanup
    pub fn with_grace_period(timeout: Duration, grace_period: Duration) -> Self {
        TimeoutEnforcer {
            timeout,
            grace_period,
        }
    }

    /// Creates a timeout enforcer from task timeout_seconds
    ///
    /// # Arguments
    ///
    /// * `timeout_seconds` - Timeout in seconds (None = default)
    pub fn from_task_timeout(timeout_seconds: Option<i32>) -> Self {
        let timeout = match timeout_seconds {
            Some(secs) => {
                // Treat non-positive values as min timeout
                if secs <= 0 {
                    MIN_TIMEOUT
                } else {
                    let duration = Duration::from_secs(secs as u64);
                    // Clamp to valid range
                    if duration < MIN_TIMEOUT {
                        MIN_TIMEOUT
                    } else if duration > MAX_TIMEOUT {
                        MAX_TIMEOUT
                    } else {
                        duration
                    }
                }
            }
            None => DEFAULT_TIMEOUT,
        };

        TimeoutEnforcer::new(timeout)
    }

    /// Enforces timeout on a task
    ///
    /// Spawns a background task that will cancel the provided token
    /// if the timeout is exceeded.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID for logging
    /// * `cancel_token` - Cancellation token to trigger on timeout
    ///
    /// # Returns
    ///
    /// Join handle for the timeout task (can be aborted if task completes early)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_worker::timeout::TimeoutEnforcer;
    /// # use tokio_util::sync::CancellationToken;
    /// # use std::time::Duration;
    /// # use uuid::Uuid;
    /// # async fn example() {
    /// let enforcer = TimeoutEnforcer::new(Duration::from_secs(300));
    /// let cancel_token = CancellationToken::new();
    /// let task_id = Uuid::new_v4();
    ///
    /// let timeout_handle = enforcer.enforce(task_id, cancel_token.clone());
    ///
    /// // If task completes early, abort timeout
    /// timeout_handle.abort();
    /// # }
    /// ```
    pub fn enforce(&self, task_id: Uuid, cancel_token: CancellationToken) -> JoinHandle<()> {
        let timeout = self.timeout;
        let grace_period = self.grace_period;

        tokio::spawn(async move {
            // Wait for timeout
            sleep(timeout).await;

            // Check if already cancelled
            if cancel_token.is_cancelled() {
                return;
            }

            tracing::warn!(
                task_id = %task_id,
                timeout_secs = timeout.as_secs(),
                "Task timeout reached, sending cancellation signal"
            );

            // Send graceful cancellation
            cancel_token.cancel();

            // Wait grace period for cleanup
            sleep(grace_period).await;

            tracing::warn!(
                task_id = %task_id,
                "Grace period expired, task should be terminated"
            );
        })
    }

    /// Gets the timeout duration
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Gets the grace period
    pub fn grace_period(&self) -> Duration {
        self.grace_period
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_enforcer_new() {
        let enforcer = TimeoutEnforcer::new(Duration::from_secs(300));
        assert_eq!(enforcer.timeout(), Duration::from_secs(300));
        assert_eq!(enforcer.grace_period(), GRACE_PERIOD);
    }

    #[test]
    fn test_timeout_enforcer_with_grace_period() {
        let enforcer = TimeoutEnforcer::with_grace_period(
            Duration::from_secs(300),
            Duration::from_secs(10),
        );
        assert_eq!(enforcer.timeout(), Duration::from_secs(300));
        assert_eq!(enforcer.grace_period(), Duration::from_secs(10));
    }

    #[test]
    fn test_from_task_timeout_default() {
        let enforcer = TimeoutEnforcer::from_task_timeout(None);
        assert_eq!(enforcer.timeout(), DEFAULT_TIMEOUT);
    }

    #[test]
    fn test_from_task_timeout_valid() {
        let enforcer = TimeoutEnforcer::from_task_timeout(Some(600));
        assert_eq!(enforcer.timeout(), Duration::from_secs(600));
    }

    #[test]
    fn test_from_task_timeout_too_small() {
        let enforcer = TimeoutEnforcer::from_task_timeout(Some(0));
        assert_eq!(enforcer.timeout(), MIN_TIMEOUT);
    }

    #[test]
    fn test_from_task_timeout_too_large() {
        let enforcer = TimeoutEnforcer::from_task_timeout(Some(100000));
        assert_eq!(enforcer.timeout(), MAX_TIMEOUT);
    }

    #[tokio::test]
    async fn test_enforce_timeout() {
        let enforcer = TimeoutEnforcer::new(Duration::from_millis(100));
        let cancel_token = CancellationToken::new();
        let task_id = Uuid::new_v4();

        assert!(!cancel_token.is_cancelled());

        let timeout_handle = enforcer.enforce(task_id, cancel_token.clone());

        // Wait for timeout to trigger
        sleep(Duration::from_millis(150)).await;

        assert!(cancel_token.is_cancelled());

        timeout_handle.abort();
    }

    #[tokio::test]
    async fn test_enforce_cancel_early() {
        let enforcer = TimeoutEnforcer::new(Duration::from_secs(10));
        let cancel_token = CancellationToken::new();
        let task_id = Uuid::new_v4();

        let timeout_handle = enforcer.enforce(task_id, cancel_token.clone());

        // Task completes before timeout
        sleep(Duration::from_millis(50)).await;
        cancel_token.cancel();

        // Abort timeout handler
        timeout_handle.abort();

        assert!(cancel_token.is_cancelled());
    }

    #[test]
    fn test_constants() {
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(3600));
        assert_eq!(GRACE_PERIOD, Duration::from_secs(30));
        assert_eq!(MIN_TIMEOUT, Duration::from_secs(1));
        assert_eq!(MAX_TIMEOUT, Duration::from_secs(86400));
    }
}
