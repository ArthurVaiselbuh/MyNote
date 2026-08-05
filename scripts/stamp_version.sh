#!/usr/bin/env bash
# Writes a version into tauri.conf.json for the current CI build only —
# version_stamp.ps1 owns the committed value.
set -euo pipefail

version="${1:?usage: stamp_version.sh <version>}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conf="$root/src/backend/tauri.conf.json"

tmp=$(mktemp)
jq --arg v "$version" '.version = $v' "$conf" > "$tmp" && mv "$tmp" "$conf"

echo "Building version $version"
