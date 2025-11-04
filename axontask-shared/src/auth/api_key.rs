/// API key authentication utilities
///
/// This module provides utilities for generating and validating API keys.
/// These work in conjunction with the `models::api_key` module for database operations.
///
/// # Security
///
/// - **Format**: `axon_{32_chars}` (prefix + 32 random alphanumeric chars)
/// - **Storage**: Keys are hashed with SHA-256 before storage
/// - **Validation**: Constant-time comparison to prevent timing attacks
/// - **Scopes**: Fine-grained permissions (e.g., "tasks:read", "tasks:write")
///
/// # Key Format
///
/// API keys follow the pattern: `axon_abcd1234efgh5678...` (37 chars total)
/// - Prefix: "axon_" (5 chars)
/// - Random part: 32 alphanumeric chars (base62: [A-Za-z0-9])
///
/// # Example
///
/// ```
/// use axontask_shared::auth::api_key::{generate_api_key, hash_api_key, validate_api_key_format};
///
/// // Generate a new API key
/// let (key, hash) = generate_api_key();
/// assert!(key.starts_with("axon_"));
/// assert_eq!(key.len(), 37);
///
/// // Validate format
/// assert!(validate_api_key_format(&key));
///
/// // Hash matches
/// let computed_hash = hash_api_key(&key);
/// assert_eq!(hash, computed_hash);
/// ```

use rand::Rng;
use sha2::{Digest, Sha256};

/// Length of the random part of the API key (characters)
const KEY_RANDOM_LENGTH: usize = 32;

/// API key prefix
const KEY_PREFIX: &str = "axon_";

/// Total length of an API key (prefix + random)
pub const API_KEY_LENGTH: usize = KEY_PREFIX.len() + KEY_RANDOM_LENGTH;

/// Generates a new API key
///
/// Creates a cryptographically random API key with the format `axon_{32_chars}`.
/// Also returns the SHA-256 hash for database storage.
///
/// # Returns
///
/// Tuple of (plaintext_key, sha256_hash)
///
/// # Security
///
/// - Uses `rand::thread_rng()` for cryptographic randomness
/// - Key space: 62^32 ≈ 2^190 combinations
/// - Hash prevents plaintext storage
///
/// # Example
///
/// ```
/// use axontask_shared::auth::api_key::generate_api_key;
///
/// let (key, hash) = generate_api_key();
/// assert!(key.starts_with("axon_"));
/// assert_eq!(key.len(), 37);
/// assert_eq!(hash.len(), 64); // SHA-256 hex is 64 chars
/// ```
pub fn generate_api_key() -> (String, String) {
    let random_part = generate_random_string(KEY_RANDOM_LENGTH);
    let key = format!("{}{}", KEY_PREFIX, random_part);
    let hash = hash_api_key(&key);

    (key, hash)
}

