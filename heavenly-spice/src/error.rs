use core::fmt;

use heavenly_core::Epoch;

/// Errors from reading or evaluating SPICE kernels.
#[derive(Debug)]
#[non_exhaustive]
pub enum SpiceError {
    /// The file could not be read.
    #[cfg(feature = "std")]
    Io(std::io::Error),
    /// The bytes are not a well-formed DAF file of the expected kind.
    Format(&'static str),
    /// No loaded segment covers this body (or PCK frame) at this epoch.
    NoCoverage { id: i32, epoch: Epoch },
    /// The bodies' segment chains do not meet at a common centre.
    NoPath { target: i32, observer: i32 },
    /// A segment uses an SPK/PCK data type this crate does not evaluate.
    UnsupportedType { id: i32, data_type: i32 },
    /// A segment is relative to a frame other than J2000/ICRF.
    UnsupportedFrame { id: i32, frame: i32 },
    /// An error from `heavenly-core`.
    Core(heavenly_core::Error),
}

impl fmt::Display for SpiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "std")]
            SpiceError::Io(e) => write!(f, "I/O error: {e}"),
            SpiceError::Format(m) => write!(f, "malformed kernel: {m}"),
            SpiceError::NoCoverage { id, epoch } => {
                write!(f, "no kernel data for NAIF ID {id} at {epoch}")
            }
            SpiceError::NoPath { target, observer } => {
                write!(
                    f,
                    "no common centre connects NAIF IDs {target} and {observer}"
                )
            }
            SpiceError::UnsupportedType { id, data_type } => {
                write!(f, "NAIF ID {id}: segment type {data_type} is not supported")
            }
            SpiceError::UnsupportedFrame { id, frame } => {
                write!(f, "NAIF ID {id}: segment frame {frame} is not J2000")
            }
            SpiceError::Core(e) => e.fmt(f),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SpiceError {}

impl From<heavenly_core::Error> for SpiceError {
    fn from(e: heavenly_core::Error) -> Self {
        SpiceError::Core(e)
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for SpiceError {
    fn from(e: std::io::Error) -> Self {
        SpiceError::Io(e)
    }
}
