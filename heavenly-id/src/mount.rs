//! How an entity's local frame is attached to its parent.

use alloc::sync::Arc;
use core::fmt;

use heavenly_core::{Epoch, Error, FrameId, LocationId, Rotation, Transform, Vec3};
use heavenly_geo::{Ellipsoid, Geodetic};

/// What an entity is mounted in: a known frame, or another entity's frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parent {
    Frame(FrameId),
    Entity(LocationId),
}

/// A position and velocity (m, m/s) in some inertial frame, as a function of
/// time: the trajectory of a moving platform's origin.
pub type TrajectoryFn = dyn Fn(Epoch) -> Result<(Vec3, Vec3), Error> + Send + Sync;

/// The orientation of a moving platform's axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Attitude {
    /// Axes fixed relative to the inertial frame: the rotation maps body
    /// components to inertial components.
    Inertial(Rotation),
    /// Local vertical, local horizontal: z towards the centre (nadir), y
    /// opposite the orbit normal, x completing the right-handed set (along
    /// the velocity for a circular orbit). The angular velocity used is
    /// `(r × v)/|r|²`, which is exact for two-body motion; perturbations add
    /// terms of order the perturbing acceleration over the speed.
    Lvlh,
}

/// How an entity's local frame is attached to its parent.
#[derive(Clone)]
pub enum Mount {
    /// No frame: a logical grouping (a region, a fleet) with no position.
    None,
    /// Fixed relative to the parent's frame: a building on a site, a room
    /// in a building, a rack in a room, a module on a spacecraft. The
    /// transform maps this entity's frame into the parent's.
    Fixed {
        parent: Parent,
        transform: Transform,
    },
    /// On a body's surface: an east-north-up frame at a geodetic position,
    /// in a body-fixed frame such as `ITRF` or `MOON_ME`.
    Geodetic {
        body_fixed: FrameId,
        ellipsoid: Ellipsoid,
        position: Geodetic,
    },
    /// A moving platform: the frame's origin follows `trajectory` in the
    /// inertial frame `inertial`, with the given attitude.
    Trajectory {
        inertial: FrameId,
        trajectory: Arc<TrajectoryFn>,
        attitude: Attitude,
    },
}

impl Mount {
    /// Mounted at `offset_m` in the parent entity, with axes rotated by
    /// `rotation` (this frame's components → parent's components).
    pub fn in_entity(parent: LocationId, rotation: Rotation, offset_m: Vec3) -> Mount {
        Mount::Fixed {
            parent: Parent::Entity(parent),
            transform: Transform::fixed(rotation, offset_m),
        }
    }

    /// Mounted at `offset_m` in a frame, with axes rotated by `rotation`.
    pub fn in_frame(parent: FrameId, rotation: Rotation, offset_m: Vec3) -> Mount {
        Mount::Fixed {
            parent: Parent::Frame(parent),
            transform: Transform::fixed(rotation, offset_m),
        }
    }

    /// The entity this mount hangs from, if it is another entity.
    pub fn parent_entity(&self) -> Option<LocationId> {
        match self {
            Mount::Fixed {
                parent: Parent::Entity(id),
                ..
            } => Some(*id),
            _ => None,
        }
    }
}

impl fmt::Debug for Mount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mount::None => f.write_str("Mount::None"),
            Mount::Fixed { parent, transform } => f
                .debug_struct("Mount::Fixed")
                .field("parent", parent)
                .field("transform", transform)
                .finish(),
            Mount::Geodetic {
                body_fixed,
                ellipsoid,
                position,
            } => f
                .debug_struct("Mount::Geodetic")
                .field("body_fixed", body_fixed)
                .field("ellipsoid", ellipsoid)
                .field("position", position)
                .finish(),
            Mount::Trajectory {
                inertial, attitude, ..
            } => f
                .debug_struct("Mount::Trajectory")
                .field("inertial", inertial)
                .field("attitude", attitude)
                .finish_non_exhaustive(),
        }
    }
}

/// The transform from a moving platform's frame into its inertial frame.
pub(crate) fn platform_transform(r: Vec3, v: Vec3, attitude: Attitude) -> Result<Transform, Error> {
    match attitude {
        Attitude::Inertial(rot) => Ok(Transform::new(rot, r, v, Vec3::ZERO)),
        Attitude::Lvlh => {
            let h = r.cross(v);
            let z = (-r)
                .normalized()
                .ok_or(Error::InvalidInput("LVLH needs a non-zero position"))?;
            let y = (-h)
                .normalized()
                .ok_or(Error::InvalidInput("LVLH needs non-zero angular momentum"))?;
            let x = y.cross(z);
            // Columns x, y, z: body components → inertial components.
            let m = [[x.x, y.x, z.x], [x.y, y.y, z.y], [x.z, y.z, z.z]];
            let rot = Rotation::from_matrix(m).ok_or(Error::NonFinite)?;
            Ok(Transform::new(rot, r, v, h / r.norm_squared()))
        }
    }
}
