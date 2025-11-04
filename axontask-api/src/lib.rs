///! # AxonTask API Server Library
//!
//! This library provides the core functionality for the AxonTask API server.
//!
//! ## Modules
//!
//! - `app`: Application state and router builder
//! - `config`: Configuration management
//! - `error`: Error handling and HTTP response mapping
//! - `routes`: API route handlers

pub mod app;
pub mod config;
pub mod error;
pub mod middleware;
pub mod routes;
