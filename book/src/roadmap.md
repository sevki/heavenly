# Roadmap

| Milestone | Crates | Status |
|-----------|--------|--------|
| **M1** Core types and invariants | `heavenly-core` | ✅ done |
| **M2** Earth and Moon | `heavenly-geo`, `heavenly-spice` | ✅ done |
| **M3** Moving objects | `heavenly-orbit` | planned |
| **M4** Local frames and identity | `heavenly-id` | planned |
| **M5** Spatial and operational queries | `heavenly-topology` | planned |

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
