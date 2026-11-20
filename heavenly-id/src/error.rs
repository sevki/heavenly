use alloc::string::String;
use core::fmt;

use heavenly_core::LocationId;

/// Errors from the registry.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum IdError {
    /// An entity with this ID is already registered.
    DuplicateId(LocationId),
    /// This alias already names another entity.
    DuplicateAlias { alias: String, existing: LocationId },
    /// No entity has this alias.
    UnknownAlias(String),
    /// No entity has this ID.
    UnknownId(LocationId),
    /// The text is not a valid alias; the message says why.
    InvalidAlias { alias: String, reason: &'static str },
    /// An entity is mounted in a parent that is not registered.
    MissingParent {
        entity: LocationId,
        parent: LocationId,
    },
    /// Following parents from this entity leads back to it.
    Cycle(LocationId),
    /// The entity has no frame (it is a logical grouping), so it has no
    /// position.
    NotLocatable(LocationId),
    /// Two IDs map to the same frame code within the registry's namespace.
    FrameCollision { a: LocationId, b: LocationId },
    /// An error from `heavenly-core`, e.g. a missing frame path or epoch
    /// outside a trajectory.
    Core(heavenly_core::Error),
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdError::DuplicateId(id) => write!(f, "location {id} is already registered"),
            IdError::DuplicateAlias { alias, existing } => {
                write!(f, "alias {alias:?} already names {existing}")
            }
            IdError::UnknownAlias(a) => write!(f, "no location has the alias {a:?}"),
            IdError::UnknownId(id) => write!(f, "no location has the ID {id}"),
            IdError::InvalidAlias { alias, reason } => {
                write!(f, "invalid alias {alias:?}: {reason}")
            }
            IdError::MissingParent { entity, parent } => {
                write!(
                    f,
                    "location {entity} is mounted in {parent}, which is not registered"
                )
            }
            IdError::Cycle(id) => write!(f, "location {id} is (indirectly) mounted in itself"),
            IdError::NotLocatable(id) => {
                write!(f, "location {id} has no frame, so it has no position")
            }
            IdError::FrameCollision { a, b } => {
                write!(f, "locations {a} and {b} map to the same frame code")
            }
            IdError::Core(e) => e.fmt(f),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for IdError {}

impl From<heavenly_core::Error> for IdError {
    fn from(e: heavenly_core::Error) -> Self {
        IdError::Core(e)
    }
}
