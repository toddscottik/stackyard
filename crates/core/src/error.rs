use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("setting key {key:?} is not allowed; use 1-64 characters of lowercase ascii, digits, and underscores, starting with a letter")]
    InvalidSettingKey { key: String },
    #[error("setting value is {len} bytes; the limit is {limit}")]
    SettingValueTooLarge { len: usize, limit: usize },
    #[error("api version {found} is not supported; this build speaks version {supported}")]
    UnsupportedApi { found: u32, supported: u32 },
    #[error("response id {response_id:?} did not match request id {request_id:?}")]
    ResponseMismatch {
        request_id: String,
        response_id: String,
    },
    #[error("engine reply was not valid: {0}")]
    InvalidMessage(String),
    #[error("{0}")]
    OperationFailed(String),
    #[error("database schema version {found} is newer than this build ({supported})")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("engine is already running for {}", data_dir.display())]
    AlreadyRunning { data_dir: PathBuf },
    #[error("timed out waiting for the engine at {}", data_dir.display())]
    Timeout { data_dir: PathBuf },
    #[error("engine binary was not found next to this program or in LOCALDEV_ENGINE")]
    EngineBinaryMissing,
    #[error("catalog package {id} was not found")]
    PackageNotFound { id: String },
    #[error("package {id} targets {package}, this host is {host}")]
    IncompatibleTarget {
        id: String,
        package: String,
        host: String,
    },
    #[error("package {id} is already installed")]
    AlreadyInstalled { id: String },
    #[error("package {id} is already being installed")]
    InstallInProgress { id: String },
    #[error("install of {id} was cancelled")]
    Cancelled { id: String },
    #[error("digest mismatch for {id}: expected {expected}, got {actual}")]
    DigestMismatch {
        id: String,
        expected: String,
        actual: String,
    },
    #[error("archive entry {entry:?} is not a safe relative path")]
    UnsafeArchivePath { entry: String },
    #[error("package {id} cannot be removed: {reason}")]
    NotRemovable { id: String, reason: String },
    #[error("could not read version from {}: {detail}", path.display())]
    ProbeFailed { path: PathBuf, detail: String },
    #[error("{context}: {source}")]
    Io {
        context: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("{context}: {source}")]
    Database {
        context: &'static str,
        #[source]
        source: rusqlite::Error,
    },
}

impl Error {
    pub fn io(context: &'static str, source: std::io::Error) -> Self {
        Self::Io { context, source }
    }

    pub fn database(context: &'static str, source: rusqlite::Error) -> Self {
        Self::Database { context, source }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
