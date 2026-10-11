#!/usr/bin/env bash
# Two wasm artifacts. Bevy's webgpu feature replaces webgl2, so the page picks.
set -euo pipefail
cd "$(dirname "$0")/.."
profile="${PROFILE:-wasm-release}"
target_dir="${CARGO_TARGET_DIR:-target}"
if ! command -v wasm-bindgen >/dev/null 2>&1; then
  cargo install wasm-bindgen-cli --version 0.2.129 --locked
fi
wasm="${target_dir}/wasm32-unknown-unknown/${profile}/gemini-client.wasm"
rm -rf web/pkg-webgl2 web/pkg-webgpu web/assets
cargo build -p gemini-client --target wasm32-unknown-unknown --profile "$profile"
wasm-bindgen --target web --out-dir web/pkg-webgl2 "$wasm"
cargo build -p gemini-client --target wasm32-unknown-unknown --profile "$profile" --features webgpu
wasm-bindgen --target web --out-dir web/pkg-webgpu "$wasm"
mkdir -p web/assets
cp -a assets/audio web/assets/audio
echo "web/ ready. Serve that directory: python3 -m http.server 8080 -d web"
