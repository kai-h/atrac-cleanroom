#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
if [ -x "$root/.toolchain/cargo/bin/cargo" ]; then
  export RUSTUP_HOME="$root/.toolchain/rustup"
  export CARGO_HOME="$root/.toolchain/cargo"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
cargo build --locked --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir encoders/atrac-rust target/wasm32-unknown-unknown/release/atrac_cleanroom.wasm
cp web/api.js encoders/atrac-rust/api.js
node encoders/atrac-rust/package.mjs
