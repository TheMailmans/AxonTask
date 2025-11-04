/// Application state and router builder
///
/// This module defines the shared application state and provides
/// a function to build the Axum router with all routes and middleware.
///
/// # Example
///
/// ```no_run
/// use axontask_api::{app::AppState, config::Config};
/// use sqlx::PgPool;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = Config::from_env()?;
/// let pool = PgPool::connect(&config.database.url).await?;
/// let state = AppState::new(pool, config);
/// let app = axontask_api::app::build_router(state);
/// # Ok(())
/// # }
/// ```

use crate::{config::Config, middleware::security::SecurityHeadersLayer};
use axum::{
    extract::Request,
    http::{header, HeaderValue, Method},
    middleware::Next,
    response::Response,
    routing::{get, post},
    Router,
};
use axontask_shared::auth::{jwt, middleware::AuthContext};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::{
    cors::CorsLayer,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::Level;

/// Shared application state
///
/// This is cloned for each request handler via Axum's `State` extractor.
/// Uses Arc internally for cheap cloning.
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool
    pub db: PgPool,

    /// Application configuration
    pub config: Arc<Config>,
}

impl AppState {
    /// Creates new application state
    pub fn new(db: PgPool, config: Config) -> Self {
        Self {
            db,
            config: Arc::new(config),
        }
    }

    /// Gets JWT secret for token operations
    pub fn jwt_secret(&self) -> &str {
        &self.config.jwt.secret
    }
}

/// Builds the complete Axum router with all routes and middleware
///
/// # Architecture
///
/// The router is organized as follows:
/// ```text
/// /
/// ├── /health                   # Health check (public)
/// ├── /v1/                      # API v1 (versioned)
/// │   ├── /auth/                # Authentication endpoints
/// │   │   ├── POST /register
/// │   │   ├── POST /login
/// │   │   └── POST /refresh
/// │   └── /api-keys/            # API key management (authenticated)
/// │       ├── POST   /          # Create API key
/// │       ├── GET    /          # List API keys
/// │       └── DELETE /:id       # Revoke API key
/// ```
///
/// # Middleware Stack
///
/// Applied in order (bottom to top):
/// 1. Logging (tower-http TraceLayer)
/// 2. CORS (tower-http CorsLayer)
/// 3. Authentication (per-route basis)
///
/// # Example
///
/// ```no_run
/// use axontask_api::app::{AppState, build_router};
/// use sqlx::PgPool;
/// use axontask_api::config::Config;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = Config::from_env()?;
/// let pool = PgPool::connect(&config.database.url).await?;
/// let state = AppState::new(pool, config);
///
/// let app = build_router(state);
///
/// // Start server
/// let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
/// axum::serve(listener, app).await?;
/// # Ok(())
/// # }
/// ```
pub fn build_router(state: AppState) -> Router {
    // Import route handlers
    use crate::routes;

    // Health check (public, no auth)
    let health_routes = Router::new()
        .route("/health", get(routes::health::health_check));

    // Auth routes (public, no auth required)
    let auth_routes = Router::new()
        .route("/register", post(routes::auth::register))
        .route("/login", post(routes::auth::login))
        .route("/refresh", post(routes::auth::refresh));

    // API key routes (require JWT authentication)
    let api_key_routes = Router::new()
        .route("/", post(routes::api_keys::create_api_key))
        .route("/", get(routes::api_keys::list_api_keys))
        .route("/:id", post(routes::api_keys::revoke_api_key))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            jwt_auth_layer,
        ));

    // MCP tool routes (require JWT or API key authentication + rate limiting)
    let mcp_routes = Router::new()
        .route("/start_task", post(routes::mcp::start_task))
        .route("/tasks/:task_id/status", get(routes::mcp::get_task_status))
        .route("/tasks/:task_id/cancel", post(routes::mcp::cancel_task))
        .route("/tasks/:task_id/stream", get(routes::mcp::stream_task))
        .route("/tasks/:task_id/resume", post(routes::mcp::resume_task))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::rate_limit::rate_limit_layer,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            jwt_auth_layer,
        ));

    // Build complete v1 API
    let v1_routes = Router::new()
        .nest("/auth", auth_routes)
        .nest("/api-keys", api_key_routes)
        .nest("/mcp", mcp_routes);

    // Configure CORS based on environment
    let cors = if state.config.api.cors_origins.contains(&"*".to_string()) {
        // Development mode: permissive CORS
        CorsLayer::permissive()
    } else {
        // Production mode: configure allowed origins
        let origins: Vec<HeaderValue> = state
            .config
            .api
            .cors_origins
            .iter()
            .filter_map(|origin| origin.parse().ok())
            .collect();

        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
            .allow_credentials(true)
            .max_age(std::time::Duration::from_secs(3600))
    };

    // Combine all routes with middleware stack
    Router::new()
        .merge(health_routes)
        .nest("/v1", v1_routes)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(cors)
        .layer(SecurityHeadersLayer::new(state.config.api.production))
        .with_state(state)
}

/// JWT authentication middleware layer
///
/// Extracts and validates JWT token from Authorization header,
/// then injects AuthContext into request extensions.
async fn jwt_auth_layer(
    state: axum::extract::State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, crate::error::ApiError> {
    // Extract Authorization header
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| crate::error::ApiError::Unauthorized("Missing authorization header".to_string()))?;

    // Parse Bearer token
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| crate::error::ApiError::BadRequest("Expected Bearer token".to_string()))?;

    // Validate token
    let claims = jwt::validate_access_token(token, state.jwt_secret())?;

    // Create auth context
    let auth_context = AuthContext::from_jwt(claims.sub, claims.tenant_id);

    // Insert into request extensions
    req.extensions_mut().insert(auth_context);

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_creation() {
        // This is just a compile test to ensure AppState is properly structured
        // Real integration tests will use actual database connections
    }
}
