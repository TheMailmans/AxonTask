/// Password hashing module using Argon2id
///
/// This module provides secure password hashing using the Argon2id algorithm,
/// which is the recommended algorithm for password hashing (winner of the Password Hashing Competition).
///
/// # Security
///
/// - **Algorithm**: Argon2id (hybrid of Argon2i and Argon2d)
/// - **Memory**: 64 MB (65536 KB)
/// - **Iterations**: 3 passes
/// - **Parallelism**: 4 lanes
/// - **Output**: 32-byte hash
///
/// These parameters provide strong resistance against:
/// - Brute force attacks
/// - Dictionary attacks
/// - Rainbow table attacks
/// - Side-channel attacks
/// - GPU/ASIC attacks
///
/// # Example
///
/// ```
/// use axontask_shared::auth::password::{hash_password, verify_password};
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Hash a password
/// let password = "super_secret_password_123";
/// let hash = hash_password(password)?;
///
/// // Verify the password
/// assert!(verify_password(password, &hash)?);
///
/// // Wrong password fails
/// assert!(!verify_password("wrong_password", &hash)?);
/// # Ok(())
/// # }
/// ```

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2, ParamsBuilder, Version,
};

/// Error type for password hashing operations
#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    /// Failed to hash password
    #[error("Failed to hash password: {0}")]
    HashError(String),

    /// Failed to verify password
    #[error("Failed to verify password: {0}")]
    VerifyError(String),

    /// Invalid password hash format
    #[error("Invalid password hash format: {0}")]
    InvalidHash(String),
}

/// Hashes a password using Argon2id with secure parameters
///
/// # Security Parameters
///
/// - Memory: 64 MB (65536 KB) - Provides strong memory-hard resistance
/// - Iterations: 3 passes - Balances security and performance
/// - Parallelism: 4 lanes - Optimal for modern CPUs
/// - Salt: 16 bytes random - Generated using cryptographically secure RNG
///
/// # Arguments
///
/// * `password` - The plaintext password to hash
///
/// # Returns
///
/// PHC string format hash (includes algorithm, parameters, salt, and hash)
///
/// Example output:
/// ```text
/// $argon2id$v=19$m=65536,t=3,p=4$c2FsdHNhbHRzYWx0$hash...
/// ```
///
/// # Errors
///
/// Returns `PasswordError::HashError` if hashing fails
///
/// # Example
///
/// ```
/// use axontask_shared::auth::password::hash_password;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let hash = hash_password("my_password")?;
/// assert!(hash.starts_with("$argon2id$"));
/// # Ok(())
/// # }
/// ```
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    // Generate a random salt using OS RNG
    let salt = SaltString::generate(&mut OsRng);

    // Configure Argon2id parameters
    // - m_cost: 64 MB (65536 KB) of memory
    // - t_cost: 3 iterations
    // - p_cost: 4 parallel lanes
    let params = ParamsBuilder::new()
        .m_cost(65536) // 64 MB
        .t_cost(3)     // 3 iterations
        .p_cost(4)     // 4 parallelism
        .output_len(32) // 32-byte hash output
        .build()
        .map_err(|e| PasswordError::HashError(format!("Invalid parameters: {}", e)))?;

    // Create Argon2 instance with configured parameters
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        Version::V0x13,
        params,
    );

    // Hash the password
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| PasswordError::HashError(format!("Hash generation failed: {}", e)))?;

    Ok(password_hash.to_string())
}

/// Verifies a password against a hash
///
/// This function performs constant-time comparison to prevent timing attacks.
///
/// # Arguments
///
/// * `password` - The plaintext password to verify
/// * `hash` - The password hash (PHC string format)
///
/// # Returns
///
/// `Ok(true)` if password matches, `Ok(false)` if it doesn't match
///
/// # Errors
///
/// Returns `PasswordError::VerifyError` if verification fails due to invalid hash format
/// or other errors.
///
/// # Example
///
/// ```
/// use axontask_shared::auth::password::{hash_password, verify_password};
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let password = "correct_password";
/// let hash = hash_password(password)?;
///
/// // Correct password
/// assert!(verify_password(password, &hash)?);
///
/// // Incorrect password
/// assert!(!verify_password("wrong_password", &hash)?);
/// # Ok(())
/// # }
/// ```
pub fn verify_password(password: &str, hash: &str) -> Result<bool, PasswordError> {
    // Parse the stored hash
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| PasswordError::InvalidHash(format!("Failed to parse hash: {}", e)))?;

    // Create Argon2 instance (parameters are embedded in the hash)
    let argon2 = Argon2::default();

    // Verify password (constant-time comparison)
    match argon2.verify_password(password.as_bytes(), &parsed_hash) {
        Ok(_) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false), // Wrong password
        Err(e) => Err(PasswordError::VerifyError(format!("Verification failed: {}", e))),
    }
}

