use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("migration failed: expected schema version {expected}, found {found}")]
    Migration { expected: i64, found: i64 },

    #[error("path {path} escapes repository root")]
    PathEscapesRoot { path: PathBuf },

    #[error("repository root {path} does not exist or is not a directory")]
    InvalidRepoRoot { path: PathBuf },
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
