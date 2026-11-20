//! Spatial queries over `heavenly` locators: when can a signal get from A to
//! B, how long does it take, and can A see B at all?
//!
//! [Book](https://sevki.github.io/heavenly/) · [Playground](https://sevki.github.io/heavenly/playground.html) · [API docs](https://sevki.github.io/heavenly/api/heavenly_query/)
//!
//! * [`light_time`]: converged Newtonian light time in either direction,
//!   with the corrected relative position and range rate. It matches
//!   SPICE's `CN` / `XCN`.
//! * [`look_angles`]: azimuth, elevation and range from a site.
//! * [`Occulter`]: line of sight past an ellipsoidal body (Earth, Moon).
//! * [`find_windows`]: rise/set intervals of any continuous condition.
//!
//! Separation and relative velocity at a common epoch are on
//! [`heavenly_core::StateVector`]. Distance at a common epoch is *not*
//! signal delay; that is what [`light_time`] is for.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

#[cfg(feature = "alloc")]
extern crate alloc;

mod light_time;
mod shadow;
mod sun;
mod visibility;

pub use light_time::{light_time, Direction, LightTime, SPEED_OF_LIGHT_M_S};
#[cfg(feature = "alloc")]
pub use shadow::{eclipses, Eclipses};
pub use shadow::{Shadow, ShadowKind};
pub use sun::{AnalyticSun, AU_M, SUN_RADIUS_M};
#[cfg(feature = "alloc")]
pub use visibility::find_windows;
pub use visibility::{look_angles, LookAngles, Occulter, Window};
