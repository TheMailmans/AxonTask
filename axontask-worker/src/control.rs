/// Control stream listener
///
/// This module implements listening for control messages from the API server.
/// Control messages are used to cancel running tasks or send other control signals.
///
/// # Control Message Format
///
/// Control messages are sent via Redis Pub/Sub on channel: `ctrl:{task_id}`
///
/// Message format (JSON):
/// ```json
/// {
///   "command": "cancel",
///   "reason": "User requested cancellation"
/// }
/// ```
///
/// # Commands
///
/// - **cancel**: Cancel the running task
///
/// # Example
///
/// ```no_run
/// use axontask_worker::control::ControlListener;
/// use axontask_shared::redis::{RedisClient, RedisConfig};
/// use tokio_util::sync::CancellationToken;
/// use uuid::Uuid;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let redis_config = RedisConfig::from_env()?;
/// let redis_client = RedisClient::new(redis_config).await?;
///
/// let listener = ControlListener::new(redis_client);
/// let task_id = Uuid::new_v4();
/// let cancel_token = CancellationToken::new();
///
/// // Start listening (spawns background task)
/// let handle = listener.listen(task_id, cancel_token.clone()).await?;
///
/// // When task completes, stop listening
/// handle.abort();
/// # Ok(())
/// # }
/// ```

use axontask_shared::redis::RedisClient;
use redis::AsyncCommands;
use tokio_stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Control command types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlCommand {
    /// Cancel task execution
    Cancel,
}

/// Control message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlMessage {
    /// Command to execute
    pub command: ControlCommand,

    /// Optional reason/metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl ControlMessage {
    /// Creates a cancel message
    pub fn cancel(reason: Option<String>) -> Self {
        ControlMessage {
            command: ControlCommand::Cancel,
            reason,
        }
    }
}

/// Control listener error
#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    /// Redis connection error
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    /// Message parsing error
    #[error("Failed to parse control message: {0}")]
    ParseError(#[from] serde_json::Error),
}

/// Control stream listener
///
/// Listens for control messages from the API via Redis Pub/Sub.
pub struct ControlListener {
    /// Redis client
    redis: RedisClient,
}

impl ControlListener {
    /// Creates a new control listener
    ///
    /// # Arguments
    ///
    /// * `redis` - Redis client
    pub fn new(redis: RedisClient) -> Self {
        ControlListener { redis }
    }

    /// Starts listening for control messages
    ///
    /// Spawns a background task that subscribes to the control channel
    /// for the given task and triggers the cancel token when a cancel
    /// command is received.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID to listen for
    /// * `cancel_token` - Token to cancel when control message received
    ///
    /// # Returns
    ///
    /// Join handle for the listener task (abort to stop listening)
    ///
    /// # Errors
    ///
    /// Returns error if Redis connection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_worker::control::ControlListener;
    /// # use axontask_shared::redis::RedisClient;
    /// # use tokio_util::sync::CancellationToken;
    /// # use uuid::Uuid;
    /// # async fn example(listener: ControlListener, task_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
    /// let cancel_token = CancellationToken::new();
    ///
    /// let handle = listener.listen(task_id, cancel_token.clone()).await?;
    ///
    /// // Task runs...
    ///
    /// // Stop listening when task completes
    /// handle.abort();
    /// # Ok(())
    /// # }
    /// ```
    pub async fn listen(
        &self,
        task_id: Uuid,
        cancel_token: CancellationToken,
    ) -> Result<JoinHandle<()>, ControlError> {
        let redis = self.redis.clone();
        let channel = control_channel(task_id);

        let handle = tokio::spawn(async move {
            if let Err(e) = listen_loop(redis, channel, task_id, cancel_token).await {
                tracing::error!(
                    task_id = %task_id,
                    error = %e,
                    "Control listener error"
                );
            }
        });

        Ok(handle)
    }

