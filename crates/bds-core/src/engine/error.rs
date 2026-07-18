use std::fmt;

/// Errors produced by engine operations.
#[derive(Debug)]
pub enum EngineError {
    Db(diesel::result::Error),
    DbConnection(diesel::ConnectionError),
    Io(std::io::Error),
    Parse(String),
    NotFound(String),
    Conflict(String),
    Validation(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Db(e) => write!(f, "database error: {e}"),
            Self::DbConnection(e) => write!(f, "database connection error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Parse(msg) => write!(f, "parse error: {msg}"),
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::Conflict(msg) => write!(f, "conflict: {msg}"),
            Self::Validation(msg) => write!(f, "validation error: {msg}"),
        }
    }
}

impl std::error::Error for EngineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Db(e) => Some(e),
            Self::DbConnection(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<diesel::result::Error> for EngineError {
    fn from(e: diesel::result::Error) -> Self {
        Self::Db(e)
    }
}

impl From<diesel::ConnectionError> for EngineError {
    fn from(e: diesel::ConnectionError) -> Self {
        Self::DbConnection(e)
    }
}

impl From<crate::db::DatabaseError> for EngineError {
    fn from(e: crate::db::DatabaseError) -> Self {
        match e {
            crate::db::DatabaseError::Connection(e) => Self::DbConnection(e),
            crate::db::DatabaseError::Query(e) => Self::Db(e),
        }
    }
}

impl From<std::io::Error> for EngineError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_variants() {
        assert!(
            EngineError::Parse("bad yaml".into())
                .to_string()
                .contains("parse error")
        );
        assert!(
            EngineError::NotFound("post 123".into())
                .to_string()
                .contains("not found")
        );
        assert!(
            EngineError::Conflict("slug taken".into())
                .to_string()
                .contains("conflict")
        );
        assert!(
            EngineError::Validation("title empty".into())
                .to_string()
                .contains("validation")
        );
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let engine_err = EngineError::from(io_err);
        assert!(matches!(engine_err, EngineError::Io(_)));
    }
}
