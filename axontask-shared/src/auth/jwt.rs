/// JWT token generation and validation module
///
/// This module provides JWT (JSON Web Token) functionality for user authentication.
/// Tokens are signed using HS256 (HMAC-SHA256) and include claims for user/tenant identity.
///
/// # Security
///
/// - **Algorithm**: HS256 (HMAC with SHA-256)
/// - **Expiration**: Configurable (default 24 hours for access, 30 days for refresh)
/// - **Validation**: Signature, expiration, and issuer checks
/// - **Secret Management**: Secrets should be at least 32 bytes (256 bits)
///
/// # Token Types
///
/// - **Access Token**: Short-lived (24h), used for API authentication
/// - **Refresh Token**: Long-lived (30d), used to obtain new access tokens
///
/// # Example
///
/// ```
/// use axontask_shared::auth::jwt::{create_token, validate_token, Claims, TokenType};
/// use uuid::Uuid;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let user_id = Uuid::new_v4();
/// let tenant_id = Uuid::new_v4();
///
/// // Create access token
/// let claims = Claims::new(user_id, tenant_id, TokenType::Access);
/// let token = create_token(&claims, "your-secret-key")?;
///
/// // Validate token
/// let validated_claims = validate_token(&token, "your-secret-key")?;
/// assert_eq!(validated_claims.sub, user_id);
/// # Ok(())
/// # }
/// ```

use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Error type for JWT operations
#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    /// Failed to create token
    #[error("Failed to create token: {0}")]
    CreateError(String),

    /// Failed to validate token
    #[error("Failed to validate token: {0}")]
    ValidationError(String),

    /// Token has expired
    #[error("Token has expired")]
    Expired,

    /// Invalid token format
    #[error("Invalid token format: {0}")]
    InvalidFormat(String),

    /// Invalid issuer
    #[error("Invalid issuer: expected {expected}, got {actual}")]
    InvalidIssuer { expected: String, actual: String },
}

/// Token type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    /// Access token (short-lived, 24 hours)
    Access,

    /// Refresh token (long-lived, 30 days)
    Refresh,
}

impl TokenType {
    /// Gets default expiration duration for token type
    pub fn default_expiration(&self) -> Duration {
        match self {
            TokenType::Access => Duration::hours(24),
            TokenType::Refresh => Duration::days(30),
        }
    }

    /// Gets token type as string
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenType::Access => "access",
            TokenType::Refresh => "refresh",
        }
    }
}

/// JWT claims structure
///
/// Contains standard JWT claims plus AxonTask-specific claims.
///
/// # Standard Claims
///
/// - `sub`: Subject (user ID)
/// - `iss`: Issuer (always "axontask")
/// - `iat`: Issued at timestamp
/// - `exp`: Expiration timestamp
/// - `nbf`: Not before timestamp
///
/// # Custom Claims
///
/// - `tenant_id`: Current tenant context
/// - `token_type`: Access or refresh token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject - User ID
    pub sub: Uuid,

    /// Issuer - Always "axontask"
    pub iss: String,

    /// Issued at (Unix timestamp)
    pub iat: i64,

    /// Expiration time (Unix timestamp)
    pub exp: i64,

    /// Not before (Unix timestamp)
    pub nbf: i64,

    /// Tenant ID (custom claim)
    pub tenant_id: Uuid,

    /// Token type (custom claim)
    pub token_type: TokenType,
}

impl Claims {
    /// Creates new claims with default expiration
    ///
    /// # Arguments
    ///
    /// * `user_id` - User ID (subject)
    /// * `tenant_id` - Tenant ID for multi-tenancy context
    /// * `token_type` - Access or refresh token
    ///
    /// # Returns
    ///
    /// Claims with default expiration based on token type
    ///
    /// # Example
    ///
    /// ```
    /// use axontask_shared::auth::jwt::{Claims, TokenType};
    /// use uuid::Uuid;
    ///
    /// let claims = Claims::new(
    ///     Uuid::new_v4(),
    ///     Uuid::new_v4(),
    ///     TokenType::Access,
    /// );
    /// ```
    pub fn new(user_id: Uuid, tenant_id: Uuid, token_type: TokenType) -> Self {
        let now = Utc::now();
        let expiration = now + token_type.default_expiration();

        Self {
            sub: user_id,
            iss: "axontask".to_string(),
            iat: now.timestamp(),
            exp: expiration.timestamp(),
            nbf: now.timestamp(),
            tenant_id,
            token_type,
        }
    }

