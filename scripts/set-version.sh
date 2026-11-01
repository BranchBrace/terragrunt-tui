#!/usr/bin/env bash
# Write the release version into Cargo.toml and Cargo.lock so that `--version` of a
# binary installed from the release tag matches the tag.
# Called by semantic-release with the release version (rc: 1.2.0-rc.3, main: 1.2.0).
set -euo pipefail

version="${1:?usage: set-version.sh <version>}"
root="$(dirname "$0")/.."

sed -i -E "0,/^version = \".*\"$/s//version = \"$version\"/" "$root/Cargo.toml"
sed -i -E "/^name = \"terragrunt-tui\"$/{n;s/^version = \".*\"$/version = \"$version\"/}" "$root/Cargo.lock"
