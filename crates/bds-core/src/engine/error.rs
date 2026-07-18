/// Errors produced by engine operations.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("database error: {0}")]
    Db(#[from] diesel::result::Error),
    #[error("database connection error: {0}")]
    DbConnection(#[from] diesel::ConnectionError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("validation error: {0}")]
    Validation(String),
}

impl From<crate::db::DatabaseError> for EngineError {
    fn from(e: crate::db::DatabaseError) -> Self {
        match e {
            crate::db::DatabaseError::Connection(e) => Self::DbConnection(e),
            crate::db::DatabaseError::Query(e) => Self::Db(e),
        }
    }
}

impl From<reqwest::Error> for EngineError {
    fn from(e: reqwest::Error) -> Self {
        Self::Parse(e.to_string())
    }
}

impl From<serde_json::Error> for EngineError {
    fn from(e: serde_json::Error) -> Self {
        Self::Parse(e.to_string())
    }
}

impl From<serde_yaml::Error> for EngineError {
    fn from(e: serde_yaml::Error) -> Self {
        Self::Parse(e.to_string())
    }
}

pub type EngineResult<T> = Result<T, EngineError>;
