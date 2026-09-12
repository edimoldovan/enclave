//! Enclave — one app for a company's private network.
//!
//! The binary is a thin host around three roles, all the same executable:
//!
//! - `enclave mcp` — the stdio shim an assistant launches. Windowless.
//! - `enclave --serve` — the server: it owns the socket, the registry of which
//!   process shows which product, and the confirmation window. One per user.
//! - `enclave [file]`, `enclave --product <name> [file]` — a window. It claims
//!   the role for its product, or hands its file to the window that has it.
//!
//! The UIs live in their own crates (`grido` is the first); what lives here is
//! the plumbing they share.

pub mod confirm;
pub mod host;
pub mod ipc;
pub mod mcp;
pub mod role;
pub mod serve;
