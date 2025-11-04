/// Task Event model and database operations
///
/// This module provides the TaskEvent model for append-only event logging with hash chaining.
/// Events form a tamper-evident audit trail for task execution.
///
/// # Hash Chain
///
/// Each event includes:
/// - `hash_prev`: SHA-256 hash of previous event (NULL for seq=0)
/// - `hash_curr`: SHA-256 hash of (hash_prev || seq || kind || payload)
///
/// This creates a cryptographic chain where any tampering breaks the chain.
///
/// # Schema
///
/// ```sql
/// CREATE TABLE task_events (
///     task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
///     seq BIGINT NOT NULL,
///     ts TIMESTAMPTZ NOT NULL DEFAULT NOW(),
///     kind VARCHAR(50) NOT NULL,
///     payload JSONB NOT NULL DEFAULT '{}',
///     hash_prev BYTEA,
///     hash_curr BYTEA NOT NULL,
///     PRIMARY KEY (task_id, seq)
/// );
/// ```
///
/// # Example
///
/// ```no_run
/// use axontask_shared::models::task_event::{TaskEvent, AppendEvent, EventKind};
/// use axontask_shared::db::pool::{create_pool, DatabaseConfig};
/// use serde_json::json;
/// use uuid::Uuid;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let pool = create_pool(DatabaseConfig::default()).await?;
/// let task_id = Uuid::new_v4();
///
/// // Append an event (hash chain computed automatically)
/// let event = TaskEvent::append(&pool, AppendEvent {
///     task_id,
///     kind: EventKind::Started,
///     payload: json!({"adapter": "shell", "command": "ls"}),
/// }).await?;
///
/// // Verify hash chain integrity
/// let is_valid = TaskEvent::verify_chain(&pool, task_id).await?;
/// # Ok(())
/// # }
/// ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Event types for task execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    /// Task execution started
    Started,

    /// Progress update
    Progress,

    /// Standard output data
    Stdout,

    /// Standard error data
    Stderr,

    /// Task completed successfully
    Success,

    /// Task failed with error
    Error,

    /// Task was canceled
    Canceled,

    /// Task exceeded timeout
    Timeout,

    /// Digest/checkpoint event (for compaction)
    Digest,
}

impl EventKind {
    /// Converts kind to string for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::Started => "started",
            EventKind::Progress => "progress",
            EventKind::Stdout => "stdout",
            EventKind::Stderr => "stderr",
            EventKind::Success => "success",
            EventKind::Error => "error",
            EventKind::Canceled => "canceled",
            EventKind::Timeout => "timeout",
            EventKind::Digest => "digest",
        }
    }

    /// Parses kind from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "started" => Some(EventKind::Started),
            "progress" => Some(EventKind::Progress),
            "stdout" => Some(EventKind::Stdout),
            "stderr" => Some(EventKind::Stderr),
            "success" => Some(EventKind::Success),
            "error" => Some(EventKind::Error),
            "canceled" => Some(EventKind::Canceled),
            "timeout" => Some(EventKind::Timeout),
            "digest" => Some(EventKind::Digest),
            _ => None,
        }
    }
}

/// Task Event model representing an event in the task execution log
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskEvent {
    /// Task this event belongs to
    pub task_id: Uuid,

    /// Sequence number (monotonic within task, starting at 0)
    pub seq: i64,

    /// Event timestamp
    pub ts: DateTime<Utc>,

    /// Event type
    pub kind: String,

    /// Event data (JSON, adapter-specific)
    pub payload: JsonValue,

    /// SHA-256 hash of previous event (NULL for seq=0)
    pub hash_prev: Option<Vec<u8>>,

    /// SHA-256 hash of this event
    pub hash_curr: Vec<u8>,
}

/// Input for appending a new event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEvent {
    /// Task ID
    pub task_id: Uuid,

    /// Event kind
    pub kind: EventKind,

    /// Event payload
    pub payload: JsonValue,
}

