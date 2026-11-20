//! Ellipsoids, geodetic coordinates and body orientation for `heavenly`.
//!
//! * [`Ellipsoid`] and [`Geodetic`]: latitude/longitude/height ↔ body-fixed
//!   Cartesian, plus local east-north-up frames for facilities.
//! * [`earth`]: ITRF ↔ GCRF (IAU 2006 precession, truncated IAU 2000B
//!   nutation, ERA, polar motion), with caller-supplied EOP.
//! * [`iau`]: IAU/WGCCRE rotation models for the Moon and Mars.
//!
//! All models are validated against independent references (SOFA and
//! SPICE) in this crate's tests; each module documents its accuracy.
//!
//! # Example
//!
//! ```
//! use heavenly_core::*;
//! use heavenly_geo::{earth, Ellipsoid, Geodetic};
//!
//! # fn main() -> Result<(), Error> {
//! let mut frames = FrameGraph::new();
//! earth::register(&mut frames, earth::EarthOrientation::ZERO)?;
//!
//! // A facility frame (east-north-up) attached to the ITRF in London.
//! let london = FrameId::custom(Namespace(1), 1).unwrap();
//! let site = Geodetic::from_degrees(51.5072, -0.1276, 35.0)?;
//! frames.add_fixed(london, FrameId::ITRF, site.enu_to_body_fixed(&Ellipsoid::WGS84))?;
//!
//! // A rack 20 m north of the site origin, seen from the GCRF.
//! let rack = FixedLocator { frame: london, position_m: Vec3::new(0.0, 20.0, 0.0) };
//! let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
//! let s = rack.state_at(epoch, FrameId::GCRF, &frames)?;
//! assert!((s.position_m.norm() - 6.365e6).abs() < 1e4);
//! # Ok(()) }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod earth;
mod ellipsoid;
mod geodetic;
pub mod iau;
mod mat;

pub use ellipsoid::Ellipsoid;
pub use geodetic::Geodetic;
