# Identity and local frames

> **Identity is stable; position is time-dependent.**

A rack is moved to another hall. A data centre is renamed. A spacecraft
module is swapped. None of these should change *which thing* we are talking
about. `heavenly-id` keeps three things separate:

| Concept | Type | Changes when… |
|---------|------|---------------|
| Identity | `LocationId` (128-bit, UUID-compatible) | never |
| Name | `Alias`, e.g. `dc:lhr:01/hall-2/rack-07` | it is renamed or re-pointed |
| Position | the entity's `Mount` and local frame | it moves, or time passes |

```toml
heavenly-id = { git = "https://github.com/sevki/heavenly" }
```

<figure class="hv-figure">
{{#heavenly-diagram registry}}
<figcaption>Aliases name IDs, IDs name entities, and each entity's mount attaches its frame to a parent frame.</figcaption>
</figure>

## A registry

This is the design proposal's own example: a rack in a London data centre
and a rack on an orbiting satellite, resolved by alias and compared.

```rust,ignore
{{#include ../../heavenly-id/examples/racks.rs:registry}}
```

Running the example (`cargo run -p heavenly-id --example racks`) prints the
current separation and relative speed. It also finds the closest approach
over the next day: 441 km, on an overhead pass.

## Mounts

Every entity says how its local frame attaches to its parent:

| Mount | For | Frame |
|-------|-----|-------|
| `Mount::Fixed` (`in_entity`, `in_frame`) | buildings, rooms, racks, devices, spacecraft modules | a fixed rotation and offset in the parent's frame |
| `Mount::Geodetic` | sites on the Earth, Moon or Mars | east-north-up at a geodetic position in a body-fixed frame |
| `Mount::Trajectory` | spacecraft and other moving platforms | origin follows a trajectory function; axes follow an `Attitude` |
| `Mount::None` | regions, fleets, other logical groups | none (`locate` returns `NotLocatable`) |

`Attitude::Inertial` keeps a platform's axes fixed in the inertial frame.
`Attitude::Lvlh` is local vertical, local horizontal: z towards the centre,
y opposite the orbit normal, and x along-track. Its angular velocity is
`(r × v)/|r|²`, which is exact for two-body motion. The tests check the
velocity of a point fixed in an LVLH frame against finite differences.

A trajectory is any `Fn(Epoch) -> Result<(Vec3, Vec3), Error>`: a
`ConicElements`, a `SampledTrajectory`, an SPK body, or your own propagator.
Errors such as `OutsideCoverage` reach the caller of `locate` unchanged.

## Frames and the graph

Each locatable entity gets a frame in the registry's namespace. Its code is
derived from the `LocationId`, so the same entity maps to the same `FrameId`
in every registry and every run. `register_frames` adds one edge per entity
to a `FrameGraph`, which joins the registry to the Earth, Moon and ephemeris
frames registered by the other crates.

Problems are reported, not guessed around:

| Error | When |
|-------|------|
| `DuplicateId`, `DuplicateAlias` | something is registered twice |
| `UnknownId`, `UnknownAlias` | a key names nothing |
| `MissingParent` | an entity is mounted in an unregistered parent |
| `Cycle` | an entity is mounted, directly or indirectly, in itself |
| `NotLocatable` | the entity (or its parent) is a logical group with no frame |
| `FrameCollision` | two IDs fold to one frame code (a 1-in-2⁶³ event, still checked) |

## Aliases

Aliases are lowercase segments separated by `:`, `.` or `/`, for example
`dc:lhr:01`, `earth.surface.gb-lon.01` or `sat:example:42/module-2/rack-1`.
They are names only. Nothing about position is derived from an alias, and
`rename`, `repoint` and `unalias` never touch an ID or a frame.

`LocationId::parse` accepts both its own 32-hex-digit form and a hyphenated
UUID, so IDs from a database or the `uuid` crate (via `from_bytes`) round-trip.
