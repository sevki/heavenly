//! Position/velocity states, dynamically and statically framed.

use core::fmt;
use core::marker::PhantomData;

use crate::{Epoch, Error, Frame, FrameId, FrameResolver, Vec3};

/// Position (metres) and velocity (metres per second) of a point relative to
/// a frame, at an epoch. The frame is checked at run time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateVector {
    pub frame: FrameId,
    pub epoch: Epoch,
    pub position_m: Vec3,
    pub velocity_m_s: Vec3,
}

impl StateVector {
    pub const fn new(frame: FrameId, epoch: Epoch, position_m: Vec3, velocity_m_s: Vec3) -> Self {
        StateVector {
            frame,
            epoch,
            position_m,
            velocity_m_s,
        }
    }

    /// A point at rest at `position_m` in `frame`.
    pub const fn at_rest(frame: FrameId, epoch: Epoch, position_m: Vec3) -> Self {
        StateVector::new(frame, epoch, position_m, Vec3::ZERO)
    }

    /// Checks that `other` shares this state's frame and epoch.
    pub fn check_compatible(&self, other: &StateVector) -> Result<(), Error> {
        if self.frame != other.frame {
            return Err(Error::FrameMismatch {
                expected: self.frame,
                found: other.frame,
            });
        }
        if self.epoch != other.epoch {
            return Err(Error::EpochMismatch {
                expected: self.epoch,
                found: other.epoch,
            });
        }
        Ok(())
    }

    /// Position and velocity of `other` relative to `self`, in the shared frame.
    pub fn relative(&self, other: &StateVector) -> Result<(Vec3, Vec3), Error> {
        self.check_compatible(other)?;
        Ok((
            other.position_m - self.position_m,
            other.velocity_m_s - self.velocity_m_s,
        ))
    }

    /// Euclidean distance in metres between two states in the same frame at
    /// the same epoch. This is geometric separation, not light-time.
    pub fn separation_m(&self, other: &StateVector) -> Result<f64, Error> {
        Ok(self.relative(other)?.0.norm())
    }

    /// Magnitude of the relative velocity in metres per second.
    ///
    /// Relative velocity is frame-dependent between rotating frames; this is
    /// the value as observed in the shared frame.
    pub fn relative_speed_m_s(&self, other: &StateVector) -> Result<f64, Error> {
        Ok(self.relative(other)?.1.norm())
    }

    /// Re-expresses this state in `destination`.
    pub fn to_frame<R: FrameResolver + ?Sized>(
        &self,
        destination: FrameId,
        resolver: &R,
    ) -> Result<StateVector, R::Error> {
        let t = resolver.transform(self.frame, destination, self.epoch)?;
        Ok(t.apply(self)?)
    }
}

/// A state whose frame is fixed at compile time.
///
/// Operations between coordinates in different frames do not type-check:
///
/// ```compile_fail
/// use heavenly_core::{Coordinate, EarthFixed, Gcrf, Epoch, Vec3};
/// let a = Coordinate::<EarthFixed>::new(Epoch::J2000, Vec3::ZERO, Vec3::ZERO);
/// let b = Coordinate::<Gcrf>::new(Epoch::J2000, Vec3::ZERO, Vec3::ZERO);
/// let _ = a.separation_m(&b);
/// ```
pub struct Coordinate<F: Frame> {
    pub position_m: Vec3,
    pub velocity_m_s: Vec3,
    pub epoch: Epoch,
    _frame: PhantomData<fn() -> F>,
}

impl<F: Frame> Coordinate<F> {
    pub const fn new(epoch: Epoch, position_m: Vec3, velocity_m_s: Vec3) -> Self {
        Coordinate {
            position_m,
            velocity_m_s,
            epoch,
            _frame: PhantomData,
        }
    }

    pub const fn frame(&self) -> FrameId {
        F::ID
    }

    /// Erases the type-level frame.
    pub const fn to_state(&self) -> StateVector {
        StateVector::new(F::ID, self.epoch, self.position_m, self.velocity_m_s)
    }

    /// Recovers a typed coordinate, checking the frame at run time.
    pub fn from_state(s: &StateVector) -> Result<Self, Error> {
        if s.frame != F::ID {
            return Err(Error::FrameMismatch {
                expected: F::ID,
                found: s.frame,
            });
        }
        Ok(Coordinate::new(s.epoch, s.position_m, s.velocity_m_s))
    }

    /// Distance in metres to another coordinate in the same frame.
    /// Fails only if the epochs differ.
    pub fn separation_m(&self, other: &Coordinate<F>) -> Result<f64, Error> {
        self.to_state().separation_m(&other.to_state())
    }

    /// Relative speed in metres per second, as observed in frame `F`.
    pub fn relative_speed_m_s(&self, other: &Coordinate<F>) -> Result<f64, Error> {
        self.to_state().relative_speed_m_s(&other.to_state())
    }

    /// Explicitly transforms into frame `G`.
    pub fn to_frame<G: Frame, R: FrameResolver + ?Sized>(
        &self,
        resolver: &R,
    ) -> Result<Coordinate<G>, R::Error> {
        let s = self.to_state().to_frame(G::ID, resolver)?;
        Ok(Coordinate::new(s.epoch, s.position_m, s.velocity_m_s))
    }
}

impl<F: Frame> Clone for Coordinate<F> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<F: Frame> Copy for Coordinate<F> {}

impl<F: Frame> PartialEq for Coordinate<F> {
    fn eq(&self, o: &Self) -> bool {
        self.position_m == o.position_m
            && self.velocity_m_s == o.velocity_m_s
            && self.epoch == o.epoch
    }
}

impl<F: Frame> fmt::Debug for Coordinate<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Coordinate")
            .field("frame", &F::ID)
            .field("epoch", &self.epoch)
            .field("position_m", &self.position_m)
            .field("velocity_m_s", &self.velocity_m_s)
            .finish()
    }
}
