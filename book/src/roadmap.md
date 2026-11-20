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

## Open design questions

These questions from the [design proposal](design.md) are still open:

1. Global namespacing of frame IDs (NAIF codes + custom namespaces?).
2. Whether `FrameResolver` should be object-safe or async, or split into a sync
   core and an async loading layer.
3. The canonical time scale for serialised states.
4. Covariance attached to states.
5. Validity intervals and accuracy bounds on transforms.
6. A URI scheme for objects, frames and coordinates.
7. Scope of relativistic support.
