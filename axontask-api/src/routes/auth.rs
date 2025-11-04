/// Authentication endpoints
///
/// This module provides user authentication endpoints:
/// - Registration
/// - Login
/// - Token refresh
///
/// # Endpoints
///
/// - `POST /v1/auth/register` - Register new user
/// - `POST /v1/auth/login` - Login and get tokens
/// - `POST /v1/auth/refresh` - Refresh access token

use crate::{
    app::AppState,
    error::{ApiError, ApiResult, ValidationErrorDetail},
};
use axum::{extract::State, Json};
use axontask_shared::{
    auth::{jwt, password},
    models::{
        membership::{CreateMembership, Membership, MembershipRole},
        tenant::{CreateTenant, Tenant, TenantPlan},
        user::{CreateUser, User},
    },
};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Register request
#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    /// Email address
    #[validate(email(message = "Invalid email format"))]
    pub email: String,

    /// Password (will be validated for strength)
    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    pub password: String,

    /// Optional display name
    #[validate(length(max = 100, message = "Name must be at most 100 characters"))]
    pub name: Option<String>,

    /// Optional tenant name (if creating a new tenant)
    #[validate(length(max = 100, message = "Tenant name must be at most 100 characters"))]
    pub tenant_name: Option<String>,
}

/// Register response
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    /// User ID
    pub user_id: String,

    /// Tenant ID
    pub tenant_id: String,

    /// Access token (24h)
    pub access_token: String,

    /// Refresh token (30d)
    pub refresh_token: String,
}

/// Login request
#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    /// Email address
    #[validate(email(message = "Invalid email format"))]
    pub email: String,

    /// Password
    pub password: String,
}

/// Login response
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// User ID
    pub user_id: String,

    /// Default tenant ID
    pub tenant_id: String,

    /// Access token (24h)
    pub access_token: String,

    /// Refresh token (30d)
    pub refresh_token: String,
}

/// Refresh token request
#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    /// Refresh token
    pub refresh_token: String,
}

/// Refresh token response
#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    /// New access token (24h)
    pub access_token: String,
}

/// Register a new user (Task 2.8)
///
/// Creates a new user account with an automatically created personal tenant.
/// The user becomes the owner of the tenant.
///
/// # Endpoint
///
/// ```text
/// POST /v1/auth/register
/// Content-Type: application/json
///
/// {
///   "email": "user@example.com",
///   "password": "SecureP@ss123",
///   "name": "John Doe",
///   "tenant_name": "John's Workspace"
/// }
/// ```
///
/// # Response
///
/// ```json
/// {
///   "user_id": "uuid",
///   "tenant_id": "uuid",
///   "access_token": "eyJ...",
///   "refresh_token": "eyJ..."
/// }
/// ```
///
/// # Errors
///
/// - `400 Bad Request`: Validation failed
/// - `409 Conflict`: Email already exists
/// - `500 Internal Server Error`: Server error
pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<Json<RegisterResponse>> {
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

    // Validate password strength
    password::validate_password_strength(&req.password).map_err(|e| {
        ApiError::ValidationError(vec![ValidationErrorDetail {
            field: "password".to_string(),
            message: e,
        }])
    })?;

    // Hash password
    let password_hash = password::hash_password(&req.password)?;

    // TODO: Wrap in transaction for atomicity
    // Create user
    let user = User::create(
        &state.db,
        CreateUser {
            email: req.email.clone(),
            password_hash,
            name: req.name.clone(),
            avatar_url: None,
        },
    )
    .await?;

    // Create personal tenant
    let tenant_name = req
        .tenant_name
        .unwrap_or_else(|| format!("{}'s Workspace", req.name.as_deref().unwrap_or("User")));

    let tenant = Tenant::create(
        &state.db,
        CreateTenant {
            name: tenant_name,
            plan: TenantPlan::Trial,
        },
    )
    .await?;

    // Create membership (user as owner)
    Membership::create(
        &state.db,
        CreateMembership {
            tenant_id: tenant.id,
            user_id: user.id,
            role: MembershipRole::Owner,
        },
    )
    .await?;

    // Generate tokens
    let access_claims = jwt::Claims::new(user.id, tenant.id, jwt::TokenType::Access);
    let refresh_claims = jwt::Claims::new(user.id, tenant.id, jwt::TokenType::Refresh);

    let access_token = jwt::create_token(&access_claims, state.jwt_secret())?;
    let refresh_token = jwt::create_token(&refresh_claims, state.jwt_secret())?;

    Ok(Json(RegisterResponse {
        user_id: user.id.to_string(),
        tenant_id: tenant.id.to_string(),
        access_token,
        refresh_token,
    }))
}

/// Login endpoint (Task 2.9)
///
/// Authenticates a user and returns JWT tokens.
///
/// # Endpoint
///
/// ```text
/// POST /v1/auth/login
/// Content-Type: application/json
///
/// {
///   "email": "user@example.com",
///   "password": "SecureP@ss123"
/// }
/// ```
///
/// # Response
///
/// ```json
/// {
///   "user_id": "uuid",
///   "tenant_id": "uuid",
///   "access_token": "eyJ...",
///   "refresh_token": "eyJ..."
/// }
/// ```
///
/// # Errors
///
/// - `400 Bad Request`: Validation failed
/// - `401 Unauthorized`: Invalid credentials
/// - `500 Internal Server Error`: Server error
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<LoginResponse>> {
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

    // Find user by email
    let user = User::find_by_email(&state.db, &req.email)
        .await?
        .ok_or_else(|| ApiError::Unauthorized("Invalid email or password".to_string()))?;

    // Verify password
    let valid = password::verify_password(&req.password, &user.password_hash)?;
    if !valid {
        return Err(ApiError::Unauthorized(
            "Invalid email or password".to_string(),
        ));
    }

    // Get user's primary tenant (first membership, typically their personal tenant)
    let memberships = Membership::list_by_user(&state.db, user.id).await?;
    let tenant_id = memberships
        .first()
        .map(|m| m.tenant_id)
        .ok_or_else(|| ApiError::InternalError("User has no tenant membership".to_string()))?;

    // Update last login
    User::update_last_login(&state.db, user.id).await?;

    // Generate tokens
    let access_claims = jwt::Claims::new(user.id, tenant_id, jwt::TokenType::Access);
    let refresh_claims = jwt::Claims::new(user.id, tenant_id, jwt::TokenType::Refresh);

    let access_token = jwt::create_token(&access_claims, state.jwt_secret())?;
    let refresh_token = jwt::create_token(&refresh_claims, state.jwt_secret())?;

    Ok(Json(LoginResponse {
        user_id: user.id.to_string(),
        tenant_id: tenant_id.to_string(),
        access_token,
        refresh_token,
    }))
}

/// Token refresh endpoint (Task 2.10)
///
/// Exchanges a refresh token for a new access token.
///
/// # Endpoint
///
/// ```text
/// POST /v1/auth/refresh
/// Content-Type: application/json
///
/// {
///   "refresh_token": "eyJ..."
/// }
/// ```
///
/// # Response
///
/// ```json
/// {
///   "access_token": "eyJ..."
/// }
/// ```
///
/// # Errors
///
/// - `401 Unauthorized`: Invalid or expired refresh token
/// - `500 Internal Server Error`: Server error
pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> ApiResult<Json<RefreshResponse>> {
    // Use the refresh helper from jwt module
    let access_token = jwt::refresh_access_token(&req.refresh_token, state.jwt_secret())?;

    Ok(Json(RefreshResponse { access_token }))
}
