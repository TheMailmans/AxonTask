///! # AxonTask Worker Library
///!
///! This library provides the core worker functionality for executing tasks
///! via adapters and managing the task lifecycle.
///!
///! ## Modules
///!
///! - `adapters`: Task execution adapters (shell, docker, fly, mock)
///! - `orchestrator`: Worker orchestration and task dispatch
///! - `queue`: Task queue reader
///! - `events`: Event emission to Redis Streams
///!
///! ## Example
///!
///! ```no_run
///! use axontask_worker::adapters::{MockAdapter, Adapter};
///!
///! # async fn example() {
///! let adapter = MockAdapter::new();
///! println!("Adapter: {}", adapter.name());
///! # }
///! ```

pub mod adapters;
// pub mod control;
pub mod events;
// pub mod metrics;
pub mod orchestrator;
pub mod queue;
// pub mod shutdown;
// pub mod timeout;
