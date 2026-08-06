#!/usr/bin/env bash
# Prints the version CI should build: <major>.<minor> from tauri.conf.json,
# with a third component counting commits since the last version stamp.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conf="$root/src/backend/tauri.conf.json"

base=$(jq -r .version "$conf" | cut -d. -f1,2)

last_stamp=$(git -C "$root" log -1 --format=%H -G'"version"' -- "$conf" || true)
commits_since_stamp=$(git -C "$root" rev-list --count "${last_stamp:+$last_stamp..}HEAD")

echo "${base}.${commits_since_stamp}"
