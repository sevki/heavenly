# Contributing

Before opening a pull request, run the same checks CI does:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
cargo build -p heavenly-core --no-default-features --target thumbv7em-none-eabihf
mdbook build book
```

Code samples in this book are included from `heavenly-core/examples/` using
`{{#include}}` anchors. Add new samples there, not inline, so CI compiles and
runs them.
