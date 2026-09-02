//! How a Prompt fails, as distinct from how it is abandoned.

use std::fmt;

/// What `.interact()` returns instead of an answer.
///
/// Cancel is one of these and I/O failure is another, which is the whole reason the enum exists:
/// clack resolves a cancelled Prompt with a symbol rather than throwing, because the surrounding
/// program may legitimately carry on. `.interact()` bubbles it as an error because that is what
/// most callers want; `.interact_opt()` hands back an [`Option`] and keeps clack's own semantics.
#[derive(Debug)]
pub enum ClackError {
	/// The user abandoned the Prompt — escape, or `ctrl+c`.
	Cancelled,
	/// The terminal could not be read from, written to, or put into raw mode.
	Io(std::io::Error),
}

impl fmt::Display for ClackError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Cancelled => f.write_str("cancelled"),
			Self::Io(error) => write!(f, "{error}"),
		}
	}
}

impl std::error::Error for ClackError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Self::Cancelled => None,
			Self::Io(error) => Some(error),
		}
	}
}

impl From<std::io::Error> for ClackError {
	fn from(error: std::io::Error) -> Self {
		Self::Io(error)
	}
}
