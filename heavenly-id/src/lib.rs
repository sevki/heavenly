//! Stable identities, aliases and local frames for `heavenly`.
//!
//! [Book](https://sevki.github.io/heavenly/) · [Playground](https://sevki.github.io/heavenly/playground.html) · [API docs](https://sevki.github.io/heavenly/api/heavenly_id/)
//!
//! **Identity is stable; position is time-dependent.** A [`Registry`] keeps
//! these apart:
//!
//! * every entity (site, building, room, rack, device, spacecraft, module…)
//!   has an immutable [`heavenly_core::LocationId`];
//! * [`Alias`]es such as `dc:lhr:01` are a separate, mutable mapping onto
//!   IDs, and can be renamed or re-pointed without touching identity;
//! * each entity's [`Mount`] attaches a local frame to its parent: fixed in
//!   another entity, geodetic on a body, or following a trajectory with an
//!   attitude.
//!
//! [`Registry::register_frames`] turns the whole tree into frame-graph edges,
//! so any rack, device or module can be located in any connected frame.
//!
//! [`Registry`] is a trait: implement its four lookup methods over wherever
//! your entities live (a database, an inventory service), and the frames,
//! paths, validation and locators come with it, identical for every store.
//! [`MemoryRegistry`] is the in-memory implementation, with the write
//! operations.
//!
//! # Example
//!
//! ```
//! use heavenly_core::*;
//! use heavenly_geo::{earth, Ellipsoid, Geodetic};
//! use heavenly_id::{Entity, Kind, MemoryRegistry, Mount, Registry};
//!
//! # fn main() -> Result<(), heavenly_id::IdError> {
//! let mut reg = MemoryRegistry::new(Namespace(7))?;
//! let site = reg.add(Entity::new(
//!     LocationId(1), Kind::Facility, "London LHR1",
//!     Mount::Geodetic {
//!         body_fixed: FrameId::ITRF,
//!         ellipsoid: Ellipsoid::WGS84,
//!         position: Geodetic::from_degrees(51.5072, -0.1276, 35.0)?,
//!     },
//! ))?;
//! let rack = reg.add(Entity::new(
//!     LocationId(2), Kind::Rack, "Rack 7",
//!     Mount::in_entity(site, Rotation::IDENTITY, Vec3::new(12.0, 30.0, 1.0)),
//! ))?;
//! reg.alias("dc:lhr:01", site)?;
//! reg.alias("dc:lhr:01/rack-07", rack)?;
//!
//! let mut graph = FrameGraph::new();
//! earth::register(&mut graph, earth::EarthOrientation::ZERO)?;
//! reg.register_frames(&mut graph)?;
//!
//! let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
//! let s = reg.locate("dc:lhr:01/rack-07", epoch, FrameId::ITRF, &graph)?;
//! assert!((s.position_m.norm() - 6.365e6).abs() < 5e3);
//!
//! // Renaming an alias does not change identity.
//! reg.rename("dc:lhr:01/rack-07", "dc:lhr:01/hall-a/rack-07")?;
//! assert_eq!(reg.id_of("dc:lhr:01/hall-a/rack-07")?, rack);
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

mod alias;
mod error;
mod mount;
mod registry;

pub use alias::Alias;
pub use error::IdError;
pub use heavenly_core::LocationId;
pub use mount::{Attitude, Mount, Parent, TrajectoryFn};
pub use registry::{entity_frame, Entity, Key, Kind, MemoryRegistry, Registry};
