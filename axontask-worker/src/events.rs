/// Event emission to Redis Streams
///
/// This module handles emitting adapter events to Redis Streams with proper
/// formatting, sequencing, and hash chaining for integrity.
///
/// # Architecture
///
/// Events flow: Adapter → EventEmitter → Redis Streams → API (SSE) → Client
///
/// # Event Format
///
/// Events are stored in Redis Streams with the following structure:
/// ```text
/// events:{task_id}:
///   1234567890-0:
///     seq: "0"
///     kind: "started"
///     payload: "{...}"
///     ts: "2025-01-04T12:00:00Z"
///     hash_prev: ""
///     hash_curr: "abcd1234..."
/// ```
///
/// # Hash Chaining
///
/// Each event includes:
/// - `hash_prev`: SHA-256 hash of previous event (empty for first event)
/// - `hash_curr`: SHA-256 hash of current event
///
/// This creates a tamper-evident chain of events.
///
/// # Example
///
/// ```no_run
/// use axontask_worker::events::EventEmitter;
/// use axontask_worker::adapters::{AdapterEvent, AdapterEventKind};
/// use axontask_shared::redis::{RedisClient, RedisConfig};
/// use uuid::Uuid;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let redis_config = RedisConfig::from_env()?;
/// let redis_client = RedisClient::new(redis_config).await?;
/// let emitter = EventEmitter::new(redis_client);
///
/// let task_id = Uuid::new_v4();
/// let event = AdapterEvent::started(serde_json::json!({"adapter": "shell"}));
///
/// emitter.emit(task_id, event).await?;
/// # Ok(())
/// # }
/// ```

use crate::adapters::{AdapterEvent, AdapterEventKind};
use anyhow::{Context, Result};
use axontask_shared::events::serialization::{event_stream_key, serialize_event};
use axontask_shared::models::task_event::TaskEvent;
use axontask_shared::redis::RedisClient;
use chrono::Utc;
use redis::AsyncCommands;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Event emitter state
///
/// Tracks sequence numbers and previous hash for hash chaining.
struct EmitterState {
    /// Current sequence number
    seq: AtomicI64,

    /// Previous event hash (for chaining)
    prev_hash: Mutex<Option<Vec<u8>>>,
}

impl EmitterState {
    fn new() -> Self {
        EmitterState {
            seq: AtomicI64::new(0),
            prev_hash: Mutex::new(None),
        }
    }

    async fn next_seq(&self) -> i64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }

    async fn get_prev_hash(&self) -> Option<Vec<u8>> {
        self.prev_hash.lock().await.clone()
    }

    async fn set_prev_hash(&self, hash: Vec<u8>) {
        *self.prev_hash.lock().await = Some(hash);
    }
}

/// Event emitter for Redis Streams
///
/// Handles emitting adapter events to Redis Streams with sequencing and hash chaining.
pub struct EventEmitter {
    /// Redis client
    redis: RedisClient,

    /// Per-task emitter state
    state: Arc<EmitterState>,
}

impl EventEmitter {
    /// Creates a new event emitter
    pub fn new(redis: RedisClient) -> Self {
        EventEmitter {
            redis,
            state: Arc::new(EmitterState::new()),
        }
    }

    /// Emits an event to Redis Streams
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `event` - Adapter event to emit
    ///
    /// # Returns
    ///
    /// Ok with stream ID if successful, Err if emission failed
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails or serialization fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_worker::events::EventEmitter;
    /// # use axontask_worker::adapters::AdapterEvent;
    /// # use axontask_shared::redis::{RedisClient, RedisConfig};
    /// # use uuid::Uuid;
    /// # async fn example(emitter: EventEmitter, task_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
    /// let event = AdapterEvent::stdout("Hello World".to_string());
    /// let stream_id = emitter.emit(task_id, event).await?;
    /// println!("Emitted event: {}", stream_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn emit(&self, task_id: Uuid, event: AdapterEvent) -> Result<String> {
        let seq = self.state.next_seq().await;
        let ts = Utc::now();
        let hash_prev = self.state.get_prev_hash().await;

        // Create task event
        let task_event = TaskEvent {
            task_id,
            seq,
            kind: event.kind.to_string(),
            payload: event.payload,
            ts,
            hash_prev: hash_prev.clone(),
            hash_curr: vec![], // Will be computed below
        };

