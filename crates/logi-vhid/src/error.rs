//! Error type used across the crate.

use std::fmt;
use std::io;

/// Result type used by this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors from the Logitech virtual HID interface.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A Windows API or supporting I/O/thread operation failed.
    Io {
        /// Short operation name, e.g. `"DeviceIoControl"`.
        context: &'static str,
        /// Underlying error. Some Windows APIs do not set a useful last-error
        /// code (notably input blocked by UIPI), so this may be zero or stale.
        source: io::Error,
    },
    /// A protocol/state check failed, including a PID gate or cursor that could
    /// not reach its target; not necessarily a malformed driver response.
    Protocol(String),
    /// The requested input, option or state is unsupported (for example a
    /// disabled device, unmapped character, or full six-key report).
    Unsupported(String),
    /// A bus interface, expected HID child, or matching visible window was not
    /// found, including a child-device appearance timeout.
    NotFound(String),
    /// Cooperative execution was cancelled at a safe boundary.
    Cancelled,
    /// Cooperative execution reached its caller-provided deadline.
    TimedOut,
}

impl Error {
    pub(crate) fn io(context: &'static str, source: io::Error) -> Self {
        Self::Io { context, source }
    }

    pub(crate) fn last_io(context: &'static str) -> Self {
        Self::io(context, io::Error::last_os_error())
    }

    pub(crate) fn is_invalid_parameter(&self) -> bool {
        matches!(
            self,
            Self::Io { source, .. } if source.raw_os_error() == Some(87)
        )
    }

    pub(crate) fn is_no_more_items(&self) -> bool {
        matches!(
            self,
            Self::Io { source, .. } if source.raw_os_error() == Some(259)
        )
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::Protocol(message) => write!(f, "protocol error: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported: {message}"),
            Self::NotFound(message) => write!(f, "not found: {message}"),
            Self::Cancelled => f.write_str("execution cancelled"),
            Self::TimedOut => f.write_str("execution timed out"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(source: io::Error) -> Self {
        Self::io("windows api", source)
    }
}
