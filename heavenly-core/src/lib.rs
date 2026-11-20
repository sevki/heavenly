//! Core types for `heavenly`: epochs and time scales, reference-frame
//! identifiers, state vectors, rigid frame transformations and the traits
//! that tie them together.
//!
//! **Identity is stable; position is time-dependent; reference frames provide
//! meaning; placement topology is policy.**
//!
//! # Units
//!
//! Everything is SI: metres, metres per second, radians, radians per second,
//! seconds. Fields and methods carry the unit in their name (`position_m`,
//! `angular_velocity_rad_s`, …).
//!
//! # Features
//!
//! * `std` (default): `std::error::Error` impls. Implies `alloc`.
//! * `alloc`: the [`FrameGraph`] resolver.
//!
//! Without either feature the crate is `no_std` and allocation-free.
//!
//! # Example
//!
//! ```
//! use heavenly_core::*;
//!
//! # fn main() -> Result<(), Error> {
//! // A facility frame 10 m east of a site origin, and a rack within it.
//! let site = FrameId::custom(Namespace(1), 100).unwrap();
//! let facility = FrameId::custom(Namespace(1), 101).unwrap();
//!
//! let mut frames = FrameGraph::new();
//! frames.add_fixed(facility, site, Transform::fixed(Rotation::IDENTITY, Vec3::new(10.0, 0.0, 0.0)))?;
//!
//! let rack = FixedLocator { frame: facility, position_m: Vec3::new(1.0, 2.0, 0.0) };
//! let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
//! let s = rack.state_at(epoch, site, &frames)?;
//! assert_eq!(s.position_m, Vec3::new(11.0, 2.0, 0.0));
//! # Ok(()) }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/sevki/heavenly/main/book/src/images/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/sevki/heavenly/main/book/theme/favicon.png"
)]
#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

#[cfg(feature = "alloc")]
extern crate alloc;

mod error;
mod frame;
#[cfg(feature = "alloc")]
mod graph;
mod locator;
mod math;
mod state;
pub mod time;
pub mod transform;

pub use error::Error;
pub use frame::{
    EarthFixed, Frame, FrameId, Gcrf, MarsFixed, MarsIcrf, MoonFixed, MoonIcrf, Namespace, SsbIcrf,
};
#[cfg(feature = "alloc")]
pub use graph::FrameGraph;
pub use locator::{FixedLocator, LocationId, LocationRecord, Locator};
pub use math::{Rotation, Vec3};
pub use state::{Coordinate, StateVector};
pub use time::{Calendar, Epoch, TimeScale};
pub use transform::{FrameResolver, FrameTransform, Transform};
