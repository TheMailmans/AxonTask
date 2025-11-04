/// Error handling for the API server
///
/// This module provides a unified error type that maps to HTTP responses.
/// All handlers should return `Result<T, ApiError>` which automatically
/// converts to appropriate HTTP status codes.
///
/// # Example
///
/// ```
/// use axontask_api::error::{ApiError, ApiResult};
/// use axum::Json;
/// use serde_json::json;
///
/// async fn handler() -> ApiResult<Json<serde_json::Value>> {
///     // Business logic that can fail
///     let data = fetch_data().await?;
///     Ok(Json(json!({ "data": data })))
/// }
/// ```

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::fmt;

/// API result type alias
pub type ApiResult<T> = Result<T, ApiError>;

/// Unified API error type
#[derive(Debug)]
pub enum ApiError {
    /// Bad request (400)
    BadRequest(String),

    /// Unauthorized (401)
    Unauthorized(String),

    /// Forbidden (403)
    Forbidden(String),

    /// Not found (404)
    NotFound(String),

    /// Conflict (409) - e.g., duplicate email
    Conflict(String),

    /// Unprocessable entity (422) - validation errors
    ValidationError(Vec<ValidationErrorDetail>),

    /// Too many requests (429)
    RateLimitExceeded {
        retry_after: u64,
        message: String,
    },

    /// Internal server error (500)
    InternalError(String),

    /// Service unavailable (503)
    ServiceUnavailable(String),
}

/// Validation error detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationErrorDetail {
    /// Field that failed validation
    pub field: String,

    /// Error message
    pub message: String,
}

/// Error response format
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error code (e.g., "bad_request", "unauthorized")
    pub error: String,

    /// Human-readable error message
    pub message: String,

    /// Optional validation errors
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Vec<ValidationErrorDetail>>,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            ApiError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            ApiError::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            ApiError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ApiError::Conflict(msg) => write!(f, "Conflict: {}", msg),
            ApiError::ValidationError(errors) => {
                write!(f, "Validation failed: {} errors", errors.len())
            }
            ApiError::RateLimitExceeded { message, .. } => write!(f, "Rate limit exceeded: {}", message),
            ApiError::InternalError(msg) => write!(f, "Internal error: {}", msg),
            ApiError::ServiceUnavailable(msg) => write!(f, "Service unavailable: {}", msg),
        }
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Handle rate limit separately to add Retry-After header
        if let ApiError::RateLimitExceeded { retry_after, message } = &self {
            let body = Json(ErrorResponse {
                error: "rate_limit_exceeded".to_string(),
                message: message.clone(),
                details: None,
            });

            let mut response = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
            response.headers_mut().insert(
                "Retry-After",
                axum::http::HeaderValue::from_str(&retry_after.to_string()).unwrap(),
            );
            return response;
        }

        let (status, error_code, message, details) = match self {
            ApiError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "bad_request",
                msg,
                None,
            ),
            ApiError::Unauthorized(msg) => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                msg,
                None,
            ),
            ApiError::Forbidden(msg) => (
                StatusCode::FORBIDDEN,
                "forbidden",
                msg,
                None,
            ),
            ApiError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                "not_found",
                msg,
                None,
            ),
            ApiError::Conflict(msg) => (
                StatusCode::CONFLICT,
                "conflict",
                msg,
                None,
            ),
            ApiError::ValidationError(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                "Request validation failed".to_string(),
                Some(errors),
            ),
            ApiError::RateLimitExceeded { message, .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_exceeded",
                message,
                None,
            ),
            ApiError::InternalError(msg) => {
                // Log internal errors but don't expose details to clients
                tracing::error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "An internal error occurred".to_string(),
                    None,
                )
            }
            ApiError::ServiceUnavailable(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                msg,
                None,
            ),
        };

        let body = Json(ErrorResponse {
            error: error_code.to_string(),
            message,
            details,
        });

        (status, body).into_response()
    }
}

