#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

printf '== ingest (Go)\n'
go vet -C ingest ./...
go test -C ingest ./...

printf '\n== kernel (Rust)\n'
bash scripts/cargo.sh test --lib --quiet

printf '\n== web (TypeScript)\n'
(cd web && npx tsc --noEmit && echo "types ok")

if [ -f web/public/data/meta.json ] && [ -f web/public/syndicate_kernel.wasm ]; then
  printf '\n== browser against native\n'
  (cd web && node scripts/check-kernel.mjs)
fi
