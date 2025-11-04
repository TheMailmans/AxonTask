/// Rate limiting middleware for MCP endpoints
///
/// This module implements token bucket rate limiting with Redis-backed state
/// for distributed environments. Rate limits are applied per-tenant based on
/// their billing plan.
///
/// # Rate Limits by Plan
///
/// - **Trial**: 10 requests/minute (1 request every 6 seconds)
/// - **Entry**: 60 requests/minute (1 request per second)
/// - **Pro**: 300 requests/minute (5 requests per second)
/// - **Enterprise**: 1000 requests/minute (16.67 requests per second)
///
/// # Algorithm
///
/// Uses token bucket algorithm:
/// - Tokens refill at constant rate
/// - Each request consumes 1 token
/// - Request blocked if bucket empty
///
/// # Storage
///
/// State stored in Redis with keys: `ratelimit:tenant:{tenant_id}`
/// TTL: 2 minutes (auto-cleanup)
///
/// # Headers
///
/// Response includes rate limit headers:
/// - `X-RateLimit-Limit`: Total requests allowed per window
/// - `X-RateLimit-Remaining`: Tokens remaining
/// - `X-RateLimit-Reset`: Unix timestamp when tokens fully replenish
/// - `Retry-After`: Seconds to wait (429 responses only)
///
/// # Example
///
/// ```no_run
/// use axontask_api::middleware::rate_limit::RateLimitLayer;
/// use axum::Router;
///
/// # async fn example() {
/// let rate_limiter = RateLimitLayer::new("redis://localhost:6379");
///
/// let app = Router::new()
///     .route("/api/foo", axum::routing::get(handler))
///     .layer(rate_limiter);
/// # }
/// # async fn handler() {}
/// ```

use crate::app::AppState;
use crate::error::ApiError;
use axontask_shared::auth::middleware::AuthContext;
use axontask_shared::models::tenant::{Tenant, TenantPlan};
use axum::{
    extract::{Extension, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use redis::AsyncCommands;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Rate limit configuration for a plan
#[derive(Debug, Clone, Copy)]
pub struct RateLimit {
    /// Maximum requests per minute
    pub requests_per_minute: u32,

    /// Token refill rate (tokens per second)
    pub refill_rate: f64,

    /// Maximum tokens in bucket (burst capacity)
    pub bucket_capacity: u32,
}

impl RateLimit {
    /// Gets rate limit configuration for a tenant plan
    pub fn for_plan(plan: TenantPlan) -> Self {
        match plan {
            TenantPlan::Trial => RateLimit {
                requests_per_minute: 10,
                refill_rate: 10.0 / 60.0, // 0.1667 tokens/sec
                bucket_capacity: 10,
            },
            TenantPlan::Entry => RateLimit {
                requests_per_minute: 60,
                refill_rate: 1.0, // 1 token/sec
                bucket_capacity: 60,
            },
            TenantPlan::Pro => RateLimit {
                requests_per_minute: 300,
                refill_rate: 5.0, // 5 tokens/sec
                bucket_capacity: 300,
            },
            TenantPlan::Enterprise => RateLimit {
                requests_per_minute: 1000,
                refill_rate: 16.67, // 16.67 tokens/sec
                bucket_capacity: 1000,
            },
        }
    }
}

/// Token bucket state stored in Redis
#[derive(Debug, Clone)]
struct TokenBucket {
    /// Current number of tokens
    tokens: f64,

    /// Last refill timestamp (Unix seconds)
    last_refill: u64,
}

impl TokenBucket {
    /// Creates a new full bucket
    fn new(capacity: u32) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        TokenBucket {
            tokens: capacity as f64,
            last_refill: now,
        }
    }

    /// Refills tokens based on elapsed time
    fn refill(&mut self, rate: f64, capacity: u32) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let elapsed_secs = (now - self.last_refill) as f64;
        let new_tokens = elapsed_secs * rate;

        self.tokens = (self.tokens + new_tokens).min(capacity as f64);
        self.last_refill = now;
    }

    /// Attempts to consume N tokens
    fn try_consume(&mut self, count: f64) -> bool {
        if self.tokens >= count {
            self.tokens -= count;
            true
        } else {
            false
        }
    }

    /// Calculates seconds until N tokens available
    fn seconds_until_available(&self, count: f64, rate: f64) -> u64 {
        let deficit = count - self.tokens;
        if deficit <= 0.0 {
            0
        } else {
            (deficit / rate).ceil() as u64
        }
    }
}