impl TaskEvent {
    /// Computes hash for an event
    ///
    /// Hash = SHA-256(hash_prev || seq || kind || payload_bytes)
    ///
    /// # Example
    ///
    /// ```
    /// use axontask_shared::models::task_event::TaskEvent;
    /// use serde_json::json;
    ///
    /// let hash = TaskEvent::compute_hash(
    ///     None,  // No previous hash (first event)
    ///     0,     // Sequence 0
    ///     "started",
    ///     &json!({"adapter": "shell"})
    /// );
    /// assert_eq!(hash.len(), 32); // SHA-256 produces 32 bytes
    /// ```
    pub fn compute_hash(
        hash_prev: Option<&[u8]>,
        seq: i64,
        kind: &str,
        payload: &JsonValue,
    ) -> Vec<u8> {
        let mut hasher = Sha256::new();

        // Include previous hash if present
        if let Some(prev) = hash_prev {
            hasher.update(prev);
        }

        // Include sequence number
        hasher.update(seq.to_le_bytes());

        // Include event kind
        hasher.update(kind.as_bytes());

        // Include payload
        let payload_bytes = serde_json::to_vec(payload).unwrap_or_default();
        hasher.update(&payload_bytes);

        hasher.finalize().to_vec()
    }

    /// Appends a new event to the task event log
    ///
    /// Automatically:
    /// - Determines next sequence number
    /// - Fetches previous event hash
    /// - Computes hash chain
    /// - Inserts event atomically
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `data` - Event data
    ///
    /// # Returns
    ///
    /// The newly created event
    ///
    /// # Errors
    ///
    /// Returns an error if database operation fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::task_event::{TaskEvent, AppendEvent, EventKind};
    /// # use sqlx::PgPool;
    /// # use serde_json::json;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, task_id: Uuid) -> Result<(), sqlx::Error> {
    /// let event = TaskEvent::append(&pool, AppendEvent {
    ///     task_id,
    ///     kind: EventKind::Stdout,
    ///     payload: json!({"data": "Hello, world!\n"}),
    /// }).await?;
    ///
    /// println!("Appended event seq={}", event.seq);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn append(pool: &PgPool, data: AppendEvent) -> Result<Self, sqlx::Error> {
        // Get the last event to determine next seq and previous hash
        let last_event: Option<(i64, Option<Vec<u8>>)> = sqlx::query_as(
            "SELECT seq, hash_curr FROM task_events WHERE task_id = $1 ORDER BY seq DESC LIMIT 1"
        )
        .bind(data.task_id)
        .fetch_optional(pool)
        .await?;

        let (next_seq, hash_prev) = match last_event {
            Some((last_seq, Some(last_hash))) => (last_seq + 1, Some(last_hash)),
            Some((last_seq, None)) => (last_seq + 1, None),
            None => (0, None), // First event
        };

        // Compute hash for this event
        let hash_curr = Self::compute_hash(
            hash_prev.as_deref(),
            next_seq,
            data.kind.as_str(),
            &data.payload,
        );

        // Insert event
        let event = sqlx::query_as::<_, TaskEvent>(
            r#"
            INSERT INTO task_events (task_id, seq, kind, payload, hash_prev, hash_curr)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING task_id, seq, ts, kind, payload, hash_prev, hash_curr
            "#,
        )
        .bind(data.task_id)
        .bind(next_seq)
        .bind(data.kind.as_str())
        .bind(data.payload)
        .bind(hash_prev)
        .bind(hash_curr)
        .fetch_one(pool)
        .await?;

