//! Asynchronous RPC-style worker API framework for Crack projects.
//!
//! This crate provides a flexible framework for defining and invoking
//! asynchronous API methods across process or thread boundaries. It is designed
//! for use in both native and WebAssembly targets.
//!
//! # Architecture
//!
//! * **API definitions** — Define method signatures using the [`api::api_method_macros`]
//!   traits and macros. Methods are grouped into *API groups*.
//! * **Client** — [`api::ApiClient`] sends typed requests to a worker and awaits
//!   typed responses. It handles request/response correlation transparently.
//! * **Worker** — [`crack_worker`] runs an event loop that receives requests,
//!   dispatches them to registered implementations, and sends back responses.
//! * **Transport** — [`crack_worker::WorkerPipe`] provides the underlying
//!   message-passing channel (MPSC) between client and worker.
//!
//! # Feature highlights
//!
//! * Cross-platform: works on native targets and WebAssembly (via `n0_future`).
//! * Type-safe: arguments and return values are serialized with `postcard`
//!   and type-checked via traits.
//! * Async-first: all I/O uses `async`/`await` via `n0_future` task abstraction.
//!
//! # Example
//!
//! ```ignore
//! // Define an API group and method
//! declare_api_group2! {
//!     MyApiGroup,
//!     [
//!         (MyMethod, MyArg, MyRet),
//!     ]
//! }
//!
//! // Client side
//! let client = ApiClient::new(pipe);
//! let result = client.call::<MyMethod>(MyArg { ... }).await?;
//! ```
//!
//! See the [`api`] and [`crack_worker`] modules for detailed usage.

/// Client-facing RPC API types, traits, and declaration macros.
pub mod api;
/// Worker transport, dispatch mapping, and worker-loader abstractions.
pub mod crack_worker;

/// Re-export of cross-platform async utilities.
pub use _crack_utils;
/// Re-export of error handling crate.
pub use anyhow;
/// Re-export of async trait support.
pub use async_trait;
/// Re-export of futures utilities.
pub use futures;
/// Re-export of `paste` macro for macro-based API declarations.
pub use paste;
