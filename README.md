<p align="center">
  <img src="book/src/images/logo.png" alt="heavenly logo: a gridded globe with a moon, a planet and an orbit" width="200">
</p>

# heavenly

[![crates.io](https://img.shields.io/crates/v/heavenly.svg)](https://crates.io/crates/heavenly) [![docs.rs](https://img.shields.io/docsrs/heavenly)](https://docs.rs/heavenly)

**[Book](https://sevki.github.io/heavenly/)** · **[Playground](https://sevki.github.io/heavenly/playground.html)** (the library in your browser, as WebAssembly) · **[API docs](https://sevki.github.io/heavenly/api/heavenly/)** · [Changelog](CHANGELOG.md)

A Rust library for describing, transforming and resolving physical locations
across celestial bodies and moving reference frames, from a rack in London to
a lunar base or a spacecraft in transit.

> **Identity is stable; position is time-dependent; reference frames provide
> meaning; placement topology is policy.**

```toml
[dependencies]
heavenly = "0.8"
```

```rust
use heavenly::geo::{earth, Ellipsoid, Geodetic};
use heavenly::orbit::{gm, ConicElements, TwoBodyLocator};
use heavenly::query::look_angles;
use heavenly::*;

let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
let mut frames = FrameGraph::new();
earth::register(&mut frames, earth::EarthOrientation::ZERO)?;

// A ground station in London (east-north-up) and a satellite in a 420 km orbit.
let london = FrameId::custom(Namespace(1), 1).unwrap();
let site = Geodetic::from_degrees(51.5072, -0.1276, 35.0)?;
frames.add_fixed(london, FrameId::ITRF, site.enu_to_body_fixed(&Ellipsoid::WGS84))?;
let sat = TwoBodyLocator {
    frame: FrameId::GCRF,
    elements: ConicElements {
        periapsis_m: 6_798_137.0, eccentricity: 0.0005, inclination_rad: 51.64f64.to_radians(),
        ascending_node_rad: 3.0, argument_of_periapsis_rad: 0.0, mean_anomaly_rad: 0.0,
        epoch, gm_m3_s2: gm::EARTH,
    },
};
let look = look_angles::<_, _, Error>(london, &sat, epoch, &frames)?;
```

## What's in it

The core types (`Epoch`, `FrameId`, `Transform`, `StateVector`, `FrameGraph`,
`Locator`, `LocationId`, …) are at the crate root. Everything else is a
module behind a cargo feature, all on by default:

| Module | What it does | Book |
|--------|--------------|------|
| crate root | epochs (UTC with leap seconds, TAI, TT, TDB), frames, transforms, frame graph, locators | [Concepts](https://sevki.github.io/heavenly/concepts.html) |
| `heavenly::geo` | ellipsoids, geodetic coordinates, ENU site frames, Earth orientation (ITRF ↔ GCRF) with IERS EOP tables, IAU Moon/Mars models | [Earth, Moon and Mars](https://sevki.github.io/heavenly/earth-and-moon.html) |
| `heavenly::spice` | dependency-free reader for JPL SPK / binary PCK kernels (DE440, lunar orientation) | [Ephemerides](https://sevki.github.io/heavenly/spice.html) |
| `heavenly::orbit` | two-body propagation, orbital elements, sampled trajectories with explicit coverage and gap errors; TLEs, OMMs and SGP4/SDP4; catalogues; J2 secular motion and numerical propagation with J2–J6, drag, radiation pressure, the Sun and the Moon | [Orbits](https://sevki.github.io/heavenly/orbits.html), [Perturbations](https://sevki.github.io/heavenly/perturbations.html), [SGP4](https://sevki.github.io/heavenly/sgp4.html), [Catalogues](https://sevki.github.io/heavenly/catalogs.html) |
| `heavenly::id` | stable identities, aliases, and a registry of facilities, racks and spacecraft with their own frames | [Identity](https://sevki.github.io/heavenly/identity.html) |
| `heavenly::query` | light time, look angles, line of sight past the Earth or Moon, visibility windows, the Sun, shadows and eclipses | [Signals](https://sevki.github.io/heavenly/signals.html) |
| `heavenly::topology` | failure domains and jurisdictions, as policy rather than coordinates | [Topology](https://sevki.github.io/heavenly/topology.html) |

## Checked against the references

Every accuracy figure in the book is measured in the test suite against SOFA
(IAU), SPICE (JPL), Vallado's SGP4 or Skyfield. CI's conformance job
regenerates the reference values from those libraries, checks them against
the committed copies, and runs the comparisons on Linux x64 and arm64:

| Check | Reference | Worst difference |
|-------|-----------|------------------|
| Earth orientation, 1980–2050 | SOFA `c2t06a` (IAU 2006/2000A) | 7.6 mas (≈0.24 m) |
| DE440 planetary ephemeris | SPICE `spkgeo` | 7e-5 m |
| DE440 lunar orientation | SPICE `sxform` | 5e-12 rad |
| Two-body propagation | SPICE `prop2b` | 3.4e-12 relative |
| Light time, Moon / Sun / Mars | SPICE `spkezr` CN / XCN | 4.7e-11 s |
| Eclipse edges (LEO, GEO, HEO) | SPICE `gfoclt` | 39 ms |
| SGP4/SDP4, official verification set | Vallado's C++ | 1.5e-9 km |
| SGP4 from OMM (KVN, JSON, CSV) | Vallado's C++ via `sgp4.omm` | 2.8e-10 km |
| TEME → ITRF | Skyfield | 5.9e-9 m |
| Numerical propagation, J2–J6, 1 day (LEO) | independent RK4 implementation | 2 mm |
| Analytic Moon, 1950–2050 | DE440 via SPICE | 0.08° |
| Analytic Earth pole, 1950–2050 | SOFA `xy06` | 0.17″ |

## `no_std`

`heavenly` is `no_std` with `default-features = false`. Its only external
dependency is `libm`, and CI builds it for Cortex-M and RISC-V bare-metal
targets and for `wasm32-unknown-unknown`, where it also runs the
conformance suite (in Node, via `wasm-pack`). MSRV is Rust 1.70. See
[`no_std` and features](https://sevki.github.io/heavenly/no-std.html).

## Repository layout

`heavenly/` is the crate you depend on. It re-exports the internal
`heavenly-*` crates as modules and holds the examples (`cargo run -p heavenly
--example placement`). The book's source is in [`book/`](book/src/SUMMARY.md)
(`mdbook serve book`). The design proposal is
[`docs/design.md`](docs/design.md).

## Versioning

One version for the whole workspace. The minor version increases with each
roadmap milestone (currently **0.8.0** = M8). See [`CHANGELOG.md`](CHANGELOG.md).

## License

MIT OR Apache-2.0
