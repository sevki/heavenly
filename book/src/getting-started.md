# Getting started

`heavenly` is not yet published to crates.io. Depend on it from Git:

```toml
[dependencies]
heavenly = { git = "https://github.com/sevki/heavenly" }
```

That one crate gives you everything. The core types are at the root
(`heavenly::Epoch`, `heavenly::FrameGraph`, …), and each area is a module:

| Module | Cargo feature | Chapter |
|--------|---------------|---------|
| `heavenly::geo` | `geo` | [Earth, Moon and Mars](earth-and-moon.md) |
| `heavenly::spice` | `spice` | [Ephemerides (SPICE)](spice.md) |
| `heavenly::orbit` | `orbit` | [Orbits and trajectories](orbits.md) |
| `heavenly::id` | `id` | [Identity and local frames](identity.md) |
| `heavenly::query` | `query` | [Signals and visibility](signals.md) |
| `heavenly::topology` | `topology` | [Placement topology](topology.md) |

Every feature is on by default. To trim the build, or for `no_std`, see
[`no_std` and features](no-std.md).

## A first example

This example places a rack in a facility frame and resolves where it is in
the surrounding site frame:

```rust,ignore
{{#include ../../heavenly/examples/rack.rs:rack}}
```

The code samples in this book come from `heavenly/examples/`, and CI builds
and runs all of them. Try one locally:

```sh
cargo run -p heavenly --example rack
```
