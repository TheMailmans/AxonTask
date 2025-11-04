//! # AxonTask API Server
//!
//! This is the main API server for AxonTask, providing MCP-native endpoints
//! for starting, streaming, and managing background tasks.
//!
//! ## Architecture
//!
//! The API server is built with Axum and provides:
//! - MCP tool endpoints (start_task, stream_task, get_status, cancel_task, resume_task)
//! - Authentication (JWT + API keys)
//! - Rate limiting and quota enforcement
//! - SSE streaming with backfill and live tail
//!
//! ## Usage
//!
//! ```bash
//! cargo run -p axontask-api
//! ```

use axontask_api::{app, config::Config};
use axontask_shared::db::pool;
use sqlx::PgPool;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "axontask_api=info,tower_http=info,sqlx=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!(
        "AxonTask API Server v{} starting...",
        env!("CARGO_PKG_VERSION")
    );

    // Load configuration
    let config = Config::from_env()?;
    tracing::info!("Configuration loaded successfully");

    // Initialize database pool
    let db_config = pool::DatabaseConfig {
        url: config.database.url.clone(),
        max_connections: config.database.max_connections,
        min_connections: 5,
        connect_timeout_seconds: 30,
        idle_timeout_seconds: Some(600),
        max_lifetime_seconds: Some(1800),
        test_before_acquire: true,
    };

    let pool = pool::create_pool(db_config).await?;
    tracing::info!("Database connection pool initialized");

    // Run migrations
    axontask_shared::db::migrations::run_migrations(&pool).await?;
    tracing::info!("Database migrations completed");

    // Create application state
    let state = app::AppState::new(pool, config.clone());

    // Build router
    let app = app::build_router(state);

    // Start server
    let bind_addr = config.bind_address();
    tracing::info!("Server listening on http://{}", bind_addr);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server shut down gracefully");

    Ok(())
}

/// Graceful shutdown handler
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    tracing::info!("Shutdown signal received, shutting down...");
}
