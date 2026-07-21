//! Public API surface for the asynchronous worker RPC framework.
//!
//! This module re-exports the client-facing types and the macro-based API
//! declaration machinery. See sub-modules for details:
//!
//! * [`api_client`] — [`ApiClient`] for sending typed requests to a worker.
//! * [`api_method_macros`] — Traits and macros for declaring API groups and methods.
//! * [`api_worker_declarations`] — Example worker API declarations.

/// Typed client for sending RPC requests and awaiting worker responses.
pub mod api_client;
/// Traits and macros for declaring API groups, methods, and implementations.
pub mod api_method_macros;
/// Built-in worker API group declarations used by the framework.
pub mod api_worker_declarations;
