use core::fmt;

use crate::{Epoch, FrameId};

/// Errors produced by the core types.
///
/// Adapters (ephemeris readers, propagators, registries) are expected to
/// define their own error types that implement `From<heavenly_core::Error>`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// A value was expressed in a different frame than the operation requires.
    FrameMismatch { expected: FrameId, found: FrameId },
    /// Two values were evaluated at different epochs.
    EpochMismatch { expected: Epoch, found: Epoch },
    /// No chain of known transformations connects the two frames.
    NoPath {
        source: FrameId,
        destination: FrameId,
    },
    /// A transformation exists but cannot be evaluated at the requested epoch
    /// (for example, an ephemeris kernel does not cover it).
    OutOfRange {
        source: FrameId,
        destination: FrameId,
        epoch: Epoch,
    },
    /// An edge between these two frames has already been registered.
    DuplicateEdge { a: FrameId, b: FrameId },
    /// The requested UTC instant precedes 1972-01-01, before which UTC was
    /// not defined by whole leap seconds.
    UtcOutOfRange,
    /// A calendar date or time-of-day field is out of range.
    InvalidCalendar,
    /// An epoch string could not be parsed.
    Parse(&'static str),
    /// A numeric input was NaN or infinite.
    NonFinite,
    /// An input was outside its valid domain; the message names it.
    InvalidInput(&'static str),
    /// A trajectory or ephemeris was asked for a state outside the time span
    /// it covers.
    OutsideCoverage {
        epoch: Epoch,
        start: Epoch,
        end: Epoch,
    },
    /// A trajectory covers the span around `epoch` but has no data between
    /// `before` and `after`, a gap longer than it is allowed to interpolate.
    DataGap {
        epoch: Epoch,
        before: Epoch,
        after: Epoch,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::FrameMismatch { expected, found } => {
                write!(f, "frame mismatch: expected {expected}, found {found}")
            }
            Error::EpochMismatch { expected, found } => {
                write!(f, "epoch mismatch: expected {expected:?}, found {found:?}")
            }
            Error::NoPath {
                source,
                destination,
            } => write!(f, "no transformation path from {source} to {destination}"),
            Error::OutOfRange {
                source,
                destination,
                epoch,
            } => write!(
                f,
                "transformation {source} -> {destination} unavailable at {epoch:?}"
            ),
            Error::DuplicateEdge { a, b } => {
                write!(f, "an edge between {a} and {b} is already registered")
            }
            Error::UtcOutOfRange => f.write_str("UTC is only supported from 1972-01-01"),
            Error::InvalidCalendar => f.write_str("invalid calendar date or time"),
            Error::Parse(msg) => write!(f, "parse error: {msg}"),
            Error::NonFinite => f.write_str("non-finite numeric input"),
            Error::InvalidInput(what) => write!(f, "invalid input: {what}"),
            Error::OutsideCoverage { epoch, start, end } => {
                write!(f, "{epoch} is outside the data span {start} – {end}")
            }
            Error::DataGap {
                epoch,
                before,
                after,
            } => {
                write!(f, "{epoch} falls in a data gap from {before} to {after}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}
