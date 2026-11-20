//! **heavenly**: describing, transforming and resolving physical locations
//! across the Earth, the Moon, the planets and moving spacecraft.
//!
//! [Book](https://sevki.github.io/heavenly/) · [Playground](https://sevki.github.io/heavenly/playground.html) · [API docs](https://sevki.github.io/heavenly/api/heavenly/)
//!
//! > **Identity is stable; position is time-dependent; reference frames
//! > provide meaning; placement topology is policy.**
//!
//! ```toml
//! [dependencies]
//! heavenly = "0.9"
//! ```
//!
//! The core types are at the crate root: [`Epoch`], [`FrameId`],
//! [`Transform`], [`StateVector`], [`FrameGraph`], [`Locator`], and so on.
//! Everything else is a module behind a cargo feature, and all of them are
//! on by default:
//!
//! | Module | Feature | What it does |
//! |--------|---------|--------------|
//! | [`geo`] | `geo` | ellipsoids, geodetic coordinates, Earth orientation, IAU Moon/Mars models |
//! | [`spice`] | `spice` | JPL SPK / binary PCK kernels (DE440 ephemerides, lunar orientation) |
//! | [`orbit`] | `orbit` | two-body, SGP4, J2 secular and numerical (J2–J6, drag, Sun, Moon) propagation, orbital elements, sampled trajectories |
//! | [`id`] | `id` | stable identities, aliases, and a registry of mounted local frames |
//! | [`query`] | `query` | light time, look angles, line of sight, visibility windows |
//! | [`topology`] | `topology` | failure domains and jurisdictions (policy, not coordinates) |
//!
//! With the off-by-default `jetstream` feature, `heavenly::wire` encodes the
//! core types in a compact, fixed-size binary form using
//! [jetstream](https://github.com/sevki/jetstream)'s `WireFormat`.
//!
//! For `no_std`, use `default-features = false` and enable what you need:
//! the core and most modules are allocation-free, and `alloc` adds the
//! frame graph and other heap-backed types.
//!
//! # Example
//!
//! ```
//! # #[cfg(all(feature = "geo", feature = "orbit", feature = "query"))]
//! # fn main() -> Result<(), heavenly::Error> {
//! use heavenly::geo::{earth, Ellipsoid, Geodetic};
//! use heavenly::orbit::{gm, ConicElements, TwoBodyLocator};
//! use heavenly::query::look_angles;
//! use heavenly::*;
//!
//! let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
//! let mut frames = FrameGraph::new();
//! earth::register(&mut frames, earth::EarthOrientation::ZERO)?;
//!
//! // A ground station in London (east-north-up) …
//! let london = FrameId::custom(Namespace(1), 1).unwrap();
//! let site = Geodetic::from_degrees(51.5072, -0.1276, 35.0)?;
//! frames.add_fixed(london, FrameId::ITRF, site.enu_to_body_fixed(&Ellipsoid::WGS84))?;
//!
//! // … and a satellite in a 420 km orbit.
//! let sat = TwoBodyLocator {
//!     frame: FrameId::GCRF,
//!     elements: ConicElements {
//!         periapsis_m: 6_798_137.0,
//!         eccentricity: 0.0005,
//!         inclination_rad: 51.64f64.to_radians(),
//!         ascending_node_rad: 3.0,
//!         argument_of_periapsis_rad: 0.0,
//!         mean_anomaly_rad: 0.0,
//!         epoch,
//!         gm_m3_s2: gm::EARTH,
//!     },
//! };
//! let look = look_angles::<_, _, Error>(london, &sat, epoch, &frames)?;
//! println!("elevation {:.1}°, range {:.0} km", look.elevation_rad.to_degrees(), look.range_m / 1e3);
//! # Ok(()) }
//! # #[cfg(not(all(feature = "geo", feature = "orbit", feature = "query")))]
//! # fn main() {}
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/sevki/heavenly/main/book/src/images/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/sevki/heavenly/main/book/theme/favicon.png"
)]
#![forbid(unsafe_code)]

pub use heavenly_core::*;

/// Ellipsoids, geodetic coordinates, Earth orientation and IAU body models.
#[cfg(feature = "geo")]
pub use heavenly_geo as geo;

/// JPL SPICE kernels: SPK ephemerides and binary PCK orientation.
#[cfg(feature = "spice")]
pub use heavenly_spice as spice;

/// Orbit propagation (two-body, SGP4, J2 secular, numerical with
/// perturbations), orbital elements and sampled trajectories.
#[cfg(feature = "orbit")]
pub use heavenly_orbit as orbit;

/// Stable identities, aliases and a registry of mounted local frames.
#[cfg(feature = "id")]
pub use heavenly_id as id;

/// Light time, look angles, line of sight and visibility windows.
#[cfg(feature = "query")]
pub use heavenly_query as query;

/// Failure domains, jurisdictions and placement policy.
#[cfg(feature = "topology")]
pub use heavenly_topology as topology;
