#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "=== MyNote build ==="

case "$(uname -s)" in
  Darwin) exe_name="MyNote"; out_name="mynote-macos" ;;
  *)      exe_name="MyNote"; out_name="mynote-linux" ;;
esac

cd "$root/src/frontend"
echo "[1/2] npm install"
npm install --no-audit --no-fund

cd "$root/src/backend"
echo "[2/2] tauri build (runs frontend build, then cargo release)"
"$root/src/frontend/node_modules/.bin/tauri" build --no-bundle

mkdir -p "$root/output"
cp -f "$root/src/backend/target/release/$exe_name" "$root/output/$out_name"

echo
echo "Done: $root/output/$out_name"
