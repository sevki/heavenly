//! Resolving an object's state at an epoch.

use core::fmt;

use crate::{Epoch, FrameId, FrameResolver, StateVector, Vec3};

/// A stable, opaque object identity. It never encodes a position.
///
/// Human-readable aliases (e.g. `dc:lhr:01`) belong in a registry that maps
/// them to `LocationId`s; renaming an alias does not change the identity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocationId(pub u128);

impl LocationId {
    /// The 16 bytes of the ID, big-endian (the byte order of a UUID).
    pub const fn to_bytes(self) -> [u8; 16] {
        self.0.to_be_bytes()
    }

    /// Builds an ID from 16 big-endian bytes, such as a UUID's.
    pub const fn from_bytes(b: [u8; 16]) -> LocationId {
        LocationId(u128::from_be_bytes(b))
    }

    /// Parses 32 hexadecimal digits, optionally hyphenated as a UUID
    /// (`8-4-4-4-12`). Case-insensitive. This accepts the output of both
    /// `Display` and a UUID's canonical form.
    pub fn parse(s: &str) -> Result<LocationId, crate::Error> {
        let b = s.as_bytes();
        let hyphenated = b.len() == 36;
        if hyphenated {
            for (i, c) in b.iter().enumerate() {
                if matches!(i, 8 | 13 | 18 | 23) != (*c == b'-') {
                    return Err(crate::Error::Parse("malformed UUID hyphenation"));
                }
            }
        } else if b.len() != 32 {
            return Err(crate::Error::Parse(
                "a location ID is 32 hex digits or a hyphenated UUID",
            ));
        }
        let mut v: u128 = 0;
        for c in b.iter().filter(|c| **c != b'-') {
            let d = (*c as char)
                .to_digit(16)
                .ok_or(crate::Error::Parse("non-hexadecimal digit in location ID"))?;
            v = (v << 4) | d as u128;
        }
        Ok(LocationId(v))
    }
}

impl core::str::FromStr for LocationId {
    type Err = crate::Error;
    fn from_str(s: &str) -> Result<LocationId, crate::Error> {
        LocationId::parse(s)
    }
}

impl From<u128> for LocationId {
    fn from(v: u128) -> LocationId {
        LocationId(v)
    }
}

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
