#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "==> Building rv-web-bridge (wasm32-unknown-unknown)..."
cargo build --manifest-path "$ROOT_DIR/Cargo.toml" --target wasm32-unknown-unknown --release -p rv-web-bridge

echo "==> Running wasm-bindgen..."
mkdir -p "$ROOT_DIR/web/src/pkg"
wasm-bindgen --target web --out-dir "$ROOT_DIR/web/src/pkg" \
  "$ROOT_DIR/target/wasm32-unknown-unknown/release/rv_web_bridge.wasm"

echo "==> Building web frontend (tsc + vite build)..."
cd "$ROOT_DIR/web"
npm run build
echo "==> Done! Static bundle ready in web/dist/"
