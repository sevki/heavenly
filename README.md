# heavenly

A Rust library for describing, transforming and resolving physical locations
across celestial bodies and moving reference frames — from a rack in London to
a lunar base or a spacecraft in transit.

> **Identity is stable; position is time-dependent; reference frames provide
> meaning; placement topology is policy.**

See [`docs/design.md`](docs/design.md) for the full design proposal.

## Status

| Milestone | Crate | State |
|-----------|-------|-------|
| M1 — Core types and invariants | `heavenly-core` | implemented |
| M2 — Earth and Moon | `heavenly-geo`, `heavenly-spice` | planned |
| M3 — Moving objects | `heavenly-orbit` | planned |
| M4 — Local frames and identity | `heavenly-id` | planned |
| M5 — Spatial and operational queries | `heavenly-topology` | planned |

## `heavenly-core`

* `Epoch` — an instant stored as integer nanoseconds of TT since J2000, with
  UTC (leap seconds, including `23:59:60`), TAI, TT and TDB conversions.
  Parsing requires an explicit time scale (`Z`, ` TAI`, ` TT`, ` TDB`).
* `FrameId` — namespaced frame identifiers; each frame names an origin *and*
  axes (e.g. `GCRF`, `ITRF`, `SSB_ICRF`, `MOON_PA`).
* `Transform` / `FrameTransform` — rigid, time-dependent transforms with an
  exactly specified velocity convention; composition and inversion are tested
  against finite differences.
* `StateVector` (runtime-checked frame) and `Coordinate<F>` (compile-time frame).
* `FrameResolver`, `FrameGraph` (path-finding resolver), `Locator`,
  `FixedLocator`, `LocationId`, `LocationRecord`.

The crate is `no_std` with `--no-default-features`; `alloc` enables
`FrameGraph`, `std` (default) adds `std::error::Error`.

```rust
use heavenly_core::*;

let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
let site = FrameId::custom(Namespace(1), 100).unwrap();
let rack_frame = FrameId::custom(Namespace(1), 101).unwrap();

let mut frames = FrameGraph::new();
frames.add_fixed(rack_frame, site, Transform::fixed(Rotation::IDENTITY, Vec3::new(10.0, 0.0, 0.0)))?;

let rack = FixedLocator { frame: rack_frame, position_m: Vec3::new(1.0, 2.0, 0.0) };
let state = rack.state_at(epoch, site, &frames)?;
```

## License

MIT OR Apache-2.0
