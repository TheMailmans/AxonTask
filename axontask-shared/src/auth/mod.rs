/// Authentication and authorization utilities
///
/// This module provides secure authentication primitives for AxonTask:
///
/// # Modules
///
/// - [`password`]: Argon2id password hashing and validation
/// - [`jwt`]: JWT token generation and validation
/// - [`api_key`]: API key generation and validation utilities
///
/// # Security Features
///
/// - **Password Hashing**: Argon2id with 64 MB memory, 3 iterations
/// - **JWT Tokens**: HS256 signing with configurable expiration
/// - **API Keys**: Secure random generation with SHA-256 hashing
/// - **Constant-time Comparison**: All verification uses constant-time operations
///
/// # Example
///
/// ```no_run
/// use axontask_shared::auth::password::{hash_password, verify_password};
/// use axontask_shared::auth::jwt::{create_token, validate_token, Claims};
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Password authentication
/// let hash = hash_password("user_password")?;
/// assert!(verify_password("user_password", &hash)?);
///
/// // JWT token generation
/// let claims = Claims::new("user-id".to_string(), "tenant-id".to_string());
/// let token = create_token(claims, "secret-key")?;
/// # Ok(())
/// # }
/// ```

pub mod password;
pub mod jwt;
pub mod api_key;
pub mod middleware;
pub mod authorization;
