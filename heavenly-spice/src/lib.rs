//! A dependency-free reader for NAIF SPICE kernels, connecting JPL
//! ephemerides to `heavenly` frames.
//!
//! * [`Spk`]: SPK ephemerides, types 2 and 3 (Chebyshev position, and
//!   position + velocity). These are the types JPL's DE planetary and lunar
//!   ephemerides use, e.g. `de440s.bsp`.
//! * [`Pck`]: binary PCK orientation, type 2, e.g. the DE440 lunar
//!   `moon_pa_de440_200625.bpc`.
//! * [`frames`]: helpers that register body-centred ICRF frames and the
//!   lunar `MOON_PA` / `MOON_ME` frames on a [`heavenly_core::FrameGraph`].
//! * [`SpkLocator`]: a [`heavenly_core::Locator`] for any body in a kernel.
//!
//! Both little- and big-endian (`LTL-IEEE` / `BIG-IEEE`) files are read.
//! Kernels are not bundled; download them from
//! <https://naif.jpl.nasa.gov/pub/naif/generic_kernels/>.
//!
//! # Time
//!
//! Kernels are indexed by TDB. `heavenly-core` converts TT to TDB with a
//! two-term series (~30 µs), which displaces the Earth relative to the Solar
//! System barycentre by up to ~1 m. Positions *between* bodies near each
//! other (Earth–Moon) are affected by ~3 cm.
//!
//! # Example
//!
//! ```no_run
//! use std::sync::Arc;
//! use heavenly_core::*;
//! use heavenly_spice::{frames, Pck, Spk};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut graph = FrameGraph::new();
//! let spk = Arc::new(Spk::open("de440s.bsp")?);
//! frames::register_ephemeris(&mut graph, spk, frames::STANDARD_BODIES)?;
//! frames::register_moon(&mut graph, Arc::new(Pck::open("moon_pa_de440_200625.bpc")?))?;
//!
//! let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
//! let t = graph.transform(FrameId::MOON_ICRF, FrameId::GCRF, epoch)?;
//! println!("Earth–Moon distance: {:.0} km", t.transform.translation_m.norm() / 1e3);
//! # Ok(()) }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/sevki/heavenly/main/book/src/images/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/sevki/heavenly/main/book/theme/favicon.png"
)]
#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

extern crate alloc;

mod daf;
mod error;
pub mod frames;
mod locator;
mod pck;
mod rot;
mod spk;

pub use error::SpiceError;
pub use locator::SpkLocator;
pub use pck::{Pck, PckSegment};
pub use spk::{Spk, SpkSegment, J2000};