/// Convert sqlx errors to API errors
impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => {
                ApiError::NotFound("Resource not found".to_string())
            }
            sqlx::Error::Database(db_err) => {
                // Check for unique constraint violations
                if let Some(constraint) = db_err.constraint() {
                    if constraint.contains("email") {
                        return ApiError::Conflict("Email already exists".to_string());
                    }
                    return ApiError::Conflict(format!("Constraint violation: {}", constraint));
                }

                // Other database errors are internal
                ApiError::InternalError(format!("Database error: {}", db_err))
            }
            _ => ApiError::InternalError(format!("Database error: {}", err)),
        }
    }
}

/// Convert auth errors to API errors
impl From<axontask_shared::auth::middleware::AuthError> for ApiError {
    fn from(err: axontask_shared::auth::middleware::AuthError) -> Self {
        match err {
            axontask_shared::auth::middleware::AuthError::MissingCredentials => {
                ApiError::Unauthorized("Missing credentials".to_string())
            }
            axontask_shared::auth::middleware::AuthError::InvalidFormat(msg) => {
                ApiError::BadRequest(msg)
            }
            axontask_shared::auth::middleware::AuthError::InvalidToken(msg) => {
                ApiError::Unauthorized(msg)
            }
            axontask_shared::auth::middleware::AuthError::InvalidApiKey(msg) => {
                ApiError::Unauthorized(msg)
            }
            axontask_shared::auth::middleware::AuthError::DatabaseError(msg) => {
                ApiError::InternalError(msg)
            }
        }
    }
}

/// Convert authorization errors to API errors
impl From<axontask_shared::auth::authorization::AuthzError> for ApiError {
    fn from(err: axontask_shared::auth::authorization::AuthzError) -> Self {
        match err {
            axontask_shared::auth::authorization::AuthzError::NotMember(_) => {
                ApiError::Forbidden("Not a member of this tenant".to_string())
            }
            axontask_shared::auth::authorization::AuthzError::InsufficientRole { .. } => {
                ApiError::Forbidden("Insufficient permissions".to_string())
            }
            axontask_shared::auth::authorization::AuthzError::MissingScope(scope) => {
                ApiError::Forbidden(format!("Missing required scope: {}", scope))
            }
            axontask_shared::auth::authorization::AuthzError::NotAuthorized => {
                ApiError::Forbidden("Not authorized to access this resource".to_string())
            }
            axontask_shared::auth::authorization::AuthzError::DatabaseError(err) => {
                ApiError::InternalError(format!("Database error: {}", err))
            }
        }
    }
}

/// Convert password errors to API errors
impl From<axontask_shared::auth::password::PasswordError> for ApiError {
    fn from(err: axontask_shared::auth::password::PasswordError) -> Self {
        ApiError::InternalError(format!("Password operation failed: {}", err))
    }
}

/// Convert JWT errors to API errors
impl From<axontask_shared::auth::jwt::JwtError> for ApiError {
    fn from(err: axontask_shared::auth::jwt::JwtError) -> Self {
        match err {
            axontask_shared::auth::jwt::JwtError::Expired => {
                ApiError::Unauthorized("Token expired".to_string())
            }
            axontask_shared::auth::jwt::JwtError::InvalidIssuer { .. } => {
                ApiError::Unauthorized("Invalid token issuer".to_string())
            }
            _ => ApiError::Unauthorized(format!("Invalid token: {}", err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ApiError::BadRequest("Invalid input".to_string());
        assert_eq!(err.to_string(), "Bad request: Invalid input");

        let err = ApiError::NotFound("User not found".to_string());
        assert_eq!(err.to_string(), "Not found: User not found");
    }

    #[test]
    fn test_validation_error() {
        let errors = vec![
            ValidationErrorDetail {
                field: "email".to_string(),
                message: "Invalid email format".to_string(),
            },
            ValidationErrorDetail {
                field: "password".to_string(),
                message: "Password too short".to_string(),
            },
        ];

        let err = ApiError::ValidationError(errors);
        assert_eq!(err.to_string(), "Validation failed: 2 errors");
    }
}
