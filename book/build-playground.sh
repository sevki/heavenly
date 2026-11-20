#!/usr/bin/env bash
# Builds the playground into the book's source tree:
#   1. wasm-pack compiles crate heavenly-viz into @heavenly/react (web/packages/react/wasm);
#   2. Vite builds the React app (web/playground) into book/src/playground/
#      (app.js and heavenly_viz_bg.wasm).
# Needs wasm-pack, the wasm32-unknown-unknown target and Node 20.19+.
set -euo pipefail
cd "$(dirname "$0")/../web"
npm ci --no-audit --no-fund
npm run wasm -w @heavenly/react
npm run build
ls -l ../book/src/playground/app.js ../book/src/playground/heavenly_viz_bg.wasm
