# Roadmap

Each milestone is released as the next minor version; see `CHANGELOG.md`.

| Milestone | In `heavenly` | Status |
|-----------|---------------|--------|
| **M1** Core types and invariants | crate root | ✅ done (0.1) |
| **M2** Earth and Moon | `heavenly::geo`, `heavenly::spice` | ✅ done (0.2) |
| **M3** Moving objects | `heavenly::orbit`, `spice::SpkLocator` | ✅ done (0.3) |
| **M4** Local frames and identity | `heavenly::id` | ✅ done (0.4) |
| **M5** Spatial and operational queries | `heavenly::query`, `heavenly::topology` | ✅ done (0.5) |
| **M6** TLEs and SGP4 | `orbit::Tle`, `orbit::Sgp4`, `FrameId::TEME` | ✅ done (0.6) |
| **M7** Earth orientation data, sunlight, batch operations | `geo::eop`, `query::{AnalyticSun, eclipses}`, `*_batch` with SIMD, `id::Registry` trait | ✅ done (0.7) |
| **M8** Catalogues and constellations | `orbit::{Omm, Catalog}`, `query::catalog`, `design::walker_delta` | ✅ done (0.8) |
| **M9** Perturbations | `orbit::{NumericalPropagator, ForceModel, SecularJ2Locator, analytic}`, `query::AnalyticMoon` | ✅ done (unreleased) |
| **M10** Covariance and collision probability | `Covariance`, `UncertainState`, `orbit::uncertainty`, `query::collision_probability` | ✅ done (unreleased) |

## Open design questions

These questions from the [design proposal](design.md) are still open:

1. Global namespacing of frame IDs (NAIF codes + custom namespaces?).
2. Whether `FrameResolver` should be object-safe or async, or split into a sync
   core and an async loading layer.
3. The canonical time scale for serialised states.
4. Validity intervals and accuracy bounds on transforms.
5. A URI scheme for objects, frames and coordinates.
6. Scope of relativistic support.

Settled: covariance attached to states (M10). A covariance travels beside
the state as an `UncertainState`, not inside `StateVector`, so that exact
states stay small and `Copy` and their struct literals keep working; it
follows the state through frames (`to_frame`) and time
(`propagate_uncertain`).
