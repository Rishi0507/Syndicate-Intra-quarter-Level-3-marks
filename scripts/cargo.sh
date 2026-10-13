#!/usr/bin/env bash
# Runs cargo against the kernel manifest from anywhere in the repository.
#   scripts/cargo.sh test --lib
#   scripts/cargo.sh build --release
#
# A toolchain installed outside the default location is picked up from
# CARGO_HOME and RUSTUP_HOME when they are set.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$here/kernel/target}"
if [ -n "${CARGO_HOME:-}" ] && [ -d "$CARGO_HOME/bin" ]; then
  export PATH="$CARGO_HOME/bin:$PATH"
fi

exec cargo "$@" --manifest-path "$here/kernel/Cargo.toml"
