#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

exe=""
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) exe=".exe" ;; esac

out="${1:-data/vintage}"
quarters="${QUARTERS:-2024-03 2024-06 2024-09 2024-12 2025-03 2025-06 2025-09 2025-12 2026-03 2026-06}"
mkdir -p "$out"

for q in $quarters; do
  until="$(date -u -d "$q-01 +1 month -1 day +105 days" +%F)"
  printf '\n== %s: panel as filed by %s\n' "$q" "$until"
  ./bin/ingest$exe build -sec data/raw/sec -fred data/raw/fred -out "$out/$q" -until "$until" 2>"$out/$q.log"
  ./kernel/target/release/syndicate$exe backtest "$out/$q" --only "$q" --out "$out/$q/backtest.json" --records "$out/$q/records.tsv" 2>>"$out/$q.log" | sed -n '2,8p'
done

printf '\nReports are in %s/<quarter>/backtest.json\n' "$out"
