/// Configuration management for the API server
///
/// This module loads configuration from environment variables and provides
/// a type-safe configuration struct.
///
/// # Environment Variables
///
/// - `DATABASE_URL`: PostgreSQL connection string
/// - `API_HOST`: Host to bind to (default: 0.0.0.0)
/// - `API_PORT`: Port to bind to (default: 8080)
/// - `JWT_SECRET`: Secret key for JWT signing (required)
/// - `RUST_LOG`: Log level (default: info)
///
/// # Example
///
/// ```no_run
/// use axontask_api::config::Config;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = Config::from_env()?;
/// println!("Server will listen on {}:{}", config.api.host, config.api.port);
/// # Ok(())
/// # }
/// ```

use serde::{Deserialize, Serialize};
use std::env;

/// Complete application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// API server configuration
    pub api: ApiConfig,

    /// Database configuration
    pub database: DatabaseConfig,

    /// JWT configuration
    pub jwt: JwtConfig,
}

/// API server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Host to bind to
    pub host: String,

    /// Port to bind to
    pub port: u16,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// PostgreSQL connection URL
    pub url: String,

    /// Maximum number of connections in pool
    pub max_connections: u32,
}

/// JWT configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    /// Secret key for JWT signing
    ///
    /// IMPORTANT: This must be kept secret and should be at least 32 bytes.
    /// Generate with: `openssl rand -hex 32`
    pub secret: String,
}

impl Config {
    /// Loads configuration from environment variables
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Required environment variables are missing
    /// - Environment variables have invalid values
    ///
    /// # Example
    ///
    /// ```no_run
    /// use axontask_api::config::Config;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let config = Config::from_env()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_env() -> anyhow::Result<Self> {
        // Load .env file if present (for development)
        dotenvy::dotenv().ok();

        let api_host = env::var("API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let api_port = env::var("API_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()?;

        let database_url = env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable is required"))?;

        let max_connections = env::var("DATABASE_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<u32>()?;

        let jwt_secret = env::var("JWT_SECRET")
            .map_err(|_| anyhow::anyhow!("JWT_SECRET environment variable is required"))?;

        if jwt_secret.len() < 32 {
            anyhow::bail!("JWT_SECRET must be at least 32 characters long");
        }

        Ok(Self {
            api: ApiConfig {
                host: api_host,
                port: api_port,
            },
            database: DatabaseConfig {
                url: database_url,
                max_connections,
            },
            jwt: JwtConfig {
                secret: jwt_secret,
            },
        })
    }

    /// Returns the server bind address
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.api.host, self.api.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_address() {
        let config = Config {
            api: ApiConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
            },
            database: DatabaseConfig {
                url: "postgresql://localhost/test".to_string(),
                max_connections: 10,
            },
            jwt: JwtConfig {
                secret: "test-secret-key-at-least-32-bytes-long".to_string(),
            },
        };

        assert_eq!(config.bind_address(), "127.0.0.1:8080");
    }
}
