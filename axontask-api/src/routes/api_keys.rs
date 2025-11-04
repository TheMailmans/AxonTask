/// API key management endpoints
///
/// This module provides CRUD endpoints for API key management.
/// All endpoints require JWT authentication.
///
/// # Endpoints
///
/// - `POST /v1/api-keys` - Create API key
/// - `GET /v1/api-keys` - List API keys
/// - `POST /v1/api-keys/:id` - Revoke API key

use crate::{
    app::AppState,
    error::{ApiError, ApiResult, ValidationErrorDetail},
};
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use axontask_shared::{
    auth::{api_key as api_key_util, middleware::AuthContext},
    models::api_key::{ApiKey, CreateApiKey},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

/// Create API key request
#[derive(Debug, Deserialize, Validate)]
pub struct CreateApiKeyRequest {
    /// API key name/description
    #[validate(length(min = 1, max = 100, message = "Name must be 1-100 characters"))]
    pub name: String,

    /// Comma-separated scopes (e.g., "tasks:read,tasks:write")
    ///
    /// Available scopes:
    /// - `*`: All permissions
    /// - `tasks:*`: All task permissions
    /// - `tasks:read`: Read tasks
    /// - `tasks:write`: Create/update tasks
    /// - `tasks:delete`: Delete tasks
    /// - `webhooks:*`: All webhook permissions
    /// - `webhooks:manage`: Manage webhooks
    #[validate(length(min = 1, message = "At least one scope is required"))]
    pub scopes: String,

    /// Optional expiration date (ISO 8601)
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Create API key response
#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    /// API key ID
    pub id: String,

    /// The plaintext API key (ONLY returned on creation)
    ///
    /// IMPORTANT: This is the only time the plaintext key is shown.
    /// Store it securely as it cannot be retrieved later.
    pub key: String,

    /// API key name
    pub name: String,

    /// Scopes
    pub scopes: Vec<String>,

    /// Created at
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Expires at
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// API key list item (masked)
#[derive(Debug, Serialize)]
pub struct ApiKeyListItem {
    /// API key ID
    pub id: String,

    /// API key name
    pub name: String,

    /// Key prefix (e.g., "axon_abc...")
    pub key_prefix: String,

    /// Scopes
    pub scopes: Vec<String>,

    /// Whether key is revoked
    pub revoked: bool,

    /// Created at
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Last used at
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Expires at
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// List API keys response
#[derive(Debug, Serialize)]
pub struct ListApiKeysResponse {
    /// API keys
    pub keys: Vec<ApiKeyListItem>,
}

/// Revoke API key response
#[derive(Debug, Serialize)]
pub struct RevokeApiKeyResponse {
    /// Whether key was revoked
    pub revoked: bool,
}

/// Create API key (Task 2.11)
///
/// Creates a new API key for the authenticated user's tenant.
/// Returns the plaintext key ONLY on creation.
///
/// # Endpoint
///
/// ```text
/// POST /v1/api-keys
/// Authorization: Bearer <jwt_token>
/// Content-Type: application/json
///
/// {
///   "name": "Production Server",
///   "scopes": "tasks:read,tasks:write",
///   "expires_at": "2026-01-01T00:00:00Z"
/// }
/// ```
///
/// # Response
///
/// ```json
/// {
///   "id": "uuid",
///   "key": "axon_abcdef123456...",
///   "name": "Production Server",
///   "scopes": ["tasks:read", "tasks:write"],
///   "created_at": "2025-01-03T12:00:00Z",
///   "expires_at": "2026-01-01T00:00:00Z"
/// }
/// ```
///
/// # Errors
///
/// - `400 Bad Request`: Validation failed
/// - `401 Unauthorized`: Missing or invalid JWT token
/// - `500 Internal Server Error`: Server error
pub async fn create_api_key(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(req): Json<CreateApiKeyRequest>,
) -> ApiResult<Json<CreateApiKeyResponse>> {
    // Validate request
    req.validate().map_err(|e| {
        let errors: Vec<ValidationErrorDetail> = e
            .field_errors()
            .iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |error| ValidationErrorDetail {
                    field: field.to_string(),
                    message: error
                        .message
                        .as_ref()
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "Validation failed".to_string()),
                })
            })
            .collect();
        ApiError::ValidationError(errors)
    })?;

    // Parse scopes
    let scopes = api_key_util::parse_scopes(&req.scopes);
    if scopes.is_empty() {
        return Err(ApiError::ValidationError(vec![ValidationErrorDetail {
            field: "scopes".to_string(),
            message: "At least one scope is required".to_string(),
        }]));
    }

    // Create API key in database (generates key internally)
    let (api_key, plaintext_key) = ApiKey::create(
        &state.db,
        CreateApiKey {
            tenant_id: auth.tenant_id,
            name: req.name.clone(),
            scopes,
            expires_at: req.expires_at,
        },
    )
    .await?;

    Ok(Json(CreateApiKeyResponse {
        id: api_key.id.to_string(),
        key: plaintext_key,
        name: api_key.name,
        scopes: api_key.scopes,
        created_at: api_key.created_at,
        expires_at: api_key.expires_at,
    }))
}

/// List API keys (Task 2.11)
///
/// Lists all API keys for the authenticated user's tenant.
/// Keys are masked (only prefix shown).
///
/// # Endpoint
///
/// ```text
/// GET /v1/api-keys
/// Authorization: Bearer <jwt_token>
/// ```
///
/// # Response
///
/// ```json
/// {
///   "keys": [
///     {
///       "id": "uuid",
///       "name": "Production Server",
///       "key_prefix": "axon_abc...",
///       "scopes": ["tasks:read", "tasks:write"],
///       "revoked": false,
///       "created_at": "2025-01-03T12:00:00Z",
///       "last_used_at": "2025-01-03T14:30:00Z",
///       "expires_at": "2026-01-01T00:00:00Z"
///     }
///   ]
/// }
/// ```
///
/// # Errors
///
/// - `401 Unauthorized`: Missing or invalid JWT token
/// - `500 Internal Server Error`: Server error
pub async fn list_api_keys(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<ListApiKeysResponse>> {
    // List API keys for tenant
    let api_keys = ApiKey::list_by_tenant(&state.db, auth.tenant_id).await?;

    let keys = api_keys
        .into_iter()
        .map(|key| ApiKeyListItem {
            id: key.id.to_string(),
            name: key.name,
            key_prefix: key.key_prefix,
            scopes: key.scopes,
            revoked: key.revoked,
            created_at: key.created_at,
            last_used_at: key.last_used_at,
            expires_at: key.expires_at,
        })
        .collect();

    Ok(Json(ListApiKeysResponse { keys }))
}

/// Revoke API key (Task 2.11)
///
/// Revokes an API key, preventing it from being used for authentication.
///
/// # Endpoint
///
/// ```text
/// POST /v1/api-keys/:id
/// Authorization: Bearer <jwt_token>
/// ```
///
/// # Response
///
/// ```json
/// {
///   "revoked": true
/// }
/// ```
///
/// # Errors
///
/// - `401 Unauthorized`: Missing or invalid JWT token
/// - `404 Not Found`: API key not found
/// - `500 Internal Server Error`: Server error
pub async fn revoke_api_key(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RevokeApiKeyResponse>> {
    // Revoke API key (with tenant isolation)
    let revoked = ApiKey::revoke_with_tenant(&state.db, id, auth.tenant_id).await?;

    if !revoked {
        return Err(ApiError::NotFound("API key not found".to_string()));
    }

    Ok(Json(RevokeApiKeyResponse { revoked }))
}
