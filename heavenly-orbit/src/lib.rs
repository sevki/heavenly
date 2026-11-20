//! Orbital motion for `heavenly`: things whose position comes from a
//! trajectory rather than a fixed point.
//!
//! * [`kepler::propagate`]: universal-variable two-body propagation for
//!   elliptic, parabolic and hyperbolic orbits.
//! * [`ConicElements`]: orbital elements ↔ state vectors (SPICE conventions).
//! * [`TwoBodyLocator`]: a [`heavenly_core::Locator`] for a Keplerian orbit.
//! * [`SampledTrajectory`]: a `Locator` for tabulated states, with explicit
//!   errors for out-of-coverage epochs and data gaps.
//! * [`gm`]: DE440 gravitational parameters.
//!
//! Everything is validated against SPICE (`prop2b`, `conics`, `oscelt`) in
//! the tests.
//!
//! # Example
//!
//! ```
//! use heavenly_core::*;
//! use heavenly_orbit::{gm, ConicElements, TwoBodyLocator};
//!
//! # fn main() -> Result<(), Error> {
//! let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
//! // A 500 km circular orbit at 51.6° (ISS-like), in GCRF.
//! let iss = TwoBodyLocator {
//!     frame: FrameId::GCRF,
//!     elements: ConicElements {
//!         periapsis_m: 6_878_137.0,
//!         eccentricity: 0.0,
//!         inclination_rad: 51.6f64.to_radians(),
//!         ascending_node_rad: 0.0,
//!         argument_of_periapsis_rad: 0.0,
//!         mean_anomaly_rad: 0.0,
//!         epoch,
//!         gm_m3_s2: gm::EARTH,
//!     },
//! };
//! let period_s = iss.elements.period_s().unwrap();
//! assert!((period_s / 60.0 - 94.6).abs() < 0.1);
//! // A circular orbit keeps its radius at every epoch.
//! for minutes in [0.0, 17.0, 60.0, 1440.0] {
//!     let s = iss.state(epoch.add_seconds(minutes * 60.0)?)?;
//!     assert!((s.position_m.norm() - 6_878_137.0).abs() < 1e-6);
//! }
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

mod elements;
pub mod gm;
pub mod kepler;
#[cfg(feature = "alloc")]
mod sampled;
mod two_body;

pub use elements::ConicElements;
#[cfg(feature = "alloc")]
pub use sampled::SampledTrajectory;
pub use two_body::TwoBodyLocator;