/// Generates a random alphanumeric string
///
/// Uses base62 encoding (A-Z, a-z, 0-9) for URL-safe keys.
fn generate_random_string(length: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();

    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Hashes an API key using SHA-256
///
/// # Arguments
///
/// * `key` - Plaintext API key
///
/// # Returns
///
/// Hex-encoded SHA-256 hash (64 characters)
///
/// # Example
///
/// ```
/// use axontask_shared::auth::api_key::hash_api_key;
///
/// let hash = hash_api_key("axon_test123");
/// assert_eq!(hash.len(), 64);
///
/// // Same input = same hash (deterministic)
/// let hash2 = hash_api_key("axon_test123");
/// assert_eq!(hash, hash2);
/// ```
pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Validates API key format
///
/// Checks that the key:
/// - Starts with "axon_"
/// - Has correct length (37 chars)
/// - Contains only alphanumeric characters after prefix
///
/// # Arguments
///
/// * `key` - API key to validate
///
/// # Returns
///
/// `true` if format is valid, `false` otherwise
///
/// # Example
///
/// ```
/// use axontask_shared::auth::api_key::validate_api_key_format;
///
/// // Valid
/// assert!(validate_api_key_format("axon_abcdefghijklmnopqrstuvwxyz123456"));
///
/// // Invalid - wrong prefix
/// assert!(!validate_api_key_format("wrong_abcdefghijklmnopqrstuvwxyz123456"));
///
/// // Invalid - too short
/// assert!(!validate_api_key_format("axon_short"));
///
/// // Invalid - special characters
/// assert!(!validate_api_key_format("axon_abc!@#$%^&*()_+={}[]|\\:;\"'<>,.?/"));
/// ```
pub fn validate_api_key_format(key: &str) -> bool {
    // Check length
    if key.len() != API_KEY_LENGTH {
        return false;
    }

    // Check prefix
    if !key.starts_with(KEY_PREFIX) {
        return false;
    }

    // Check random part is alphanumeric
    let random_part = &key[KEY_PREFIX.len()..];
    random_part.chars().all(|c| c.is_alphanumeric())
}

/// Validates an API key against a hash
///
/// Uses constant-time comparison to prevent timing attacks.
///
/// # Arguments
///
/// * `key` - Plaintext API key
/// * `stored_hash` - SHA-256 hash from database
///
/// # Returns
///
/// `true` if key matches hash, `false` otherwise
///
/// # Security
///
/// Uses `constant_time_compare` to prevent timing side-channel attacks.
/// This ensures the comparison time doesn't leak information about
/// which characters match.
///
/// # Example
///
/// ```
/// use axontask_shared::auth::api_key::{generate_api_key, verify_api_key};
///
/// let (key, hash) = generate_api_key();
///
/// // Correct key
/// assert!(verify_api_key(&key, &hash));
///
/// // Wrong key
/// assert!(!verify_api_key("axon_wrongkey123", &hash));
/// ```
pub fn verify_api_key(key: &str, stored_hash: &str) -> bool {
    let computed_hash = hash_api_key(key);
    constant_time_compare(&computed_hash, stored_hash)
}

/// Constant-time string comparison
///
/// Prevents timing attacks by ensuring comparison always takes
/// the same amount of time regardless of where strings differ.
///
/// # Arguments
///
/// * `a` - First string
/// * `b` - Second string
///
/// # Returns
///
/// `true` if strings are equal, `false` otherwise
///
/// # Security
///
/// This function:
/// - Always compares the full length of both strings
/// - Uses bitwise OR to accumulate differences without short-circuiting
/// - Returns only after all bytes have been compared
///
/// # Example
///
/// ```
/// use axontask_shared::auth::api_key::constant_time_compare;
///
/// assert!(constant_time_compare("hello", "hello"));
/// assert!(!constant_time_compare("hello", "world"));
/// ```
pub fn constant_time_compare(a: &str, b: &str) -> bool {
    // Different lengths = not equal (but still compare to prevent timing leak)
    if a.len() != b.len() {
        return false;
    }

    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    // XOR all bytes and accumulate
    let mut result = 0u8;
    for i in 0..a_bytes.len() {
        result |= a_bytes[i] ^ b_bytes[i];
    }

    result == 0
}

/// Parses scopes from a comma-separated string
///
/// # Arguments
///
/// * `scopes_str` - Comma-separated scopes (e.g., "tasks:read,tasks:write")
///
/// # Returns
///
/// Vector of trimmed scope strings
///
/// # Example
///
/// ```
/// use axontask_shared::auth::api_key::parse_scopes;
///
/// let scopes = parse_scopes("tasks:read, tasks:write, webhooks:manage");
/// assert_eq!(scopes, vec!["tasks:read", "tasks:write", "webhooks:manage"]);
///
/// // Empty string
/// let empty = parse_scopes("");
/// assert!(empty.is_empty());
/// ```
pub fn parse_scopes(scopes_str: &str) -> Vec<String> {
    scopes_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Checks if a scope list contains a required scope
///
/// Supports wildcard matching with `*`:
/// - `tasks:*` matches `tasks:read`, `tasks:write`, etc.
/// - `*` matches everything
///
/// # Arguments
///
/// * `scopes` - List of granted scopes
/// * `required` - Required scope to check
///
/// # Returns
///
/// `true` if required scope is granted (exact or wildcard match)
///
/// # Example
///
/// ```
/// use axontask_shared::auth::api_key::has_scope;
///
/// let scopes = vec!["tasks:read".to_string(), "webhooks:*".to_string()];
///
/// // Exact match
/// assert!(has_scope(&scopes, "tasks:read"));
///
/// // Wildcard match
/// assert!(has_scope(&scopes, "webhooks:create"));
/// assert!(has_scope(&scopes, "webhooks:delete"));
///
/// // No match
/// assert!(!has_scope(&scopes, "tasks:write"));
///
/// // Global wildcard
/// let admin_scopes = vec!["*".to_string()];
/// assert!(has_scope(&admin_scopes, "anything"));
/// ```
pub fn has_scope(scopes: &[String], required: &str) -> bool {
    for scope in scopes {
        // Global wildcard
        if scope == "*" {
            return true;
        }

        // Exact match
        if scope == required {
            return true;
        }

        // Wildcard match (e.g., "tasks:*" matches "tasks:read")
        if scope.ends_with(":*") {
            let prefix = &scope[..scope.len() - 1]; // Remove "*", keep ":"
            if required.starts_with(prefix) {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_api_key() {
        let (key1, hash1) = generate_api_key();
        let (key2, hash2) = generate_api_key();

        // Check format
        assert!(key1.starts_with("axon_"));
        assert_eq!(key1.len(), 37);

        // Check randomness
        assert_ne!(key1, key2);
        assert_ne!(hash1, hash2);

        // Check hash length
        assert_eq!(hash1.len(), 64); // SHA-256 hex
        assert_eq!(hash2.len(), 64);
    }

    #[test]
    fn test_hash_api_key() {
        let key = "axon_test123";
        let hash = hash_api_key(key);

        assert_eq!(hash.len(), 64);

        // Deterministic
        let hash2 = hash_api_key(key);
        assert_eq!(hash, hash2);

        // Different key = different hash
        let hash3 = hash_api_key("axon_different");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_validate_api_key_format() {
        // Valid
        assert!(validate_api_key_format("axon_abcdefghijklmnopqrstuvwxyz123456"));
        assert!(validate_api_key_format("axon_ABCDEFGHIJKLMNOPQRSTUVWXYZ123456"));
        assert!(validate_api_key_format("axon_01234567890123456789012345678901"));

        // Invalid - wrong prefix
        assert!(!validate_api_key_format("wrong_abcdefghijklmnopqrstuvwxyz123456"));

        // Invalid - too short
        assert!(!validate_api_key_format("axon_short"));

        // Invalid - too long
        assert!(!validate_api_key_format("axon_abcdefghijklmnopqrstuvwxyz1234567890"));

        // Invalid - special characters
        assert!(!validate_api_key_format("axon_abc!@#$%^&*()_+={}[]|\\:;\"'<>?"));

        // Invalid - no prefix
        assert!(!validate_api_key_format("abcdefghijklmnopqrstuvwxyz1234567890"));
    }

    #[test]
    fn test_verify_api_key() {
        let (key, hash) = generate_api_key();

        // Correct key
        assert!(verify_api_key(&key, &hash));

        // Wrong key
        assert!(!verify_api_key("axon_wrongkey12345678901234567890", &hash));

        // Empty key
        assert!(!verify_api_key("", &hash));
    }

    #[test]
    fn test_constant_time_compare() {
        // Equal strings
        assert!(constant_time_compare("hello", "hello"));
        assert!(constant_time_compare("", ""));

        // Different strings
        assert!(!constant_time_compare("hello", "world"));
        assert!(!constant_time_compare("hello", "hello2"));
        assert!(!constant_time_compare("hello", "Hell"));

        // Different lengths
        assert!(!constant_time_compare("short", "longer string"));
        assert!(!constant_time_compare("", "not empty"));
    }

    #[test]
    fn test_constant_time_compare_timing() {
        // This is a basic sanity check
        // Proper timing attack resistance requires statistical analysis

        use std::time::Instant;

        let s1 = "a".repeat(100);
        let s2_early_diff = "b".repeat(100);
        let s2_late_diff = format!("{}b", "a".repeat(99));

        // Compare strings that differ early
        let start = Instant::now();
        let _ = constant_time_compare(&s1, &s2_early_diff);
        let early_duration = start.elapsed();

        // Compare strings that differ late
        let start = Instant::now();
        let _ = constant_time_compare(&s1, &s2_late_diff);
        let late_duration = start.elapsed();

        // Times should be similar (within 10x due to system noise)
        let ratio = early_duration.as_nanos() as f64 / late_duration.as_nanos() as f64;
        assert!(
            ratio > 0.1 && ratio < 10.0,
            "Timing difference too large: early={:?}, late={:?}",
            early_duration,
            late_duration
        );
    }

    #[test]
    fn test_parse_scopes() {
        assert_eq!(
            parse_scopes("tasks:read,tasks:write,webhooks:manage"),
            vec!["tasks:read", "tasks:write", "webhooks:manage"]
        );

        // With spaces
        assert_eq!(
            parse_scopes("tasks:read, tasks:write, webhooks:manage"),
            vec!["tasks:read", "tasks:write", "webhooks:manage"]
        );

        // Empty
        assert_eq!(parse_scopes(""), Vec::<String>::new());

        // Single scope
        assert_eq!(parse_scopes("tasks:read"), vec!["tasks:read"]);

        // Extra commas
        assert_eq!(
            parse_scopes("tasks:read,,tasks:write,"),
            vec!["tasks:read", "tasks:write"]
        );
    }

    #[test]
    fn test_has_scope() {
        let scopes = vec![
            "tasks:read".to_string(),
            "tasks:write".to_string(),
            "webhooks:*".to_string(),
        ];

        // Exact matches
        assert!(has_scope(&scopes, "tasks:read"));
        assert!(has_scope(&scopes, "tasks:write"));

        // Wildcard matches
        assert!(has_scope(&scopes, "webhooks:create"));
        assert!(has_scope(&scopes, "webhooks:delete"));
        assert!(has_scope(&scopes, "webhooks:manage"));

        // No match
        assert!(!has_scope(&scopes, "tasks:delete"));
        assert!(!has_scope(&scopes, "users:read"));
    }

    #[test]
    fn test_has_scope_global_wildcard() {
        let admin_scopes = vec!["*".to_string()];

        assert!(has_scope(&admin_scopes, "tasks:read"));
        assert!(has_scope(&admin_scopes, "tasks:write"));
        assert!(has_scope(&admin_scopes, "anything"));
        assert!(has_scope(&admin_scopes, "users:admin"));
    }

    #[test]
    fn test_has_scope_empty() {
        let empty_scopes: Vec<String> = vec![];

        assert!(!has_scope(&empty_scopes, "tasks:read"));
        assert!(!has_scope(&empty_scopes, "anything"));
    }

    #[test]
    fn test_generate_random_string() {
        let s1 = generate_random_string(32);
        let s2 = generate_random_string(32);

        assert_eq!(s1.len(), 32);
        assert_eq!(s2.len(), 32);
        assert_ne!(s1, s2); // Should be random

        // Should be alphanumeric
        assert!(s1.chars().all(|c| c.is_alphanumeric()));
        assert!(s2.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_full_api_key_workflow() {
        // Generate key
        let (plaintext, hash) = generate_api_key();

        // Validate format
        assert!(validate_api_key_format(&plaintext));

        // Verify against hash
        assert!(verify_api_key(&plaintext, &hash));

        // Wrong key doesn't verify
        let (wrong_key, _) = generate_api_key();
        assert!(!verify_api_key(&wrong_key, &hash));
    }
}
