# Wire format

To send states between processes or store them compactly, enable the
`jetstream` feature:

```toml
heavenly = { git = "https://github.com/sevki/heavenly", features = ["jetstream"] }
```

The feature is on `main` and ships in the next release. The published
0.5.0 crates predate it, so until then depend on Git as above.

`heavenly::wire` then implements
[jetstream](https://github.com/sevki/jetstream)'s `WireFormat` for the core
types. Every value has a fixed size and a little-endian layout, so there are
no field names, no schema negotiation and no allocation per value:

| Type | Bytes | Layout |
|------|------:|--------|
| `Epoch` | 16 | `i128` nanoseconds of TT since J2000.0 |
| `FrameId` | 12 | `u32` namespace, `i64` code |
| `LocationId` | 16 | `u128` |
| `Vec3` | 24 | `f64` x, y, z |
| `Rotation` | 32 | `f64` quaternion w, x, y, z |
| `Transform` | 104 | rotation, translation (m), origin velocity (m/s), angular velocity (rad/s) |
| `StateVector` | 77 | `u8` version (1), frame, epoch, position (m), velocity (m/s) |

```rust,ignore
use heavenly::wire::WireFormat;

let mut bytes = Vec::new();
state.encode(&mut bytes)?;                         // 77 bytes
let back = StateVector::decode(&mut &bytes[..])?;  // bit-for-bit identical
```

The design proposal asks that a serialised state carry its frame, epoch,
units and version, and this layout does. The frame is explicit, the epoch is
an exact instant (TT nanoseconds, so there is no time-scale ambiguity), the
units are fixed SI, and `StateVector` starts with a version byte.

Decoding checks what it reads. A frame in the reserved namespace must be a
known one, a quaternion must be normalisable, an unknown `StateVector`
version is rejected, and truncated input is an error rather than a panic.

The feature is off by default because it needs `std`, jetstream's
dependencies, and jetstream's minimum Rust version (currently 1.91).
Everything else in `heavenly` stays `no_std`-capable with only `libm`.
