# Introduction

<p align="center">
  <img src="images/logo.png" alt="heavenly logo: a gridded globe with a moon, a planet and an orbit" width="180">
</p>

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
{{#heavenly-diagram crates}}
<figcaption>You depend on <code>heavenly</code>; each module is a feature-gated crate built on the same core.</figcaption>
</figure>

## What exists today

Milestones **M1**–**M5** are complete (version 0.5). The `heavenly` crate provides:

| Type | Purpose |
|------|---------|
| [`Epoch`](time.md) | An instant, with UTC / TAI / TT / TDB conversion |
| [`FrameId`](frames.md) | A namespaced reference frame (origin *and* axes) |
| [`Transform`](transforms.md) | Rigid, time-dependent frame kinematics |
| `StateVector`, `Coordinate<F>` | Position + velocity in a frame at an epoch |
| [`FrameGraph`](graph.md) | Resolves transforms between any connected frames |
| `Locator`, `LocationId` | Stable identity and a way to find it at any epoch |

`heavenly::geo` adds ellipsoids, geodetic coordinates, Earth orientation
(ITRF ↔ GCRF) and IAU Moon/Mars models. See
[Earth, Moon and Mars](earth-and-moon.md). `heavenly::spice` reads JPL
ephemerides and lunar orientation from SPICE kernels. See
[Ephemerides](spice.md).

`heavenly::orbit` adds two-body propagation, orbital elements and sampled
trajectories. See [Orbits and trajectories](orbits.md).

`heavenly::id` adds stable identities, aliases and a registry of
facilities, racks and spacecraft with their own frames. See
[Identity and local frames](identity.md).

`heavenly::query` adds light time, look angles, line of sight and visibility
windows, and `heavenly::topology` models failure domains and jurisdictions
as policy. See [Signals and visibility](signals.md) and
[Placement topology](topology.md).

To see it running, try the [Playground](playground.md): the library
compiled to WebAssembly, computing orbits, ground tracks and visibility in
your browser.

See the [roadmap](roadmap.md) for what comes next, and the
[design proposal](design.md) for the full rationale.

API reference: [`heavenly`](./api/heavenly/index.html), published with this book.
