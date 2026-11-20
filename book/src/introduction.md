# Introduction

`heavenly` is a Rust library for describing, transforming and resolving
physical locations across celestial bodies and moving reference frames. The
same API covers a server rack in London, a lunar base, an Earth-orbiting
satellite and a spacecraft in transit.

It rests on one principle:

> **Identity is stable; position is time-dependent; reference frames provide
> meaning; placement topology is policy.**

So `heavenly` never treats a name like `us-east-1c` or `lhr1` as a
coordinate. It never assumes a single universal XYZ origin, and it never
compares two positions unless they share a frame and an epoch.

<figure class="hv-figure">
{{#include diagrams/crates.svg}}
<figcaption>The crates. Each depends only on <code>heavenly-core</code> (and <code>libm</code>).</figcaption>
</figure>

## What exists today

Milestones **M1**–**M3** are complete (version 0.3). The `heavenly-core` crate provides:

| Type | Purpose |
|------|---------|
| [`Epoch`](time.md) | An instant, with UTC / TAI / TT / TDB conversion |
| [`FrameId`](frames.md) | A namespaced reference frame (origin *and* axes) |
| [`Transform`](transforms.md) | Rigid, time-dependent frame kinematics |
| `StateVector`, `Coordinate<F>` | Position + velocity in a frame at an epoch |
| [`FrameGraph`](graph.md) | Resolves transforms between any connected frames |
| `Locator`, `LocationId` | Stable identity and a way to find it at any epoch |

`heavenly-geo` adds ellipsoids, geodetic coordinates, Earth orientation
(ITRF ↔ GCRF) and IAU Moon/Mars models. See
[Earth, Moon and Mars](earth-and-moon.md). `heavenly-spice` reads JPL
ephemerides and lunar orientation from SPICE kernels. See
[Ephemerides](spice.md).

`heavenly-orbit` adds two-body propagation, orbital elements and sampled
trajectories. See [Orbits and trajectories](orbits.md).

See the [roadmap](roadmap.md) for what comes next, and the
[design proposal](design.md) for the full rationale.

API reference: [`heavenly_core` on docs](./api/heavenly_core/index.html) (when
built with the published site).
