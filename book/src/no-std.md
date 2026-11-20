# `no_std` and features

The `heavenly` crate is `no_std` with default features off. Its only
external dependency is [`libm`](https://crates.io/crates/libm). Enable just
the modules you need:

```toml
heavenly = { version = "0.5", default-features = false, features = ["geo", "orbit"] }
```

| Feature | Default | Enables | Needs a heap |
|---------|---------|---------|--------------|
| `std` | yes | `std::error::Error` impls, `Spk::open` / `Pck::open`; implies `alloc` | yes |
| `alloc` | via `std` | `FrameGraph`, `SampledTrajectory`, `find_windows`, `register` helpers | yes |
| `geo` | yes | `heavenly::geo` | no |
| `orbit` | yes | `heavenly::orbit` | no (`SampledTrajectory` with `alloc`) |
| `query` | yes | `heavenly::query` | no (`find_windows` with `alloc`) |
| `spice` | yes | `heavenly::spice`; kernels are held in memory | yes |
| `id` | yes | `heavenly::id` (implies `geo`) | yes |
| `topology` | yes | `heavenly::topology` | yes |
| `jetstream` | **no** | `heavenly::wire`: compact binary encoding ([Wire format](wire.md)) | yes (std) |

Without `std`, `heavenly::spice` still reads kernels: load the bytes
yourself and use `Spk::from_bytes` / `Pck::from_bytes`.

CI builds every module for two bare-metal targets, `thumbv7em-none-eabihf`
(Cortex-M) and `riscv32imac-unknown-none-elf`, with and without `alloc`, so
`no_std` support is tested.

The minimum supported Rust version is **1.70** for every feature except
`jetstream`, which needs what jetstream itself needs (currently Rust 1.91).
CI checks both.