    /// Creates claims with custom expiration
    ///
    /// # Arguments
    ///
    /// * `user_id` - User ID
    /// * `tenant_id` - Tenant ID
    /// * `token_type` - Token type
    /// * `expires_in` - Custom expiration duration
    ///
    /// # Example
    ///
    /// ```
    /// use axontask_shared::auth::jwt::{Claims, TokenType};
    /// use chrono::Duration;
    /// use uuid::Uuid;
    ///
    /// let claims = Claims::with_expiration(
    ///     Uuid::new_v4(),
    ///     Uuid::new_v4(),
    ///     TokenType::Access,
    ///     Duration::hours(1), // 1 hour expiration
    /// );
    /// ```
    pub fn with_expiration(
        user_id: Uuid,
        tenant_id: Uuid,
        token_type: TokenType,
        expires_in: Duration,
    ) -> Self {
        let now = Utc::now();
        let expiration = now + expires_in;

        Self {
            sub: user_id,
            iss: "axontask".to_string(),
            iat: now.timestamp(),
            exp: expiration.timestamp(),
            nbf: now.timestamp(),
            tenant_id,
            token_type,
        }
    }

    /// Checks if token has expired
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() >= self.exp
    }

    /// Gets time until expiration
    pub fn time_until_expiration(&self) -> Option<Duration> {
        let now = Utc::now().timestamp();
        if self.exp > now {
            Some(Duration::seconds(self.exp - now))
        } else {
            None
        }
    }
}

/// Creates a JWT token from claims
///
/// Signs the token using HS256 (HMAC-SHA256) with the provided secret.
///
/// # Arguments
///
/// * `claims` - Token claims
/// * `secret` - Secret key for signing (should be at least 32 bytes)
///
/// # Returns
///
/// Base64-encoded JWT token string
///
/// # Errors
///
/// Returns `JwtError::CreateError` if token creation fails
///
/// # Security
///
/// The secret should be:
/// - At least 32 bytes (256 bits) for HS256
/// - Randomly generated
/// - Stored securely (environment variable or secret manager)
/// - Rotated periodically
///
/// # Example
///
/// ```
/// use axontask_shared::auth::jwt::{create_token, Claims, TokenType};
/// use uuid::Uuid;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let claims = Claims::new(
///     Uuid::new_v4(),
///     Uuid::new_v4(),
///     TokenType::Access,
/// );
///
/// let token = create_token(&claims, "your-secret-key-at-least-32-bytes")?;
/// assert!(!token.is_empty());
/// # Ok(())
/// # }
/// ```
pub fn create_token(claims: &Claims, secret: &str) -> Result<String, JwtError> {
    let header = Header::new(Algorithm::HS256);
    let key = EncodingKey::from_secret(secret.as_bytes());

    encode(&header, claims, &key)
        .map_err(|e| JwtError::CreateError(format!("Token encoding failed: {}", e)))
}

/// Validates a JWT token and extracts claims
///
/// Verifies:
/// - Signature is valid
/// - Token hasn't expired
/// - Issuer is "axontask"
/// - Token is not used before nbf time
///
/// # Arguments
///
/// * `token` - JWT token string
/// * `secret` - Secret key used for signing
///
/// # Returns
///
/// Validated claims if token is valid
///
/// # Errors
///
/// Returns error if:
/// - Signature is invalid
/// - Token has expired
/// - Issuer doesn't match
/// - Token format is invalid
///
/// # Example
///
/// ```
/// use axontask_shared::auth::jwt::{create_token, validate_token, Claims, TokenType};
/// use uuid::Uuid;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let user_id = Uuid::new_v4();
/// let tenant_id = Uuid::new_v4();
/// let secret = "your-secret-key-at-least-32-bytes";
///
/// // Create token
/// let claims = Claims::new(user_id, tenant_id, TokenType::Access);
/// let token = create_token(&claims, secret)?;
///
/// // Validate token
/// let validated = validate_token(&token, secret)?;
/// assert_eq!(validated.sub, user_id);
/// assert_eq!(validated.tenant_id, tenant_id);
/// # Ok(())
/// # }
/// ```
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, JwtError> {
    let key = DecodingKey::from_secret(secret.as_bytes());

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&["axontask"]);
    validation.validate_exp = true;
    validation.validate_nbf = true;

    let token_data = decode::<Claims>(token, &key, &validation).map_err(|e| {
        match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => JwtError::Expired,
            jsonwebtoken::errors::ErrorKind::InvalidIssuer => JwtError::InvalidIssuer {
                expected: "axontask".to_string(),
                actual: "unknown".to_string(),
            },
            _ => JwtError::ValidationError(format!("Token validation failed: {}", e)),
        }
    })?;

    Ok(token_data.claims)
}

