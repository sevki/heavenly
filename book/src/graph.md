# Frame graphs and locators

## `FrameResolver`

Anything that can produce a `FrameTransform` between two frames at an epoch
implements `FrameResolver`. That includes `FrameGraph` today, and ephemeris
adapters and remote services later.

```rust,ignore
pub trait FrameResolver {
    type Error: From<heavenly_core::Error>;
    fn transform(&self, source: FrameId, destination: FrameId, epoch: Epoch)
        -> Result<FrameTransform, Self::Error>;
}
```

## `FrameGraph`

Nodes are frames and edges are known transforms. Each edge is registered as
*child → parent* and can be traversed in either direction. The resolver finds
the shortest path and composes the transforms along it.

```text
SSB_ICRF
├── GCRF
│   ├── ITRF
│   │   └── London site → building → room → rack
│   └── satellite body → module → rack
└── MOON_ICRF
    └── MOON_PA
        └── lunar facility
```

Edges can be time-dependent closures, so an edge may be unavailable at some
epochs. Failures are explicit:

| Error | When |
|-------|------|
| `NoPath` | no chain of edges connects the frames |
| `OutOfRange` | an edge exists but can't be evaluated at that epoch (e.g. kernel coverage) |
| `DuplicateEdge` | the same pair was registered twice |

## Locators

A `Locator` resolves an object's state in any frame at any epoch.
`FixedLocator` covers anything at rest in its own frame, such as a rack,
antenna or building. Orbit and ephemeris locators are planned for M3.

`LocationRecord { id, locator }` pairs a stable `LocationId` with its
locator, keeping identity separate from position.