/// Validates password strength
///
/// Checks that password meets minimum security requirements:
/// - At least 8 characters long
/// - Contains at least one uppercase letter
/// - Contains at least one lowercase letter
/// - Contains at least one digit
/// - Contains at least one special character
///
/// # Arguments
///
/// * `password` - The password to validate
///
/// # Returns
///
/// `Ok(())` if password is strong enough, `Err` with description if not
///
/// # Example
///
/// ```
/// use axontask_shared::auth::password::validate_password_strength;
///
/// // Strong password
/// assert!(validate_password_strength("MyP@ssw0rd!").is_ok());
///
/// // Too short
/// assert!(validate_password_strength("Sh0rt!").is_err());
///
/// // Missing special character
/// assert!(validate_password_strength("Password123").is_err());
/// ```
pub fn validate_password_strength(password: &str) -> Result<(), String> {
    if password.len() < 8 {
        return Err("Password must be at least 8 characters long".to_string());
    }

    if !password.chars().any(|c| c.is_uppercase()) {
        return Err("Password must contain at least one uppercase letter".to_string());
    }

    if !password.chars().any(|c| c.is_lowercase()) {
        return Err("Password must contain at least one lowercase letter".to_string());
    }

    if !password.chars().any(|c| c.is_numeric()) {
        return Err("Password must contain at least one digit".to_string());
    }

    if !password.chars().any(|c| !c.is_alphanumeric()) {
        return Err("Password must contain at least one special character".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_password() {
        let password = "test_password_123";
        let hash = hash_password(password).expect("Hash should succeed");

        // Hash should start with $argon2id$
        assert!(hash.starts_with("$argon2id$"));

        // Hash should contain version
        assert!(hash.contains("v=19"));

        // Hash should contain parameters
        assert!(hash.contains("m=65536")); // 64 MB
        assert!(hash.contains("t=3"));     // 3 iterations
        assert!(hash.contains("p=4"));     // 4 parallelism
    }

    #[test]
    fn test_hash_password_produces_different_salts() {
        let password = "same_password";

        let hash1 = hash_password(password).expect("Hash 1 should succeed");
        let hash2 = hash_password(password).expect("Hash 2 should succeed");

        // Different salts = different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_verify_password_correct() {
        let password = "correct_password";
        let hash = hash_password(password).expect("Hash should succeed");

        let result = verify_password(password, &hash).expect("Verify should succeed");
        assert!(result, "Correct password should verify");
    }

    #[test]
    fn test_verify_password_incorrect() {
        let password = "correct_password";
        let hash = hash_password(password).expect("Hash should succeed");

        let result = verify_password("wrong_password", &hash).expect("Verify should succeed");
        assert!(!result, "Wrong password should not verify");
    }

    #[test]
    fn test_verify_password_empty() {
        let password = "password";
        let hash = hash_password(password).expect("Hash should succeed");

        let result = verify_password("", &hash).expect("Verify should succeed");
        assert!(!result, "Empty password should not verify");
    }

    #[test]
    fn test_verify_password_invalid_hash() {
        let result = verify_password("password", "invalid_hash");
        assert!(result.is_err(), "Invalid hash should return error");
    }

    #[test]
    fn test_verify_password_malformed_hash() {
        let result = verify_password("password", "$argon2id$invalid");
        assert!(result.is_err(), "Malformed hash should return error");
    }

    #[test]
    fn test_hash_verify_roundtrip() {
        let passwords = vec![
            "simple",
            "with spaces",
            "with-special-chars!@#$%",
            "unicode-密码-パスワード",
            "very_long_password_that_is_longer_than_usual_passwords_123456789",
        ];

        for password in passwords {
            let hash = hash_password(password).expect("Hash should succeed");
            let verified = verify_password(password, &hash).expect("Verify should succeed");
            assert!(verified, "Password '{}' should verify", password);
        }
    }

    #[test]
    fn test_validate_password_strength_valid() {
        let valid_passwords = vec![
            "MyP@ssw0rd!",
            "Str0ng!Pass",
            "C0mpl3x#Pwd",
            "S3cur3$Password",
        ];

        for password in valid_passwords {
            assert!(
                validate_password_strength(password).is_ok(),
                "Password '{}' should be valid",
                password
            );
        }
    }

    #[test]
    fn test_validate_password_strength_too_short() {
        let result = validate_password_strength("Sh0rt!");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least 8 characters"));
    }

    #[test]
    fn test_validate_password_strength_no_uppercase() {
        let result = validate_password_strength("lowercase1!");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("uppercase letter"));
    }

    #[test]
    fn test_validate_password_strength_no_lowercase() {
        let result = validate_password_strength("UPPERCASE1!");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("lowercase letter"));
    }

    #[test]
    fn test_validate_password_strength_no_digit() {
        let result = validate_password_strength("NoDigits!");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("digit"));
    }

    #[test]
    fn test_validate_password_strength_no_special() {
        let result = validate_password_strength("NoSpecial123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("special character"));
    }

    #[test]
    fn test_timing_attack_resistance() {
        // This test verifies that verification time doesn't leak information
        // about password correctness. In practice, Argon2 is designed to be
        // constant-time for verification.

        let password = "correct_password";
        let hash = hash_password(password).expect("Hash should succeed");

        // Verify with correct password
        let start = std::time::Instant::now();
        let _ = verify_password(password, &hash);
        let correct_duration = start.elapsed();

        // Verify with incorrect password of same length
        let start = std::time::Instant::now();
        let _ = verify_password("incorrect_pwd_", &hash);
        let incorrect_duration = start.elapsed();

        // Durations should be similar (within 50% variance due to system noise)
        // This is a rough check - proper timing attack resistance is built into Argon2
        let ratio = correct_duration.as_micros() as f64 / incorrect_duration.as_micros() as f64;
        assert!(
            ratio > 0.5 && ratio < 2.0,
            "Timing difference too large: correct={:?}, incorrect={:?}",
            correct_duration,
            incorrect_duration
        );
    }
}