/// Validates token and checks it's an access token
///
/// Convenience wrapper around `validate_token` that also ensures
/// the token type is `Access`.
///
/// # Example
///
/// ```
/// use axontask_shared::auth::jwt::{create_token, validate_access_token, Claims, TokenType};
/// use uuid::Uuid;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let claims = Claims::new(Uuid::new_v4(), Uuid::new_v4(), TokenType::Access);
/// let token = create_token(&claims, "secret")?;
///
/// let validated = validate_access_token(&token, "secret")?;
/// assert_eq!(validated.token_type, TokenType::Access);
/// # Ok(())
/// # }
/// ```
pub fn validate_access_token(token: &str, secret: &str) -> Result<Claims, JwtError> {
    let claims = validate_token(token, secret)?;

    if claims.token_type != TokenType::Access {
        return Err(JwtError::ValidationError(
            "Expected access token, got refresh token".to_string(),
        ));
    }

    Ok(claims)
}

/// Validates token and checks it's a refresh token
///
/// Convenience wrapper around `validate_token` that also ensures
/// the token type is `Refresh`.
///
/// # Example
///
/// ```
/// use axontask_shared::auth::jwt::{create_token, validate_refresh_token, Claims, TokenType};
/// use uuid::Uuid;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let claims = Claims::new(Uuid::new_v4(), Uuid::new_v4(), TokenType::Refresh);
/// let token = create_token(&claims, "secret")?;
///
/// let validated = validate_refresh_token(&token, "secret")?;
/// assert_eq!(validated.token_type, TokenType::Refresh);
/// # Ok(())
/// # }
/// ```
pub fn validate_refresh_token(token: &str, secret: &str) -> Result<Claims, JwtError> {
    let claims = validate_token(token, secret)?;

    if claims.token_type != TokenType::Refresh {
        return Err(JwtError::ValidationError(
            "Expected refresh token, got access token".to_string(),
        ));
    }

    Ok(claims)
}

