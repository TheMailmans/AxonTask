//! # AxonTask Shared Library
//!
//! This crate contains shared types, utilities, and business logic used across
//! the AxonTask API server and worker systems.
//!
//! ## Module Organization
//!
//! - `models`: Database models and data structures
//! - `auth`: Authentication and authorization utilities
//! - `redis`: Redis client and stream utilities
//! - `integrity`: Hash chain and receipt generation
//! - `config`: Configuration management
//! - `error`: Common error types

// Public modules (to be implemented in phases)
pub mod auth; // Phase 2: Authentication System
// pub mod config;
pub mod db; // Phase 1: Core Data Layer
// pub mod error;
// pub mod integrity;
pub mod models; // Phase 1: Database models
// pub mod redis;

/// Current version of the AxonTask shared library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_set() {
        assert!(!VERSION.is_empty());
    }
}
