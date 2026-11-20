//! Orbital motion for `heavenly`: things whose position comes from a
//! trajectory rather than a fixed point.
//!
//! [Book](https://sevki.github.io/heavenly/) · [Playground](https://sevki.github.io/heavenly/playground.html) · [API docs](https://sevki.github.io/heavenly/api/heavenly_orbit/)
//!
//! * [`kepler::propagate`]: universal-variable two-body propagation for
//!   elliptic, parabolic and hyperbolic orbits.
//! * [`ConicElements`]: orbital elements ↔ state vectors (SPICE conventions).
//! * [`TwoBodyLocator`]: a [`heavenly_core::Locator`] for a Keplerian orbit.
//! * [`SampledTrajectory`]: a `Locator` for tabulated states, with explicit
//!   errors for out-of-coverage epochs and data gaps.
//! * [`Tle`] and [`Sgp4`]: NORAD two-line element sets and the SGP4/SDP4
//!   propagator (a port of Vallado's reference implementation), giving
//!   states in [`FrameId::TEME`](heavenly_core::FrameId::TEME).
//! * [`SecularJ2Locator`]: a conic whose plane and apsides turn at the
//!   Earth's J2 secular rates, as cheap as two-body.
//! * [`NumericalPropagator`]: integrates the equations of motion under a
//!   [`ForceModel`]: zonal gravity to J6, atmospheric drag, solar radiation
//!   pressure, and the Sun and the Moon ([`analytic`] series, or your own).
//! * [`gm`]: DE440 gravitational parameters.
//!
//! Everything is validated in the tests: the two-body code against SPICE
//! (`prop2b`, `conics`, `oscelt`), and SGP4 against Vallado's C++ over the
//! official verification set.
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

pub mod analytic;
#[cfg(feature = "alloc")]
mod catalog;
pub mod design;
mod elements;
pub mod forces;
pub mod gm;
pub mod kepler;
mod numerical;
#[cfg(feature = "alloc")]
mod omm;
#[cfg(feature = "alloc")]
mod sampled;
mod secular;
pub mod sgp4;
mod tle;
mod two_body;
pub mod uncertainty;

#[cfg(feature = "alloc")]
pub use catalog::{Catalog, CatalogEntry, Rejected};
pub use elements::ConicElements;
pub use forces::{Drag, ForceModel, RadiationPressure, ThirdBody, Zonal};
pub use numerical::{NumericalPropagator, Tolerance};
#[cfg(feature = "alloc")]
pub use omm::Omm;
#[cfg(feature = "alloc")]
pub use sampled::SampledTrajectory;
pub use secular::SecularJ2Locator;
pub use sgp4::{Gravity, OpsMode, Sgp4, Sgp4Error};
pub use tle::{checksum, Tle};
pub use two_body::TwoBodyLocator;