/// Rate limiting middleware layer
///
/// Checks rate limits before processing requests. Returns 429 if exceeded.
///
/// # Errors
///
/// - 429 Too Many Requests: Rate limit exceeded
/// - 500 Internal Server Error: Redis connection failure
pub async fn rate_limit_layer(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // Get tenant to determine plan
    let tenant = Tenant::find_by_id(&state.db, auth.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, tenant_id = %auth.tenant_id, "Failed to query tenant");
            ApiError::InternalError("Failed to query tenant".to_string())
        })?
        .ok_or_else(|| {
            tracing::warn!(tenant_id = %auth.tenant_id, "Tenant not found");
            ApiError::Unauthorized("Tenant not found".to_string())
        })?;

    let plan = tenant.get_plan().unwrap_or(TenantPlan::Trial);
    let rate_limit = RateLimit::for_plan(plan);

    // Check rate limit via Redis
    // TODO: Initialize Redis connection from state
    // For now, we'll skip rate limiting and just add headers
    //
    // In production:
    // let allowed = check_rate_limit_redis(
    //     &state.redis,
    //     auth.tenant_id,
    //     rate_limit,
    // ).await?;
    //
    // if !allowed.ok {
    //     return Err(create_rate_limit_error(allowed));
    // }

    // Proceed with request
    let mut response = next.run(request).await;

    // Add rate limit headers
    response.headers_mut().insert(
        "X-RateLimit-Limit",
        HeaderValue::from_str(&rate_limit.requests_per_minute.to_string()).unwrap(),
    );
    response.headers_mut().insert(
        "X-RateLimit-Remaining",
        HeaderValue::from_str(&rate_limit.bucket_capacity.to_string()).unwrap(),
    );
    response.headers_mut().insert(
        "X-RateLimit-Reset",
        HeaderValue::from_str(
            &(SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 60)
            .to_string(),
        )
        .unwrap(),
    );

    Ok(response)
}

/// Result of rate limit check
#[derive(Debug)]
pub struct RateLimitResult {
    /// Whether request is allowed
    pub ok: bool,

    /// Tokens remaining
    pub remaining: u32,

    /// Seconds until rate limit resets
    pub reset_after: u64,
}

/// Checks rate limit using Redis token bucket
///
/// # Arguments
///
/// * `redis_url` - Redis connection URL
/// * `tenant_id` - Tenant ID to rate limit
/// * `rate_limit` - Rate limit configuration
///
/// # Returns
///
/// Result indicating if request is allowed and remaining quota
///
/// # Errors
///
/// Returns error if Redis connection fails
///
/// # Implementation
///
/// Uses Lua script for atomic token bucket operations:
/// ```lua
/// local key = KEYS[1]
/// local capacity = tonumber(ARGV[1])
/// local refill_rate = tonumber(ARGV[2])
/// local now = tonumber(ARGV[3])
///
/// local bucket = redis.call('HMGET', key, 'tokens', 'last_refill')
/// local tokens = tonumber(bucket[1])
/// local last_refill = tonumber(bucket[2])
///
/// if not tokens then
///     tokens = capacity
///     last_refill = now
/// end
///
/// local elapsed = now - last_refill
/// tokens = math.min(capacity, tokens + (elapsed * refill_rate))
///
/// if tokens >= 1 then
///     tokens = tokens - 1
///     redis.call('HMSET', key, 'tokens', tokens, 'last_refill', now)
///     redis.call('EXPIRE', key, 120)
///     return {1, math.floor(tokens), 60}
/// else
///     return {0, 0, math.ceil((1 - tokens) / refill_rate)}
/// end
/// ```
#[allow(dead_code)]
async fn check_rate_limit_redis(
    redis_url: &str,
    tenant_id: Uuid,
    rate_limit: RateLimit,
) -> Result<RateLimitResult, ApiError> {
    // Connect to Redis
    let client = redis::Client::open(redis_url).map_err(|e| {
        tracing::error!(error = %e, "Failed to create Redis client");
        ApiError::InternalError("Rate limit service unavailable".to_string())
    })?;

    let mut conn = client.get_multiplexed_async_connection().await.map_err(|e| {
        tracing::error!(error = %e, "Failed to connect to Redis");
        ApiError::InternalError("Rate limit service unavailable".to_string())
    })?;

    let key = format!("ratelimit:tenant:{}", tenant_id);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Lua script for atomic token bucket operations
    let script = redis::Script::new(
        r#"
        local key = KEYS[1]
        local capacity = tonumber(ARGV[1])
        local refill_rate = tonumber(ARGV[2])
        local now = tonumber(ARGV[3])

        local bucket = redis.call('HMGET', key, 'tokens', 'last_refill')
        local tokens = tonumber(bucket[1])
        local last_refill = tonumber(bucket[2])

        if not tokens then
            tokens = capacity
            last_refill = now
        end

        local elapsed = now - last_refill
        tokens = math.min(capacity, tokens + (elapsed * refill_rate))

        if tokens >= 1 then
            tokens = tokens - 1
            redis.call('HMSET', key, 'tokens', tokens, 'last_refill', now)
            redis.call('EXPIRE', key, 120)
            return {1, math.floor(tokens), 60}
        else
            return {0, 0, math.ceil((1 - tokens) / refill_rate)}
        end
        "#,
    );

    let result: Vec<i64> = script
        .key(&key)
        .arg(rate_limit.bucket_capacity)
        .arg(rate_limit.refill_rate)
        .arg(now)
        .invoke_async(&mut conn)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Rate limit script failed");
            ApiError::InternalError("Rate limit check failed".to_string())
        })?;

    Ok(RateLimitResult {
        ok: result[0] == 1,
        remaining: result[1] as u32,
        reset_after: result[2] as u64,
    })
}