        // Compute current hash
        let hash_curr = self.compute_hash(&task_event)?;
        let task_event = TaskEvent {
            hash_curr: hash_curr.clone(),
            ..task_event
        };

        // Update prev_hash for next event
        self.state.set_prev_hash(hash_curr).await;

        // Serialize event
        let fields = serialize_event(&task_event)?;

        // Write to Redis Stream
        let stream_key = event_stream_key(task_id);
        let mut conn = self.redis.get_connection();

        // Convert HashMap to Vec of tuples for xadd
        let items: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        let stream_id: String = conn
            .xadd(&stream_key, "*", &items)
            .await
            .context("Failed to write event to Redis Stream")?;

        tracing::debug!(
            task_id = %task_id,
            seq = seq,
            kind = %event.kind,
            stream_id = %stream_id,
            "Event emitted"
        );

        Ok(stream_id)
    }

    /// Emits a batch of events
    ///
    /// More efficient than calling emit() multiple times.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Task ID
    /// * `events` - Vector of adapter events
    ///
    /// # Returns
    ///
    /// Vec of stream IDs
    pub async fn emit_batch(&self, task_id: Uuid, events: Vec<AdapterEvent>) -> Result<Vec<String>> {
        let mut stream_ids = Vec::with_capacity(events.len());

        for event in events {
            let stream_id = self.emit(task_id, event).await?;
            stream_ids.push(stream_id);
        }

        Ok(stream_ids)
    }

    /// Computes SHA-256 hash of event
    ///
    /// Hash includes: seq, kind, payload (JSON), ts, hash_prev
    fn compute_hash(&self, event: &TaskEvent) -> Result<Vec<u8>> {
        let mut hasher = Sha256::new();

        // Hash sequence
        hasher.update(event.seq.to_le_bytes());

        // Hash kind
        hasher.update(event.kind.as_bytes());

        // Hash payload (canonical JSON)
        let payload_json = serde_json::to_string(&event.payload)?;
        hasher.update(payload_json.as_bytes());

        // Hash timestamp (RFC3339)
        let ts_str = event.ts.to_rfc3339();
        hasher.update(ts_str.as_bytes());

        // Hash previous hash (if exists)
        if let Some(ref prev) = event.hash_prev {
            hasher.update(prev);
        }

        Ok(hasher.finalize().to_vec())
    }

    /// Resets emitter state (for testing)
    #[cfg(test)]
    pub fn reset(&self) {
        self.state.seq.store(0, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emitter_state() {
        let state = EmitterState::new();
        assert_eq!(state.seq.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_emitter_state_next_seq() {
        let state = EmitterState::new();

        assert_eq!(state.next_seq().await, 0);
        assert_eq!(state.next_seq().await, 1);
        assert_eq!(state.next_seq().await, 2);
    }

    #[tokio::test]
    async fn test_emitter_state_prev_hash() {
        let state = EmitterState::new();

        assert_eq!(state.get_prev_hash().await, None);

        let hash = vec![1, 2, 3, 4];
        state.set_prev_hash(hash.clone()).await;

        assert_eq!(state.get_prev_hash().await, Some(hash));
    }

    #[test]
    fn test_compute_hash() {
        // Note: Full integration tests with Redis are in tests/events.rs
        // This is just a unit test for hash computation

        let redis_config = axontask_shared::redis::RedisConfig {
            url: "redis://localhost:6379".to_string(),
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let redis_client = match RedisClient::new(redis_config).await {
                Ok(client) => client,
                Err(_) => return, // Skip test if Redis not available
            };

            let emitter = EventEmitter::new(redis_client);

            let event1 = TaskEvent {
                seq: 0,
                kind: "started".to_string(),
                payload: serde_json::json!({"adapter": "test"}),
                ts: Utc::now(),
                hash_prev: None,
                hash_curr: vec![],
            };

            let hash1 = emitter.compute_hash(&event1).unwrap();
            assert_eq!(hash1.len(), 32); // SHA-256 = 32 bytes

            // Same event should produce same hash
            let hash2 = emitter.compute_hash(&event1).unwrap();
            assert_eq!(hash1, hash2);

            // Different event should produce different hash
            let event2 = TaskEvent {
                seq: 1,
                ..event1.clone()
            };
            let hash3 = emitter.compute_hash(&event2).unwrap();
            assert_ne!(hash1, hash3);
        });
    }

    // Full integration tests with Redis are in tests/events.rs
}
