# Heavenly: A Universal Spatiotemporal Coordinate Library for Rust

**Project:** Heavenly (`heavenly`)  
**Name rationale:** Named for heavenly bodies; the API covers terrestrial and artificial reference frames as well as celestial ones.  
**Status:** Design proposal  
**Language:** Rust  
**Scope:** Terrestrial, orbital, planetary, and deep-space infrastructure

## Abstract

`heavenly` is a Rust library for describing, transforming, and resolving physical locations across celestial bodies and moving reference frames. It separates the stable identity of an object from its position at a particular time, its reference frame, and its operational placement topology.

The library should support locations ranging from a server rack in London to a lunar base, an Earth-orbiting satellite, or a spacecraft in transit. It should not impose a single universal XYZ origin or confuse cloud regions with physical locations.

## Motivation

Cloud infrastructure naming schemes such as `us-east-1c`, `lhr1`, and `fra1` encode operational or geographic conventions. They are useful labels but are not universal coordinates. They cannot reliably describe moving platforms, extraterrestrial facilities, or arbitrary nested local frames.

The central abstraction is:

> A physical state expressed relative to a reference frame at a specified epoch.

A coordinate is not an identity. An object's position can change while its identity remains constant. A scheduling region is also not necessarily a geographic region.

## Goals

- Represent positions and velocities in explicitly identified reference frames.
- Associate every time-dependent state with an epoch and time scale.
- Transform states between terrestrial, celestial, and user-defined local frames.
- Resolve stationary and moving objects through one location-provider interface.
- Support hierarchical frames, including building, room, rack, and device frames.
- Support interoperability with astronomical ephemerides and planetary reference frames.
- Keep stable object identifiers separate from coordinates and cloud placement domains.
- Offer a small `no_std`-friendly core where practical.

## Non-goals

- Replacing established astronomical frame definitions or ephemeris standards.
- Guaranteeing a universally valid, high-precision Euclidean frame across arbitrary interstellar distances.
- Embedding legal jurisdiction or cloud availability-zone semantics into coordinates.
- Deriving an object's trajectory from its name alone.
- Treating illustrative orbital positions as real ephemeris measurements.

## Conceptual model

### Identity

An immutable `LocationId` identifies a facility, spacecraft, rack, or other object. Human-readable aliases such as `dc:lhr:01` are registry-managed names and can change without changing identity.

### Epoch

Every dynamic position is evaluated at an explicit time. Astronomical transformations may require conversion between UTC, TAI, TT, and TDB, with appropriate time-scale data.

### Reference frame

A reference frame defines an origin, axes, and their evolution with time. Examples include Earth-fixed frames, Earth-centered inertial frames, Moon-fixed frames, and custom frames attached to a spacecraft or building.

### State

A state comprises position and velocity relative to a frame at an epoch. The API must specify units and transformation conventions.

### Locator

A locator resolves an object's state at a requested epoch. It may use fixed geodetic coordinates, an orbital propagator, an ephemeris kernel, or a custom motion model.

### Placement topology

Operational regions and failure domains are separate metadata. Two physically distant nodes can share a failure dependency; a single logical region may span multiple physical locations.

## Proposed crate layout

```text
heavenly/
├── heavenly-core/      # Frames, epochs, state vectors, transforms, traits
├── heavenly-geo/       # Geodetic coordinates, ellipsoids, Earth-fixed frames
├── heavenly-spice/     # SPICE-compatible ephemeris/frame adapter
├── heavenly-orbit/     # Trajectories and orbital propagation
├── heavenly-id/        # Stable IDs, aliases, registry interfaces
└── heavenly-topology/  # Optional scheduling and failure-domain metadata
```

## Sketch of the Rust API

The following is an **illustrative API design**, not compilable code from an existing crate.

```rust
use heavenly_core::{Epoch, FrameId, LocationId, StateVector, Transform};

pub trait FrameResolver {
    type Error;

    fn transform(
        &self,
        source: FrameId,
        destination: FrameId,
        epoch: Epoch,
    ) -> Result<Transform, Self::Error>;
}

pub trait Locator {
    type Error;

    fn state_at(
        &self,
        epoch: Epoch,
        frame: FrameId,
        resolver: &impl FrameResolver,
    ) -> Result<StateVector, Self::Error>;
}

pub struct LocationRecord<L> {
    pub id: LocationId,
    pub locator: L,
}
```

For compile-time frame safety, a typed coordinate can carry a phantom frame marker:

```rust
use core::marker::PhantomData;

pub struct Coordinate<F> {
    pub position_m: [f64; 3],
    pub velocity_m_s: [f64; 3],
    pub epoch: Epoch,
    _frame: PhantomData<F>,
}

pub enum EarthFixed {}
pub enum MoonFixed {}
pub enum Icrf {}
```

Arithmetic between differently typed frames should be disallowed unless an explicit transformation is performed. Dynamically registered frames require equivalent runtime checks.

## Frame transformations

For a position, an affine rigid-frame transformation is:

\[
\mathbf{p}_B = R_{BA}\mathbf{p}_A + \mathbf{t}_{BA}
\]

Velocity transformation also requires the relative translational and angular velocity of the frames. Transform composition must define precisely which frame expresses translation, angular velocity, and derivatives; otherwise chained transforms can silently produce incorrect results.

