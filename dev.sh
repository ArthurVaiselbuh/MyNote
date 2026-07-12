#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cd "$root/src/frontend"
[ -d node_modules ] || npm install --no-audit --no-fund

cd "$root/src/backend"
"$root/src/frontend/node_modules/.bin/tauri" dev
