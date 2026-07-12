#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== MyNote tests ==="

cd "$root/src/frontend"
[ -d node_modules ] || npm install --no-audit --no-fund
echo "[1/2] svelte-check"
npm run check

cd "$root/src/backend"
echo "[2/2] cargo test"
cargo test

# e2e drives the real exe over WebView2's CDP endpoint, which only exists on
# Windows — run test.bat there for the full suite.
echo
echo "Backend + frontend checks passed (e2e is Windows-only; see test.bat)."
