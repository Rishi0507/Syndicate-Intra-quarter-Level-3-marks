#!/usr/bin/env bash
# Builds everything from source and produces the site's data.
#
#   scripts/pipeline.sh synthetic    generated universe with a known answer (default)
#   scripts/pipeline.sh sec          real SEC filings
#
# For real filings, either set SEC_CONTACT to a name and an email address you
# can be reached at (the SEC requires automated clients to identify
# themselves), or download the *_bdc.zip archives yourself from
# https://www.sec.gov/data-research/sec-markets-data/bdc-data-sets
# into data/raw/sec.
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
