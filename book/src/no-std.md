# `no_std` and features

`heavenly-core` and `heavenly-geo` are `no_std` and allocation-free with
default features off. Their only external dependency is
[`libm`](https://crates.io/crates/libm).

| Feature | Default | Enables |
|---------|---------|---------|
| `std` | yes | `std::error::Error` for `Error`; implies `alloc` |
| `alloc` | no | `FrameGraph`, and `heavenly-geo`'s `register` helpers |

`heavenly-spice` always needs `alloc` (kernels are held in memory). Its `std`
feature (default) adds `Spk::open` and `Pck::open`; without it, load bytes
yourself and use `from_bytes`.

```toml
heavenly-core = { git = "https://github.com/sevki/heavenly", default-features = false }
```

CI builds both crates for `thumbv7em-none-eabihf`, a bare-metal Cortex-M
target, with and without `alloc`, so `no_std` support is tested.

The minimum supported Rust version is **1.70**, and CI checks it.
