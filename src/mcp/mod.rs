//! Model Context Protocol: one server for the whole product.
//!
//! The transport is here — the socket the server listens on, the stdio shim an
//! MCP client launches, and the channel that carries a call from a socket
//! thread to the UI thread — and so are the enclave's own `enclave_` tools,
//! which answer from the daemon and need no window. A product's tools live
//! with the product (`grido_` in the `grido` crate) and are reached by plain
//! function calls. Every tool is named product_verb, so one server carries
//! them all without ambiguity.

pub mod bridge;
pub mod proto;
pub mod tools;
