# Reference frames

A `FrameId` names an **origin and a set of axes** together. This differs from
SPICE, where frame IDs name only axes and the origin is a separate body. A
SPICE adapter will map `(center, frame)` pairs onto `FrameId`s.

## Well-known frames

| Constant | Origin | Axes |
|----------|--------|------|
| `SSB_ICRF` | Solar System barycentre | ICRF (the BCRS) |
| `GCRF` | Earth centre of mass | ICRF |
| `ITRF` | Earth centre of mass | Earth-fixed (rotating) |
| `TEME` | Earth centre of mass | True equator, mean equinox of date (SGP4 output; see [TLEs and SGP4](sgp4.md)) |
| `MOON_ICRF` | Moon centre of mass | ICRF |
| `MOON_PA` | Moon centre of mass | Lunar principal axes |
| `MOON_ME` | Moon centre of mass | Lunar mean-Earth / polar axes |
| `MARS_ICRF` | Mars centre of mass | ICRF |
| `MARS_IAU` | Mars centre of mass | IAU Mars body-fixed |

There is no bare `ICRF` constant. "ICRF" names a set of axes, not an origin,
and leaving the origin unstated is the ambiguity the design rules out.

## Custom frames

Applications define their own frames in their own namespaces:

```rust,ignore
let building = FrameId::custom(Namespace(42), 1).unwrap();
let rack_7   = FrameId::custom(Namespace(42), 1007).unwrap();
```

Namespace `0` (`Namespace::HEAVENLY`) is reserved for built-in frames.

## Compile-time frames

Marker types (`EarthFixed`, `Gcrf`, `SsbIcrf`, `MoonFixed`, …) implement the
`Frame` trait. When you use them as the parameter of `Coordinate<F>`, mixing
frames becomes a compile error:

```rust,ignore
let a = Coordinate::<EarthFixed>::new(epoch, p, v);
let b = Coordinate::<Gcrf>::new(epoch, p, v);
a.separation_m(&b); // error[E0308]: mismatched types
```

To convert, transform explicitly with `a.to_frame::<Gcrf, _>(&resolver)`.
