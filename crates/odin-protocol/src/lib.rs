#![forbid(unsafe_code)]
//! Subprocess protocol adapters. The process configuration is operator-owned;
//! requests cannot supply executable names or command-line arguments.
pub mod dap;
pub mod lsp;
pub mod rpc;
pub mod text;
mod workspace;
