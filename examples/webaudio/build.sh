#!/usr/bin/env bash
# Build the fourier-core WASM package and output it into this example's pkg/ directory.
#
# Prerequisites:
#   - wasm-pack: https://rustwasm.github.io/wasm-pack/installer/
#     curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
#
# Usage:
#   cd examples/webaudio
#   ./build.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CORE_DIR="$SCRIPT_DIR/../../crates/core"
OUT_DIR="$SCRIPT_DIR/pkg"

echo "Building fourier-core WASM package..."
echo "  Source: $CORE_DIR"
echo "  Output: $OUT_DIR"
echo ""

# Build with no-modules target for AudioWorklet compatibility.
# AudioWorklet scopes cannot use ES module imports, so no-modules target
# creates a global `wasm_bindgen` function that works with importScripts().
wasm-pack build "$CORE_DIR" \
  --target no-modules \
  --features wasm \
  --out-dir "$OUT_DIR" \
  --out-name fourier_core

echo ""
echo "Build complete. Files in $OUT_DIR:"
ls -lh "$OUT_DIR"/*.{js,wasm} 2>/dev/null || true

echo ""
echo "To run the demo:"
echo "  cd $SCRIPT_DIR"
echo "  python3 -m http.server 8080"
echo "  open http://localhost:8080"