        Ok(event)
    }

    /// Queries events by range
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `task_id` - Task ID
    /// * `start_seq` - Start sequence (inclusive)
    /// * `end_seq` - Optional end sequence (inclusive)
    ///
    /// # Returns
    ///
    /// Vector of events in sequence order
    pub async fn query_range(
        pool: &PgPool,
        task_id: Uuid,
        start_seq: i64,
        end_seq: Option<i64>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let events = if let Some(end) = end_seq {
            sqlx::query_as::<_, TaskEvent>(
                r#"
                SELECT task_id, seq, ts, kind, payload, hash_prev, hash_curr
                FROM task_events
                WHERE task_id = $1 AND seq >= $2 AND seq <= $3
                ORDER BY seq ASC
                "#,
            )
            .bind(task_id)
            .bind(start_seq)
            .bind(end)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as::<_, TaskEvent>(
                r#"
                SELECT task_id, seq, ts, kind, payload, hash_prev, hash_curr
                FROM task_events
                WHERE task_id = $1 AND seq >= $2
                ORDER BY seq ASC
                "#,
            )
            .bind(task_id)
            .bind(start_seq)
            .fetch_all(pool)
            .await?
        };

        Ok(events)
    }

    /// Gets the latest event for a task
    pub async fn get_latest(pool: &PgPool, task_id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        let event = sqlx::query_as::<_, TaskEvent>(
            r#"
            SELECT task_id, seq, ts, kind, payload, hash_prev, hash_curr
            FROM task_events
            WHERE task_id = $1
            ORDER BY seq DESC
            LIMIT 1
            "#,
        )
        .bind(task_id)
        .fetch_optional(pool)
        .await?;

        Ok(event)
    }

    /// Counts events for a task
    pub async fn count(pool: &PgPool, task_id: Uuid) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM task_events WHERE task_id = $1"
        )
        .bind(task_id)
        .fetch_one(pool)
        .await?;

        Ok(count)
    }

    /// Verifies hash chain integrity for a task
    ///
    /// Recomputes all hashes and verifies they match stored hashes.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `task_id` - Task ID to verify
    ///
    /// # Returns
    ///
    /// True if chain is valid, false if any hash mismatch detected
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use axontask_shared::models::task_event::TaskEvent;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool, task_id: Uuid) -> Result<(), sqlx::Error> {
    /// let is_valid = TaskEvent::verify_chain(&pool, task_id).await?;
    /// if !is_valid {
    ///     eprintln!("WARNING: Hash chain integrity violated!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn verify_chain(pool: &PgPool, task_id: Uuid) -> Result<bool, sqlx::Error> {
        let events = Self::query_range(pool, task_id, 0, None).await?;

        if events.is_empty() {
            return Ok(true); // Empty chain is valid
        }

        let mut prev_hash: Option<Vec<u8>> = None;

        for event in events {
            // Recompute hash
            let computed_hash = Self::compute_hash(
                prev_hash.as_deref(),
                event.seq,
                &event.kind,
                &event.payload,
            );

            // Verify it matches stored hash
            if computed_hash != event.hash_curr {
                return Ok(false); // Chain broken!
            }

            // Verify previous hash matches
            if event.hash_prev != prev_hash {
                return Ok(false); // Chain broken!
            }

            prev_hash = Some(event.hash_curr.clone());
        }

        Ok(true)
    }

    /// Deletes all events for a task
    ///
    /// ⚠️  This breaks the immutability guarantee. Only use for testing or GDPR compliance.
    pub async fn delete_for_task(pool: &PgPool, task_id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM task_events WHERE task_id = $1")
            .bind(task_id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_event_kind_as_str() {
        assert_eq!(EventKind::Started.as_str(), "started");
        assert_eq!(EventKind::Progress.as_str(), "progress");
        assert_eq!(EventKind::Stdout.as_str(), "stdout");
        assert_eq!(EventKind::Stderr.as_str(), "stderr");
        assert_eq!(EventKind::Success.as_str(), "success");
        assert_eq!(EventKind::Error.as_str(), "error");
        assert_eq!(EventKind::Canceled.as_str(), "canceled");
        assert_eq!(EventKind::Timeout.as_str(), "timeout");
        assert_eq!(EventKind::Digest.as_str(), "digest");
    }

    #[test]
    fn test_event_kind_from_str() {
        assert_eq!(EventKind::from_str("started"), Some(EventKind::Started));
        assert_eq!(EventKind::from_str("progress"), Some(EventKind::Progress));
        assert_eq!(EventKind::from_str("invalid"), None);
    }

    #[test]
    fn test_compute_hash() {
        let payload = json!({"test": "data"});

        // Hash for first event (no previous)
        let hash1 = TaskEvent::compute_hash(None, 0, "started", &payload);
        assert_eq!(hash1.len(), 32); // SHA-256 is 32 bytes

        // Hash for second event (with previous)
        let hash2 = TaskEvent::compute_hash(Some(&hash1), 1, "progress", &payload);
        assert_eq!(hash2.len(), 32);

        // Different data = different hash
        assert_ne!(hash1, hash2);

        // Same data = same hash (deterministic)
        let hash1_again = TaskEvent::compute_hash(None, 0, "started", &payload);
        assert_eq!(hash1, hash1_again);
    }

    #[test]
    fn test_compute_hash_includes_all_fields() {
        let payload = json!({"test": "data"});

        let hash1 = TaskEvent::compute_hash(None, 0, "started", &payload);

        // Different seq
        let hash2 = TaskEvent::compute_hash(None, 1, "started", &payload);
        assert_ne!(hash1, hash2);

        // Different kind
        let hash3 = TaskEvent::compute_hash(None, 0, "progress", &payload);
        assert_ne!(hash1, hash3);

        // Different payload
        let hash4 = TaskEvent::compute_hash(None, 0, "started", &json!({"different": "data"}));
        assert_ne!(hash1, hash4);
    }
}
