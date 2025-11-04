/// Health check endpoint
///
/// Provides a simple health check endpoint that verifies:
/// - The server is running
/// - Database connectivity
///
/// # Endpoint
///
/// ```text
/// GET /health
/// ```
///
/// # Response
///
/// ```json
/// {
///   "status": "healthy",
///   "version": "0.1.0",
///   "database": "connected"
/// }
/// ```

use crate::{app::AppState, error::ApiResult};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service status
    pub status: String,

    /// Application version
    pub version: String,

    /// Database status
    pub database: String,
}

/// Health check handler
///
/// Returns service health status including database connectivity.
///
/// # Example
///
/// ```text
/// GET /health
/// ```
///
/// Response:
/// ```json
/// {
///   "status": "healthy",
///   "version": "0.1.0",
///   "database": "connected"
/// }
/// ```
pub async fn health_check(State(state): State<AppState>) -> ApiResult<Json<HealthResponse>> {
    // Check database connectivity
    let database_status = match sqlx::query("SELECT 1").fetch_one(&state.db).await {
        Ok(_) => "connected",
        Err(_) => "disconnected",
    };

    Ok(Json(HealthResponse {
        status: if database_status == "connected" {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        },
        version: env!("CARGO_PKG_VERSION").to_string(),
        database: database_status.to_string(),
    }))
}
