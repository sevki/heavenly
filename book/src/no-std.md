# `no_std` and features

The `heavenly` crate is `no_std` with default features off. With default
features off, its only external dependency is
[`libm`](https://crates.io/crates/libm). The default `simd` feature adds
[`wide`](https://crates.io/crates/wide), which is also `no_std`. Enable just
the modules you need:

```toml
heavenly = { version = "0.9", default-features = false, features = ["geo", "orbit"] }
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
| `simd` | yes | SIMD for the batch operations, through [`wide`](https://crates.io/crates/wide). Results are bit-identical without it | no |
| `jetstream` | **no** | `heavenly::wire`: compact binary encoding ([Wire format](wire.md)) | yes (std) |

Without `std`, `heavenly::spice` still reads kernels: load the bytes
yourself and use `Spk::from_bytes` / `Pck::from_bytes`.

CI builds every module for two bare-metal targets, `thumbv7em-none-eabihf`
(Cortex-M) and `riscv32imac-unknown-none-elf`, with and without `alloc`, so
`no_std` support is tested.

## WebAssembly

`heavenly` builds for `wasm32-unknown-unknown` with or without `std`, and
needs no JavaScript glue. The [playground](playground.md) is the library
compiled to an 86 KB module with no imports. CI builds it for that target in
every feature configuration. It also runs the conformance suite there under
Node, with `wasm-pack test --node heavenly --test conformance`: the SOFA,
SPICE, Vallado SGP4 and Skyfield checks, so results in the browser are known
to match the references.

The minimum supported Rust version is **1.70** for every feature except
`jetstream`, which needs what jetstream itself needs (currently Rust 1.91).
CI checks both.
