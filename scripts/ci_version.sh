#!/usr/bin/env bash
# Prints the version CI should build: <major>.<minor> from tauri.conf.json,
# with a third component counting commits since the last version stamp.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conf="$root/src/backend/tauri.conf.json"

base=$(jq -r .version "$conf" | cut -d. -f1,2)

last_stamp=$(git -C "$root" log -1 --format=%H -G'"version"' -- "$conf" || true)
if [ -n "$last_stamp" ]; then
  commits_since_stamp=$(git -C "$root" rev-list --count "${last_stamp}..HEAD")
else
  commits_since_stamp=$(git -C "$root" rev-list --count HEAD)
fi

echo "${base}.${commits_since_stamp}"
