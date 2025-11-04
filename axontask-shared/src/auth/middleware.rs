/// Authentication middleware for Axum
///
/// This module provides middleware for JWT and API key authentication in Axum applications.
/// Middleware extracts credentials from requests, validates them, and adds authentication
/// context to request extensions.
///
/// # Middleware Types
///
/// - **JWT Middleware**: Validates Bearer tokens from Authorization header
/// - **API Key Middleware**: Validates API keys from X-Api-Key header
///
/// # Request Extensions
///
/// After successful authentication, middleware adds:
/// - `AuthContext`: Contains user_id, tenant_id, and authentication method
///
/// # Example
///
/// ```no_run
/// use axum::{Router, routing::get};
/// use axontask_shared::auth::middleware::{jwt_auth, AuthContext};
///
/// async fn protected_handler(auth: AuthContext) -> String {
///     format!("Hello, user {}!", auth.user_id)
/// }
///
/// let app = Router::new()
///     .route("/protected", get(protected_handler))
///     .layer(jwt_auth("your-jwt-secret"));
/// ```

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::jwt::{validate_access_token, JwtError};
use crate::models::api_key::ApiKey;

/// Authentication method used
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    /// JWT token authentication
    Jwt,

    /// API key authentication
    ApiKey,
}

/// Authentication context added to request extensions
///
/// This struct is added to the request after successful authentication.
/// Handlers can extract it using Axum's `Extension` extractor.
///
/// # Example
///
/// ```
/// use axum::Extension;
/// use axontask_shared::auth::middleware::AuthContext;
///
/// async fn handler(Extension(auth): Extension<AuthContext>) -> String {
///     format!("User: {}, Tenant: {}", auth.user_id, auth.tenant_id)
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    /// Authenticated user ID (None for API key auth)
    pub user_id: Option<Uuid>,

    /// Current tenant ID
    pub tenant_id: Uuid,

    /// Authentication method used
    pub method: AuthMethod,

    /// API key scopes (only for API key auth)
    pub scopes: Option<Vec<String>>,

    /// API key ID (only for API key auth)
    pub api_key_id: Option<Uuid>,
}

impl AuthContext {
    /// Creates auth context from JWT claims
    pub fn from_jwt(user_id: Uuid, tenant_id: Uuid) -> Self {
        Self {
            user_id: Some(user_id),
            tenant_id,
            method: AuthMethod::Jwt,
            scopes: None,
            api_key_id: None,
        }
    }

    /// Creates auth context from API key
    pub fn from_api_key(api_key: &ApiKey) -> Self {
        Self {
            user_id: None, // API keys are not user-specific
            tenant_id: api_key.tenant_id,
            method: AuthMethod::ApiKey,
            scopes: Some(api_key.scopes.clone()),
            api_key_id: Some(api_key.id),
        }
    }

    /// Checks if auth context has a specific scope
    ///
    /// For JWT auth, always returns true (full access).
    /// For API key auth, checks the scopes list.
    pub fn has_scope(&self, required_scope: &str) -> bool {
        match self.method {
            AuthMethod::Jwt => true, // JWT users have full access
            AuthMethod::ApiKey => {
                if let Some(ref scopes) = self.scopes {
                    super::api_key::has_scope(scopes, required_scope)
                } else {
                    false
                }
            }
        }
    }
}

/// Error type for authentication middleware
#[derive(Debug)]
pub enum AuthError {
    /// Missing authorization header
    MissingCredentials,

    /// Invalid authorization header format
    InvalidFormat(String),

    /// Token validation failed
    InvalidToken(String),

    /// API key validation failed
    InvalidApiKey(String),

    /// Database error
    DatabaseError(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            AuthError::MissingCredentials => {
                (StatusCode::UNAUTHORIZED, "Missing credentials").into_response()
            }
            AuthError::InvalidFormat(msg) => {
                (StatusCode::BAD_REQUEST, msg).into_response()
            }
            AuthError::InvalidToken(msg) => {
                (StatusCode::UNAUTHORIZED, msg).into_response()
            }
            AuthError::InvalidApiKey(msg) => {
                (StatusCode::UNAUTHORIZED, msg).into_response()
            }
            AuthError::DatabaseError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
        }
    }
}

/// JWT authentication middleware
///
/// Validates JWT tokens from the `Authorization: Bearer <token>` header.
///
/// # Arguments
///
/// * `secret` - JWT secret for validation
/// * `req` - Request
/// * `next` - Next middleware/handler
///
/// # Returns
///
/// Response with `AuthContext` extension added on success
///
/// # Errors
///
/// Returns 401 Unauthorized if:
/// - Authorization header is missing
/// - Token format is invalid
/// - Token validation fails
/// - Token has expired
///
/// # Example
///
/// ```no_run
/// use axum::{Router, routing::get, middleware};
/// use axontask_shared::auth::middleware::create_jwt_middleware;
///
/// async fn handler() -> &'static str {
///     "Protected route"
/// }
///
/// let app = Router::new()
///     .route("/protected", get(handler))
///     .layer(middleware::from_fn(create_jwt_middleware("secret")));
/// ```
pub async fn jwt_auth_middleware(
    secret: String,
    mut req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    // Extract Authorization header
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(AuthError::MissingCredentials)?;

    // Parse Bearer token
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AuthError::InvalidFormat("Expected Bearer token".to_string()))?;

    // Validate token
    let claims = validate_access_token(token, &secret).map_err(|e| match e {
        JwtError::Expired => AuthError::InvalidToken("Token expired".to_string()),
        JwtError::InvalidIssuer { .. } => AuthError::InvalidToken("Invalid issuer".to_string()),
        _ => AuthError::InvalidToken(format!("Invalid token: {}", e)),
    })?;

    // Add auth context to request extensions
    let auth_context = AuthContext::from_jwt(claims.sub, claims.tenant_id);
    req.extensions_mut().insert(auth_context);

    Ok(next.run(req).await)
}