/// Refreshes an access token using a refresh token
///
/// Takes a valid refresh token and generates a new access token
/// with the same user/tenant context.
///
/// # Arguments
///
/// * `refresh_token` - Valid refresh token
/// * `secret` - Secret key for signing
///
/// # Returns
///
/// New access token string
///
/// # Errors
///
/// Returns error if refresh token is invalid or expired
///
/// # Example
///
/// ```
/// use axontask_shared::auth::jwt::{create_token, refresh_access_token, Claims, TokenType};
/// use uuid::Uuid;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let user_id = Uuid::new_v4();
/// let tenant_id = Uuid::new_v4();
/// let secret = "secret";
///
/// // Create refresh token
/// let refresh_claims = Claims::new(user_id, tenant_id, TokenType::Refresh);
/// let refresh_token = create_token(&refresh_claims, secret)?;
///
/// // Get new access token
/// let new_access_token = refresh_access_token(&refresh_token, secret)?;
/// assert!(!new_access_token.is_empty());
/// # Ok(())
/// # }
/// ```
pub fn refresh_access_token(refresh_token: &str, secret: &str) -> Result<String, JwtError> {
    // Validate refresh token
    let refresh_claims = validate_refresh_token(refresh_token, secret)?;

    // Create new access token with same user/tenant
    let access_claims = Claims::new(
        refresh_claims.sub,
        refresh_claims.tenant_id,
        TokenType::Access,
    );

    create_token(&access_claims, secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_type_expiration() {
        assert_eq!(TokenType::Access.default_expiration(), Duration::hours(24));
        assert_eq!(TokenType::Refresh.default_expiration(), Duration::days(30));
    }

    #[test]
    fn test_claims_creation() {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let claims = Claims::new(user_id, tenant_id, TokenType::Access);

        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.tenant_id, tenant_id);
        assert_eq!(claims.iss, "axontask");
        assert_eq!(claims.token_type, TokenType::Access);
        assert!(!claims.is_expired());
    }

    #[test]
    fn test_claims_with_custom_expiration() {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let claims = Claims::with_expiration(
            user_id,
            tenant_id,
            TokenType::Access,
            Duration::hours(1),
        );

        let time_left = claims.time_until_expiration().unwrap();
        assert!(time_left.num_seconds() > 3500); // ~1 hour minus a bit
        assert!(time_left.num_seconds() <= 3600); // <= 1 hour
    }

    #[test]
    fn test_create_and_validate_token() {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let secret = "test-secret-key-at-least-32-bytes-long";

        let claims = Claims::new(user_id, tenant_id, TokenType::Access);
        let token = create_token(&claims, secret).expect("Should create token");

        let validated = validate_token(&token, secret).expect("Should validate token");
        assert_eq!(validated.sub, user_id);
        assert_eq!(validated.tenant_id, tenant_id);
        assert_eq!(validated.token_type, TokenType::Access);
        assert_eq!(validated.iss, "axontask");
    }

    #[test]
    fn test_validate_with_wrong_secret() {
        let claims = Claims::new(Uuid::new_v4(), Uuid::new_v4(), TokenType::Access);
        let token = create_token(&claims, "secret1").expect("Should create token");

        let result = validate_token(&token, "wrong-secret");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_expired_token() {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let secret = "test-secret";

        // Create token that expired 1 hour ago
        let claims = Claims::with_expiration(
            user_id,
            tenant_id,
            TokenType::Access,
            Duration::seconds(-3600), // Negative duration = already expired
        );

        assert!(claims.is_expired());
        assert!(claims.time_until_expiration().is_none());

        let token = create_token(&claims, secret).expect("Should create token");
        let result = validate_token(&token, secret);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), JwtError::Expired));
    }

    #[test]
    fn test_validate_access_token() {
        let secret = "secret";

        // Access token should validate
        let access_claims = Claims::new(Uuid::new_v4(), Uuid::new_v4(), TokenType::Access);
        let access_token = create_token(&access_claims, secret).unwrap();
        assert!(validate_access_token(&access_token, secret).is_ok());

        // Refresh token should fail
        let refresh_claims = Claims::new(Uuid::new_v4(), Uuid::new_v4(), TokenType::Refresh);
        let refresh_token = create_token(&refresh_claims, secret).unwrap();
        assert!(validate_access_token(&refresh_token, secret).is_err());
    }

    #[test]
    fn test_validate_refresh_token() {
        let secret = "secret";

        // Refresh token should validate
        let refresh_claims = Claims::new(Uuid::new_v4(), Uuid::new_v4(), TokenType::Refresh);
        let refresh_token = create_token(&refresh_claims, secret).unwrap();
        assert!(validate_refresh_token(&refresh_token, secret).is_ok());

        // Access token should fail
        let access_claims = Claims::new(Uuid::new_v4(), Uuid::new_v4(), TokenType::Access);
        let access_token = create_token(&access_claims, secret).unwrap();
        assert!(validate_refresh_token(&access_token, secret).is_err());
    }

    #[test]
    fn test_refresh_access_token() {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let secret = "secret";

        // Create refresh token
        let refresh_claims = Claims::new(user_id, tenant_id, TokenType::Refresh);
        let refresh_token = create_token(&refresh_claims, secret).unwrap();

        // Get new access token
        let new_access_token = refresh_access_token(&refresh_token, secret).unwrap();

        // Validate new access token
        let validated = validate_access_token(&new_access_token, secret).unwrap();
        assert_eq!(validated.sub, user_id);
        assert_eq!(validated.tenant_id, tenant_id);
        assert_eq!(validated.token_type, TokenType::Access);
    }

    #[test]
    fn test_refresh_with_access_token_fails() {
        let secret = "secret";

        // Try to refresh using an access token (should fail)
        let access_claims = Claims::new(Uuid::new_v4(), Uuid::new_v4(), TokenType::Access);
        let access_token = create_token(&access_claims, secret).unwrap();

        let result = refresh_access_token(&access_token, secret);
        assert!(result.is_err());
    }

    #[test]
    fn test_token_roundtrip() {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let secret = "my-secret-key-for-testing-purposes";

        // Create access token
        let access_claims = Claims::new(user_id, tenant_id, TokenType::Access);
        let access_token = create_token(&access_claims, secret).unwrap();
        let validated_access = validate_access_token(&access_token, secret).unwrap();
        assert_eq!(validated_access.sub, user_id);
        assert_eq!(validated_access.tenant_id, tenant_id);

        // Create refresh token
        let refresh_claims = Claims::new(user_id, tenant_id, TokenType::Refresh);
        let refresh_token = create_token(&refresh_claims, secret).unwrap();
        let validated_refresh = validate_refresh_token(&refresh_token, secret).unwrap();
        assert_eq!(validated_refresh.sub, user_id);
        assert_eq!(validated_refresh.tenant_id, tenant_id);

        // Refresh to get new access token
        let new_access = refresh_access_token(&refresh_token, secret).unwrap();
        let validated_new = validate_access_token(&new_access, secret).unwrap();
        assert_eq!(validated_new.sub, user_id);
        assert_eq!(validated_new.tenant_id, tenant_id);
    }
}
