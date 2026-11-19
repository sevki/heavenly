# `no_std` and features

`heavenly-core` is `no_std` and allocation-free with default features off. Its
only dependency is [`libm`](https://crates.io/crates/libm).

| Feature | Default | Enables |
|---------|---------|---------|
| `std` | yes | `std::error::Error` for `Error`; implies `alloc` |
| `alloc` | no | `FrameGraph` |

```toml
heavenly-core = { git = "https://github.com/sevki/heavenly", default-features = false }
```

CI builds the crate for `thumbv7em-none-eabihf`, a bare-metal Cortex-M
target, with and without `alloc`, so `no_std` support is tested.

The minimum supported Rust version is **1.70**, and CI checks it.
