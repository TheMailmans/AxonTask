/// API route handlers
///
/// This module contains all route handlers organized by resource:
///
/// - `health`: Health check endpoint
/// - `auth`: Authentication endpoints (register, login, refresh)
/// - `api_keys`: API key management endpoints

pub mod health;
pub mod auth;
pub mod api_keys;