/// API key authentication middleware
///
/// Validates API keys from the `X-Api-Key` header.
/// Performs database lookup to validate the key and retrieve permissions.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `req` - Request
/// * `next` - Next middleware/handler
///
/// # Returns
///
/// Response with `AuthContext` extension added on success
///
/// # Errors
///
/// Returns 401 Unauthorized if:
/// - X-Api-Key header is missing
/// - API key format is invalid
/// - API key doesn't exist or is revoked
/// - API key has expired
/// - Database error occurs
///
/// # Example
///
/// ```no_run
/// use axum::{Router, routing::get, middleware, Extension};
/// use axontask_shared::auth::middleware::create_api_key_middleware;
/// use sqlx::PgPool;
///
/// async fn handler() -> &'static str {
///     "Protected route"
/// }
///
/// async fn setup(pool: PgPool) -> Router {
///     Router::new()
///         .route("/api/tasks", get(handler))
///         .layer(middleware::from_fn(move |req, next| {
///             create_api_key_middleware(pool.clone(), req, next)
///         }))
/// }
/// ```
pub async fn api_key_auth_middleware(
    pool: PgPool,
    mut req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    // Extract X-Api-Key header
    let api_key_header = req
        .headers()
        .get("X-Api-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or(AuthError::MissingCredentials)?;

    // Validate format
    if !super::api_key::validate_api_key_format(api_key_header) {
        return Err(AuthError::InvalidFormat("Invalid API key format".to_string()));
    }

    // Validate API key (database lookup)
    let api_key = ApiKey::validate(&pool, api_key_header)
        .await
        .map_err(|e| AuthError::DatabaseError(format!("Database error: {}", e)))?
        .ok_or_else(|| AuthError::InvalidApiKey("Invalid or revoked API key".to_string()))?;

    // Check expiration
    if api_key.is_expired() {
        return Err(AuthError::InvalidApiKey("API key has expired".to_string()));
    }

    // Add auth context to request extensions
    let auth_context = AuthContext::from_api_key(&api_key);
    req.extensions_mut().insert(auth_context);

    Ok(next.run(req).await)
}

/// Creates a JWT authentication middleware closure
///
/// Helper function that captures the JWT secret and returns a middleware function.
///
/// # Example
///
/// ```no_run
/// use axum::{Router, routing::get, middleware};
/// use axontask_shared::auth::middleware::create_jwt_middleware;
///
/// let app = Router::new()
///     .route("/protected", get(|| async { "OK" }))
///     .layer(middleware::from_fn(create_jwt_middleware("secret")));
/// ```
pub fn create_jwt_middleware(
    secret: impl Into<String>,
) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, AuthError>> + Send>> + Clone {
    let secret = secret.into();
    move |req, next| {
        let secret = secret.clone();
        Box::pin(jwt_auth_middleware(secret, req, next))
    }
}

/// Creates an API key authentication middleware closure
///
/// Helper function that captures the database pool and returns a middleware function.
///
/// # Example
///
/// ```no_run
/// use axum::{Router, routing::get, middleware};
/// use axontask_shared::auth::middleware::create_api_key_middleware;
/// use sqlx::PgPool;
///
/// async fn setup(pool: PgPool) -> Router {
///     Router::new()
///         .route("/api/tasks", get(|| async { "OK" }))
///         .layer(middleware::from_fn(create_api_key_middleware(pool)))
/// }
/// ```
pub fn create_api_key_middleware(
    pool: PgPool,
) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, AuthError>> + Send>> + Clone {
    move |req, next| {
        let pool = pool.clone();
        Box::pin(api_key_auth_middleware(pool, req, next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::jwt::{Claims, TokenType};

    #[test]
    fn test_auth_context_from_jwt() {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let context = AuthContext::from_jwt(user_id, tenant_id);

        assert_eq!(context.user_id, user_id);
        assert_eq!(context.tenant_id, tenant_id);
        assert_eq!(context.method, AuthMethod::Jwt);
        assert!(context.scopes.is_none());
        assert!(context.api_key_id.is_none());
    }

    #[test]
    fn test_auth_context_has_scope_jwt() {
        let context = AuthContext::from_jwt(Uuid::new_v4(), Uuid::new_v4());

        // JWT users have all scopes
        assert!(context.has_scope("tasks:read"));
        assert!(context.has_scope("tasks:write"));
        assert!(context.has_scope("anything"));
    }

    #[test]
    fn test_auth_error_into_response() {
        let err = AuthError::MissingCredentials;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let err = AuthError::InvalidFormat("test".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let err = AuthError::DatabaseError("test".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
