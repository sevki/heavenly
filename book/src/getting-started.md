# Getting started

`heavenly` is not yet published to crates.io. Depend on it from Git:

```toml
[dependencies]
heavenly-core = { git = "https://github.com/sevki/heavenly" }
```

## A first example

This example places a rack in a facility frame and resolves where it is in
the surrounding site frame:

```rust,ignore
{{#include ../../heavenly-core/examples/rack.rs:rack}}
```

The code samples in this book come from `heavenly-core/examples/`, and CI
builds and runs all of them. Try one locally:

```sh
cargo run -p heavenly-core --example rack
```
