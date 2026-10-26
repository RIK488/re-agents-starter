use thiserror::Error;

/// Shared error type for all SIMORPH crates.
#[derive(Debug, Error)]
pub enum SimorphError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("unit error: {0}")]
    Unit(String),
    #[error("dimensional inconsistency: {0}")]
    Dimensional(String),
    #[error("outside validity domain: {0}")]
    ValidityDomain(String),
    #[error("numerical failure: {0}")]
    Numerical(String),
    #[error("component `{component}` failed: {message}")]
    Component { component: String, message: String },
    #[error("incompatible components: {0}")]
    Incompatible(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("protected resource modification refused: {0}")]
    Protected(String),
    #[error("timeout after {0} ms")]
    Timeout(u64),
    #[error("cancelled")]
    Cancelled,
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("reproducibility failure: {0}")]
    Reproducibility(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error("{0}")]
    Other(String),
}

impl From<serde_json::Error> for SimorphError {
    fn from(e: serde_json::Error) -> Self {
        SimorphError::Serialization(e.to_string())
    }
}

impl From<std::io::Error> for SimorphError {
    fn from(e: std::io::Error) -> Self {
        SimorphError::Io(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, SimorphError>;
