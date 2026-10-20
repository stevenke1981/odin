use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
#[error("{code}: {message}")]
pub struct Fault {
    pub code: String,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, Fault>;
impl Fault {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID_ARGUMENT", message)
    }
    pub fn denied(message: impl Into<String>) -> Self {
        Self::new("PERMISSION_DENIED", message)
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new("CONFLICT", message)
    }
}
impl From<std::io::Error> for Fault {
    fn from(e: std::io::Error) -> Self {
        Self::new("IO_ERROR", e.to_string())
    }
}
impl From<serde_json::Error> for Fault {
    fn from(e: serde_json::Error) -> Self {
        Self::new("JSON_ERROR", e.to_string())
    }
}
impl From<rusqlite::Error> for Fault {
    fn from(e: rusqlite::Error) -> Self {
        Self::new("STORE_ERROR", e.to_string())
    }
}
impl From<regex::Error> for Fault {
    fn from(e: regex::Error) -> Self {
        Self::invalid(e.to_string())
    }
}
