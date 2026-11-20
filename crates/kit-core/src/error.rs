use std::path::PathBuf;

/// Errors produced by the core engine. Every variant carries enough context
/// to be shown to the user directly.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("serialisation error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("not enough disk space: {needed} bytes needed, {available} bytes available")]
    InsufficientSpace { needed: u64, available: u64 },
    #[error("operation cancelled")]
    Cancelled,
    #[error("audio error: {0}")]
    Audio(String),
    #[error("{0}")]
    Other(String),
}

impl CoreError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        CoreError::Io { path: path.into(), source }
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

/// Extension trait for attaching a path to `std::io::Result`.
pub trait IoContext<T> {
    fn at(self, path: &std::path::Path) -> CoreResult<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn at(self, path: &std::path::Path) -> CoreResult<T> {
        self.map_err(|e| CoreError::io(path, e))
    }
}
