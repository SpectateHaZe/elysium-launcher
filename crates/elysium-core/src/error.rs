use std::path::PathBuf;

/// Errors produced by the core library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid JSON in {context}: {source}")]
    Json {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("checksum mismatch for {url}: expected {expected}, got {actual}")]
    Checksum {
        url: String,
        expected: String,
        actual: String,
    },

    #[error("failed to extract {path}: {source}")]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },

    #[error("unknown Minecraft version '{0}'")]
    UnknownVersion(String),

    #[error(
        "no Java runtime '{component}' is available for {platform}; set a custom Java path in settings"
    )]
    JavaUnavailable { component: String, platform: String },

    #[error("instance '{0}' not found")]
    InstanceNotFound(String),

    #[error("{0}")]
    Invalid(String),

    #[error("operation cancelled")]
    Cancelled,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Adds path context to `std::io::Error`s.
pub trait IoContext<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T> {
        self.map_err(|source| Error::Io {
            path: path.into(),
            source,
        })
    }
}