    /// Sends a control message to a task
    ///
    /// Used by the API to send control commands to running workers.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Target task ID
    /// * `message` - Control message to send
    ///
    /// # Errors
    ///
    /// Returns error if Redis publish fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_worker::control::{ControlListener, ControlMessage};
    /// # use axontask_shared::redis::RedisClient;
    /// # use uuid::Uuid;
    /// # async fn example(listener: ControlListener, task_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
    /// let message = ControlMessage::cancel(Some("User requested".to_string()));
    /// listener.send(task_id, message).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send(&self, task_id: Uuid, message: ControlMessage) -> Result<(), ControlError> {
        let channel = control_channel(task_id);
        let payload = serde_json::to_string(&message)?;

        let mut conn = self.redis.get_connection();
        let _: i32 = conn.publish(&channel, payload).await?;

        tracing::debug!(
            task_id = %task_id,
            command = ?message.command,
            "Sent control message"
        );

        Ok(())
    }
}

/// Control channel name for a task
fn control_channel(task_id: Uuid) -> String {
    format!("ctrl:{}", task_id)
}

/// Background listen loop
async fn listen_loop(
    redis: RedisClient,
    channel: String,
    task_id: Uuid,
    cancel_token: CancellationToken,
) -> Result<(), ControlError> {
    tracing::debug!(
        task_id = %task_id,
        channel = %channel,
        "Starting control listener"
    );

    // Get pubsub connection
    let client = redis::Client::open(redis.url())?;
    let conn = client.get_async_connection().await?;
    let mut pubsub = conn.into_pubsub();

    // Subscribe to control channel
    pubsub.subscribe(&channel).await?;

    tracing::info!(
        task_id = %task_id,
        channel = %channel,
        "Listening for control messages"
    );

    // Listen for messages
    let mut stream = pubsub.on_message();

    loop {
        // Check if task is already cancelled
        if cancel_token.is_cancelled() {
            tracing::debug!(task_id = %task_id, "Task already cancelled, stopping listener");
            break;
        }

        // Wait for message (with timeout to check cancellation periodically)
        tokio::select! {
            msg = stream.next() => {
                match msg {
                    Some(msg) => {
                        let payload: String = match msg.get_payload() {
                            Ok(p) => p,
                            Err(e) => {
                                tracing::error!(error = %e, "Failed to get message payload");
                                continue;
                            }
                        };

                        // Parse control message
                        let control_msg: ControlMessage = match serde_json::from_str(&payload) {
                            Ok(m) => m,
                            Err(e) => {
                                tracing::error!(error = %e, payload = %payload, "Failed to parse control message");
                                continue;
                            }
                        };

                        // Handle command
                        match control_msg.command {
                            ControlCommand::Cancel => {
                                tracing::info!(
                                    task_id = %task_id,
                                    reason = ?control_msg.reason,
                                    "Received cancel command"
                                );
                                cancel_token.cancel();
                                break;
                            }
                        }
                    }
                    None => {
                        tracing::debug!(task_id = %task_id, "Control stream ended");
                        break;
                    }
                }
            }
            _ = sleep(Duration::from_secs(1)) => {
                // Periodic check to see if we should stop
                continue;
            }
        }
    }

    tracing::debug!(task_id = %task_id, "Control listener stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_message_cancel() {
        let msg = ControlMessage::cancel(Some("test".to_string()));
        assert_eq!(msg.command, ControlCommand::Cancel);
        assert_eq!(msg.reason, Some("test".to_string()));
    }

    #[test]
    fn test_control_message_serialization() {
        let msg = ControlMessage::cancel(Some("User requested".to_string()));
        let json = serde_json::to_string(&msg).unwrap();

        assert!(json.contains("\"command\":\"cancel\""));
        assert!(json.contains("\"reason\":\"User requested\""));

        let deserialized: ControlMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.command, ControlCommand::Cancel);
        assert_eq!(deserialized.reason, Some("User requested".to_string()));
    }

    #[test]
    fn test_control_channel() {
        let task_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let channel = control_channel(task_id);
        assert_eq!(channel, "ctrl:550e8400-e29b-41d4-a716-446655440000");
    }

    // Integration tests with actual Redis are in tests/control_tests.rs
}
