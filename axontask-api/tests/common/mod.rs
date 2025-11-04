/// Common test utilities for integration tests
///
/// This module provides shared infrastructure for integration tests:
/// - Test database setup and cleanup
/// - Test Redis connection
/// - Test user/tenant creation
/// - JWT token generation
/// - API client helpers

use axontask_api::app::{build_router, AppState};
use axontask_api::config::Config;
use axontask_shared::auth::jwt::{Claims, TokenType, create_token};
use axontask_shared::models::membership::{CreateMembership, Membership, MembershipRole};
use axontask_shared::models::tenant::{CreateTenant, Tenant, TenantPlan};
use axontask_shared::models::user::{CreateUser, User};
use axontask_shared::redis::{RedisClient, RedisConfig};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Test context containing all necessary resources
pub struct TestContext {
    pub db: PgPool,
    pub redis: RedisClient,
    pub app: axum::Router,
    pub config: Config,
    pub tenant: Tenant,
    pub user: User,
    pub jwt_token: String,
}

impl TestContext {
    /// Creates a new test context with fresh database and Redis
    pub async fn new() -> anyhow::Result<Self> {
        // Load test configuration
        let config = Config::from_env()?;

        // Connect to database
        let db = PgPool::connect(&config.database.url).await?;

        // Run migrations (path relative to Cargo.toml, not this file)
        sqlx::migrate!("../migrations").run(&db).await?;

        // Connect to Redis
        let redis_config = RedisConfig::from_env()?;
        let redis = RedisClient::new(redis_config).await?;

        // Create test tenant
        let tenant = Tenant::create(
            &db,
            CreateTenant {
                name: format!("Test Tenant {}", Uuid::new_v4()),
                plan: TenantPlan::Pro, // Use Pro for testing to avoid quota limits
            },
        )
        .await?;

        // Create test user
        let user = User::create(
            &db,
            CreateUser {
                email: format!("test-{}@example.com", Uuid::new_v4()),
                password_hash: "test_hash".to_string(), // Not used in tests
                name: Some("Test User".to_string()),
                avatar_url: None,
            },
        )
        .await?;

        // Create membership
        Membership::create(
            &db,
            CreateMembership {
                tenant_id: tenant.id,
                user_id: user.id,
                role: MembershipRole::Owner,
            },
        )
        .await?;

        // Generate JWT token
        let claims = Claims::new(user.id, tenant.id, TokenType::Access);
        let jwt_token = create_token(&claims, &config.jwt.secret)?;

        // Build app
        let state = AppState::new(db.clone(), config.clone());
        let app = build_router(state);

        Ok(TestContext {
            db,
            redis,
            app,
            config,
            tenant,
            user,
            jwt_token,
        })
    }

    /// Returns authorization header value
    pub fn auth_header(&self) -> String {
        format!("Bearer {}", self.jwt_token)
    }

    /// Cleans up test data
    pub async fn cleanup(&self) -> anyhow::Result<()> {
        // Delete test tenant (cascades to users, tasks, etc.)
        Tenant::delete(&self.db, self.tenant.id).await?;
        Ok(())
    }
}

/// Helper to create a test task
pub async fn create_test_task(
    ctx: &TestContext,
    name: &str,
    adapter: &str,
    args: serde_json::Value,
) -> anyhow::Result<Uuid> {
    use axontask_shared::models::task::{CreateTask, Task};

    let task = Task::create(
        &ctx.db,
        CreateTask {
            tenant_id: ctx.tenant.id,
            created_by: Some(ctx.user.id),
            name: name.to_string(),
            adapter: adapter.to_string(),
            args,
            timeout_seconds: 60,
        },
    )
    .await?;

    Ok(task.id)
}

/// Helper to wait for condition with timeout
pub async fn wait_for<F, Fut>(
    condition: F,
    timeout_secs: u64,
) -> anyhow::Result<()>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    loop {
        if condition().await {
            return Ok(());
        }

        if start.elapsed() > timeout {
            anyhow::bail!("Condition not met within {} seconds", timeout_secs);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}
