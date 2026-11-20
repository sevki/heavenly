# Contributing

## Versioning

All crates share the workspace version in the root `Cargo.toml`. The minor
version increases by one with each roadmap milestone (M1 = 0.1, M2 = 0.2,
M3 = 0.3, …). Patch versions fix bugs within a milestone. Record every
change in `CHANGELOG.md` under the version it ships in.

## Checks

Before opening a pull request, run the same checks CI does:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
cargo build -p heavenly-core -p heavenly-geo --no-default-features --target thumbv7em-none-eabihf
mdbook build book   # also runs the diagram preprocessor
```

## Playground

`book/src/playground.md` runs `heavenly-viz` (in `viz/`) compiled to
WebAssembly. The physics belongs in `viz/src/lib.rs`, where it is
unit-tested natively. `book/src/playground/playground.js` only draws.
Build the module with `book/build-playground.sh`, then use `mdbook serve
book`; browsers will not load WebAssembly from `file://`. The `.wasm` is
built in CI and is not committed.

## Diagrams

The book's figures are drawn at build time by an mdbook preprocessor,
`heavenly-diagrams` in `diagrams/`, which is registered in `book/book.toml`.
A chapter writes `\{{#heavenly-diagram conics}}`, and the preprocessor
replaces it with an inline SVG computed by the heavenly crates. Nothing
generated is committed, so figures cannot drift from the code. An unknown
name fails the build. (`book/heavenly-preprocessor.sh` answers mdbook's `supports` probe
without compiling, because mdbook silently skips a preprocessor whose probe
fails; CI also checks that no directive survives into the HTML.) `cargo run -p heavenly-diagrams -- list` shows the
names, and `-- render <dir>` writes every figure to files for previewing.
