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

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "axontask_api=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!(
        "AxonTask API Server v{} starting...",
        env!("CARGO_PKG_VERSION")
    );

    // TODO: Load configuration
    // TODO: Initialize database pool
    // TODO: Initialize Redis client
    // TODO: Build Axum application
    // TODO: Start server

    tracing::info!("Server listening on http://127.0.0.1:8080");
    tracing::info!("This is a Phase 0 placeholder. Server functionality will be implemented in Phase 3.");

    // Keep the server running (placeholder)
    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutdown signal received, exiting...");

    Ok(())
}
