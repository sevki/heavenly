//! Rigid, time-dependent frame transformations.
//!
//! # Conventions
//!
//! A [`Transform`] from frame **A** to frame **B**, evaluated at one epoch,
//! holds four quantities. *All of them are expressed along B's axes*:
//!
//! | Field                     | Meaning                                                      |
//! |---------------------------|--------------------------------------------------------------|
//! | `rotation`                | `R_BA`: maps A-axis components to B-axis components          |
//! | `translation_m`           | `t`: position of A's origin relative to B's origin           |
//! | `origin_velocity_m_s`     | `ṫ`: velocity of A's origin relative to B (as seen in B)     |
//! | `angular_velocity_rad_s`  | `ω`: angular velocity of A's axes relative to B's axes       |
//!
//! A state `(p_A, v_A)` maps to
//!
//! ```text
//! p_B = R p_A + t
//! v_B = R v_A + ω × (R p_A) + ṫ
//! ```
//!
//! The velocity term follows from differentiating `p_B` with `Ṙ = [ω]× R`.
//! Composition ([`Transform::then`]) and inversion ([`Transform::inverse`]) are
//! derived from these equations and preserve the convention exactly, so
//! arbitrarily long chains remain consistent.

use crate::{Epoch, Error, FrameId, Rotation, StateVector, Vec3};

/// Kinematics of frame A relative to frame B at a single epoch.
/// See the [module documentation](self) for the exact convention.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Transform {
    pub rotation: Rotation,
    pub translation_m: Vec3,
    pub origin_velocity_m_s: Vec3,
    pub angular_velocity_rad_s: Vec3,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        rotation: Rotation::IDENTITY,
        translation_m: Vec3::ZERO,
        origin_velocity_m_s: Vec3::ZERO,
        angular_velocity_rad_s: Vec3::ZERO,
    };

    pub const fn new(
        rotation: Rotation,
        translation_m: Vec3,
        origin_velocity_m_s: Vec3,
        angular_velocity_rad_s: Vec3,
    ) -> Transform {
        Transform {
            rotation,
            translation_m,
            origin_velocity_m_s,
            angular_velocity_rad_s,
        }
    }

    /// A transform between two frames that do not move relative to each other.
    pub const fn fixed(rotation: Rotation, translation_m: Vec3) -> Transform {
        Transform::new(rotation, translation_m, Vec3::ZERO, Vec3::ZERO)
    }

    /// Maps a position from A to B.
    pub fn apply_position(&self, p_a: Vec3) -> Vec3 {
        self.rotation.apply(p_a) + self.translation_m
    }

    /// Maps a position and velocity from A to B.
    pub fn apply(&self, p_a: Vec3, v_a: Vec3) -> (Vec3, Vec3) {
        let rp = self.rotation.apply(p_a);
        let p_b = rp + self.translation_m;
        let v_b = self.rotation.apply(v_a)
            + self.angular_velocity_rad_s.cross(rp)
            + self.origin_velocity_m_s;
        (p_b, v_b)
    }

    /// Composes `self` (A → B) with `next` (B → C), yielding A → C.
    pub fn then(&self, next: &Transform) -> Transform {
        let (translation_m, origin_velocity_m_s) =
            next.apply(self.translation_m, self.origin_velocity_m_s);
        Transform {
            rotation: next.rotation.compose(self.rotation),
            translation_m,
            origin_velocity_m_s,
            angular_velocity_rad_s: next.angular_velocity_rad_s
                + next.rotation.apply(self.angular_velocity_rad_s),
        }
    }

    /// The transform B → A.
    pub fn inverse(&self) -> Transform {
        let r_inv = self.rotation.inverse();
        let w = self.angular_velocity_rad_s;
        let t = self.translation_m;
        Transform {
            rotation: r_inv,
            translation_m: -r_inv.apply(t),
            origin_velocity_m_s: r_inv.apply(w.cross(t) - self.origin_velocity_m_s),
            angular_velocity_rad_s: -r_inv.apply(w),
        }
    }
}

/// A [`Transform`] labelled with its source frame, destination frame and
/// epoch, so that misuse is caught at run time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameTransform {
    pub source: FrameId,
    pub destination: FrameId,
    pub epoch: Epoch,
    pub transform: Transform,
}

impl FrameTransform {
    pub const fn identity(frame: FrameId, epoch: Epoch) -> FrameTransform {
        FrameTransform {
            source: frame,
            destination: frame,
            epoch,
            transform: Transform::IDENTITY,
        }
    }

    pub fn inverse(&self) -> FrameTransform {
        FrameTransform {
            source: self.destination,
            destination: self.source,
            epoch: self.epoch,
            transform: self.transform.inverse(),
        }
    }

    /// Composes `self` (A → B) with `next` (B → C).
    pub fn then(&self, next: &FrameTransform) -> Result<FrameTransform, Error> {
        if next.source != self.destination {
            return Err(Error::FrameMismatch {
                expected: self.destination,
                found: next.source,
            });
        }
        if next.epoch != self.epoch {
            return Err(Error::EpochMismatch {
                expected: self.epoch,
                found: next.epoch,
            });
        }
        Ok(FrameTransform {
            source: self.source,
            destination: next.destination,
            epoch: self.epoch,
            transform: self.transform.then(&next.transform),
        })
    }

    /// Re-expresses `state` in the destination frame.
    ///
    /// Fails if `state` is not in the source frame or not at this epoch.
    pub fn apply(&self, state: &StateVector) -> Result<StateVector, Error> {
        if state.frame != self.source {
            return Err(Error::FrameMismatch {
                expected: self.source,
                found: state.frame,
            });
        }
        if state.epoch != self.epoch {
            return Err(Error::EpochMismatch {
                expected: self.epoch,
                found: state.epoch,
            });
        }
        let (p, v) = self.transform.apply(state.position_m, state.velocity_m_s);
        Ok(StateVector::new(self.destination, self.epoch, p, v))
    }
}

/// Supplies transformations between frames at a given epoch.
///
/// Errors from the core (frame or epoch mismatches discovered while composing)
/// must be representable in the resolver's error type.
pub trait FrameResolver {
    type Error: From<Error>;

    /// The transformation from `source` to `destination` at `epoch`.
    ///
    /// The returned value's `source`, `destination` and `epoch` must equal the
    /// arguments.
    fn transform(
        &self,
        source: FrameId,
        destination: FrameId,
        epoch: Epoch,
    ) -> Result<FrameTransform, Self::Error>;
}

impl<R: FrameResolver + ?Sized> FrameResolver for &R {
    type Error = R::Error;
    fn transform(
        &self,
        source: FrameId,
        destination: FrameId,
        epoch: Epoch,
    ) -> Result<FrameTransform, Self::Error> {
        (**self).transform(source, destination, epoch)
    }
}
