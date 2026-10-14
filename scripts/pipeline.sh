#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
mode="${1:-synthetic}"

step() { printf '\n== %s\n' "$1"; }

step "ingest (Go)"
go build -C ingest -o ../bin/ingest.exe ./cmd/ingest

case "$mode" in
  synthetic)
    ./bin/ingest.exe synth -out data/synth
    build=data/synth/build
    ;;
  sec)
    ./bin/ingest.exe fetch-fred -out data/raw/fred
    if [ -n "${SEC_CONTACT:-}" ]; then
      ./bin/ingest.exe fetch-sec -out data/raw/sec
      ./bin/ingest.exe fetch-edgar -sec data/raw/sec -cache data/raw/edgar
    elif ! ls data/raw/sec/*_bdc.zip >/dev/null 2>&1; then
      echo "No archives in data/raw/sec and SEC_CONTACT is not set. See the top of this script." >&2
      exit 1
    fi
    ./bin/ingest.exe build -sec data/raw/sec -fred data/raw/fred -out data/build
    build=data/build
    ;;
  *)
    echo "usage: scripts/pipeline.sh [synthetic|sec]" >&2
    exit 2
    ;;
esac

step "kernel (Rust): native and WebAssembly"
bash scripts/cargo.sh build --release --quiet
bash scripts/cargo.sh build --release --quiet --lib --target wasm32-unknown-unknown
cp kernel/target/wasm32-unknown-unknown/release/syndicate_kernel.wasm web/public/

step "fit, backtest, export"
./kernel/target/release/syndicate.exe export "$build" web/public/data

step "browser against native"
(cd web && node scripts/check-kernel.mjs "../$build")

printf '\nData is in web/public/data. Run the site with: cd web && npm run dev\n'