```rust
pub struct Transform {
    pub rotation: UnitQuaternion<f64>,
    pub translation_m: Vector3<f64>,
    pub origin_velocity_m_s: Vector3<f64>,
    pub angular_velocity_rad_s: Vector3<f64>,
}
```

The transform graph is evaluated at an epoch. Nodes represent frames; edges represent known, possibly time-dependent transformations. A resolver finds a valid path and composes the transforms. Missing kernels, out-of-range ephemerides, and unsupported frame pairs must produce explicit errors.

## Example reference-frame hierarchy

```text
Solar System barycentric frame
├── Earth-centered inertial frame
│   ├── Earth-fixed frame
│   │   └── London facility frame
│   │       └── Building → Room → Rack → Server
│   └── Satellite body frame
│       └── Module → Rack → Server
├── Moon-centered inertial frame
│   └── Moon-fixed frame
│       └── Lunar facility frame
└── Mars-centered inertial frame
    └── Mars-fixed frame
```

This is a conceptual graph, not a claim that all frames have a unique parent or that every frame transformation can be computed without external data.

## Example usage

```rust
// Illustrative API; not yet implemented.
let epoch = Epoch::parse("2026-11-19T17:00:00Z")?;
let london = registry.resolve("dc:lhr:01")?;
let satellite = registry.resolve("sat:example:42")?;

let a = london.state_at(epoch, FrameId::ICRF, &frames)?;
let b = satellite.state_at(epoch, FrameId::ICRF, &frames)?;

let separation_m = (a.position_m - b.position_m).norm();
let relative_velocity_m_s = (a.velocity_m_s - b.velocity_m_s).norm();
```

A geodetic position on Earth and one on the Moon must each be converted through their respective body-fixed and inertial frames before comparison. Resolving a moving spacecraft requires an appropriate trajectory source.

## Serialization and identifiers

A serialized state should explicitly encode the frame, epoch, units, and schema version:

```json
{
  "schema": "heavenly.state.v1",
  "object": "sat:example:42",
  "frame": "ICRF",
  "epoch": { "value": "2026-11-19T17:00:00", "scale": "UTC" },
  "position": { "xyz": [6800000.0, 0.0, 0.0], "unit": "m" },
  "velocity": { "xyz": [0.0, 7600.0, 0.0], "unit": "m/s" }
}
```

These values are illustrative and do not represent an actual satellite state. The schema must also define the exact interpretation of `ICRF`, including origin and conventions, rather than relying on an ambiguous shorthand.

Human-readable paths such as `earth.surface.gb-lon.01` may be exposed as aliases, but should not be treated as mathematically complete coordinates or permanent object identities.

## Precision and physical limitations

- Use SI units internally, with typed conversions at boundaries.
- Prefer local hierarchical frames for millimetre-scale work inside facilities and spacecraft.
- Track numerical precision and ephemeris accuracy separately.
- Explicitly document coordinate time scales and transformation model assumptions.
- For Solar System applications, use established barycentric and body-centered conventions.
- For interstellar and relativistic applications, avoid promising a globally preferred Euclidean coordinate frame; provide extensible reference-frame and metric abstractions if that scope is later required.
- Distance between positions at a common epoch is not the same as signal delay, line-of-sight availability, or relativistic light-time calculation.

## Ecosystem integration

Candidate dependencies and adapters to evaluate:

- **ANISE:** Rust astrodynamics, ephemeris, and frame functionality.
- **hifitime:** high-precision epochs and astronomical time scales.
- **nalgebra:** vectors, rotations, and transformation mathematics.
- **SPICE kernels:** authoritative ephemeris and orientation inputs where available.

These are integration candidates; their APIs and coverage should be validated during implementation.

## Proposed milestones

### M1 — Core types and invariants

Define typed units, epochs, frame IDs, state vectors, transformations, composition, and explicit error types. Test frame round trips and velocity transformation correctness.

### M2 — Earth and Moon

Implement geodetic-to-body-fixed conversion, reference ellipsoids, and transformations to inertial frames through a validated ephemeris adapter.

### M3 — Moving objects

Introduce `Locator` implementations backed by ephemerides and orbital propagation. Test states at multiple epochs and failure modes for missing trajectory data.

### M4 — Local frames and identity

Support frames attached to facilities, buildings, spacecraft, and racks. Introduce stable IDs, aliases, and registry resolution.

### M5 — Spatial and operational queries

Implement separation, relative velocity, optional light-time and visibility calculations, and a separate model for jurisdiction and failure-domain dependencies.

## Open design questions

1. Should frame identity be globally namespaced using established astronomical IDs plus custom namespaces?
2. Should `FrameResolver` be object-safe, asynchronous, or split into a synchronous core and async data-loading layer?
3. Which time scale should the canonical state representation use?
4. Should uncertainty be represented as covariance attached to a state?
5. How should transforms advertise validity intervals and accuracy bounds?
6. Should a universal URI scheme identify objects, reference frames, coordinates, or distinct resources for each?
7. What level of relativistic support belongs in the initial scope?

## Design principle

**Identity is stable; position is time-dependent; reference frames provide meaning; placement topology is policy.**

Keeping these concepts separate makes the same library applicable to a terrestrial data center, a moving orbital compute platform, a lunar installation, and future deep-space infrastructure.
