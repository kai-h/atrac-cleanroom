#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
export RUSTUP_HOME="$root/.toolchain/rustup"
export CARGO_HOME="$root/.toolchain/cargo"
export PATH="$CARGO_HOME/bin:$PATH"
exec cargo "$@"
