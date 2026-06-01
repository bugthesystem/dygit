//! Error type for the crate.

use std::fmt;

/// Everything that can go wrong inside `dygi`.
///
/// The hook path treats every variant the same way — log nothing, emit nothing,
/// exit 0 — so the variants exist for *diagnosis* (the `history`/`stats` commands
/// can surface a readable message), not for control flow.
#[derive(Debug)]
pub enum DygiError {
    /// An I/O operation failed (reading stdin, writing the log, etc.).
    Io(std::io::Error),
    /// JSON could not be parsed or produced.
    Json(serde_json::Error),
    /// The plugin data directory could not be located.
    NoDataDir,
}

impl fmt::Display for DygiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DygiError::Io(e) => write!(f, "io error: {e}"),
            DygiError::Json(e) => write!(f, "json error: {e}"),
            DygiError::NoDataDir => write!(f, "could not locate plugin data directory"),
        }
    }
}

impl std::error::Error for DygiError {}

impl From<std::io::Error> for DygiError {
    fn from(e: std::io::Error) -> Self {
        DygiError::Io(e)
    }
}

impl From<serde_json::Error> for DygiError {
    fn from(e: serde_json::Error) -> Self {
        DygiError::Json(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_human_readable() {
        let e = DygiError::NoDataDir;
        assert_eq!(e.to_string(), "could not locate plugin data directory");
    }

    #[test]
    fn io_error_converts() {
        let io = std::io::Error::other("boom");
        let e: DygiError = io.into();
        assert!(matches!(e, DygiError::Io(_)));
    }
}
