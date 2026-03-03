use std::io;

/// Errors that can occur during terminal capability probing.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// /dev/tty is not available (not a terminal, or no permission)
    #[error("cannot open /dev/tty: {0}")]
    NoTty(#[source] io::Error),

    /// Read, write, or mode-switch on the terminal failed
    #[error("terminal I/O failed: {0}")]
    Io(#[source] io::Error),

    /// Probe results could not be serialized to JSON
    #[error("JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}
