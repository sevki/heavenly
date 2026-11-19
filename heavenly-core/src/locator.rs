//! Resolving an object's state at an epoch.

use core::fmt;

use crate::{Epoch, FrameId, FrameResolver, StateVector, Vec3};

/// A stable, opaque object identity. It never encodes a position.
///
/// Human-readable aliases (e.g. `dc:lhr:01`) belong in a registry that maps
/// them to `LocationId`s; renaming an alias does not change the identity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocationId(pub u128);

impl fmt::Display for LocationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

impl fmt::Debug for LocationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LocationId({self})")
    }
}

/// Resolves an object's state in a requested frame at a requested epoch.
///
/// Implementations may use fixed coordinates, an orbital propagator, an
/// ephemeris, or any custom motion model.
pub trait Locator {
    type Error;

    fn state_at<R: FrameResolver + ?Sized>(
        &self,
        epoch: Epoch,
        frame: FrameId,
        resolver: &R,
    ) -> Result<StateVector, Self::Error>
    where
        Self::Error: From<R::Error>;
}

/// An object at rest at a fixed position in some frame — for example a rack
/// in a facility frame, or a ground station in an Earth-fixed frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedLocator {
    pub frame: FrameId,
    pub position_m: Vec3,
}

impl Locator for FixedLocator {
    type Error = crate::Error;

    fn state_at<R: FrameResolver + ?Sized>(
        &self,
        epoch: Epoch,
        frame: FrameId,
        resolver: &R,
    ) -> Result<StateVector, Self::Error>
    where
        Self::Error: From<R::Error>,
    {
        let here = StateVector::at_rest(self.frame, epoch, self.position_m);
        Ok(here.to_frame(frame, resolver)?)
    }
}

/// Pairs a stable identity with the means of locating it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocationRecord<L> {
    pub id: LocationId,
    pub locator: L,
}
