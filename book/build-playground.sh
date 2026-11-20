#!/usr/bin/env bash
# Builds the playground's WebAssembly module into the book's source tree.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p heavenly-viz --profile wasm --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/wasm/heavenly_viz.wasm book/src/playground/
echo "built book/src/playground/heavenly_viz.wasm ($(wc -c < book/src/playground/heavenly_viz.wasm) bytes)"
