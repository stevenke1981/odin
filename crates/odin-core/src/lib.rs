#![forbid(unsafe_code)]
//! Odin's transport-independent storage and filesystem domain.
//! No LLM API, telemetry upload, arbitrary evaluation, or network client lives here.
pub mod config;
pub mod error;
pub mod execution;
pub mod languages;
pub mod memory;
pub mod paths;
pub mod process;
pub mod sandbox;
pub mod sandbox_policy;
pub mod search;
pub mod store;
pub mod transaction;

pub use error::{Fault, Result};

pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
pub fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