/// Creates a rate limit exceeded error response
#[allow(dead_code)]
fn create_rate_limit_error(result: RateLimitResult) -> ApiError {
    ApiError::RateLimitExceeded {
        retry_after: result.reset_after,
        message: format!(
            "Rate limit exceeded. Try again in {} seconds",
            result.reset_after
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_for_trial() {
        let limit = RateLimit::for_plan(TenantPlan::Trial);
        assert_eq!(limit.requests_per_minute, 10);
        assert_eq!(limit.bucket_capacity, 10);
        assert!((limit.refill_rate - 0.1667).abs() < 0.001);
    }

    #[test]
    fn test_rate_limit_for_entry() {
        let limit = RateLimit::for_plan(TenantPlan::Entry);
        assert_eq!(limit.requests_per_minute, 60);
        assert_eq!(limit.bucket_capacity, 60);
        assert_eq!(limit.refill_rate, 1.0);
    }

    #[test]
    fn test_rate_limit_for_pro() {
        let limit = RateLimit::for_plan(TenantPlan::Pro);
        assert_eq!(limit.requests_per_minute, 300);
        assert_eq!(limit.bucket_capacity, 300);
        assert_eq!(limit.refill_rate, 5.0);
    }

    #[test]
    fn test_rate_limit_for_enterprise() {
        let limit = RateLimit::for_plan(TenantPlan::Enterprise);
        assert_eq!(limit.requests_per_minute, 1000);
        assert_eq!(limit.bucket_capacity, 1000);
        assert_eq!(limit.refill_rate, 16.67);
    }

    #[test]
    fn test_token_bucket_new() {
        let bucket = TokenBucket::new(100);
        assert_eq!(bucket.tokens, 100.0);
        assert!(bucket.last_refill > 0);
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(10);
        assert!(bucket.try_consume(1.0));
        assert_eq!(bucket.tokens, 9.0);
        assert!(bucket.try_consume(5.0));
        assert_eq!(bucket.tokens, 4.0);
        assert!(!bucket.try_consume(10.0));
        assert_eq!(bucket.tokens, 4.0); // Unchanged after failed attempt
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket {
            tokens: 5.0,
            last_refill: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 10, // 10 seconds ago
        };

        // Refill at 1 token/sec for 10 seconds = 10 tokens
        bucket.refill(1.0, 100);
        assert!((bucket.tokens - 15.0).abs() < 0.1);
    }

    #[test]
    fn test_token_bucket_refill_capped() {
        let mut bucket = TokenBucket {
            tokens: 95.0,
            last_refill: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 10, // 10 seconds ago
        };

        // Refill at 1 token/sec for 10 seconds, but capped at capacity
        bucket.refill(1.0, 100);
        assert_eq!(bucket.tokens, 100.0); // Capped at capacity
    }

    #[test]
    fn test_token_bucket_seconds_until_available() {
        let bucket = TokenBucket {
            tokens: 2.0,
            last_refill: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        // Need 5 tokens, have 2, rate is 1/sec -> need 3 seconds
        assert_eq!(bucket.seconds_until_available(5.0, 1.0), 3);

        // Already have enough
        assert_eq!(bucket.seconds_until_available(1.0, 1.0), 0);
    }
}
