#!/usr/bin/env bash
# Build and push the release images listed in deploy/images.txt.
# Called by semantic-release with the release version (rc: 1.2.0-rc.3, main: 1.2.0).
# Line format: <name> <dockerfile> [context]  ->  ghcr.io/branchbrace/terragrunt-tui-<name>:<version>
# An empty manifest is valid: the release is then a Git tag and GitHub release only.
set -euo pipefail

version="${1:?usage: release-images.sh <version>}"
registry="ghcr.io/branchbrace"
repo="terragrunt-tui"
manifest="$(dirname "$0")/../deploy/images.txt"

entries="$(grep -vE '^[[:space:]]*(#|$)' "$manifest" 2>/dev/null || true)"
if [ -z "$entries" ]; then
  echo "release-images: no image declared in deploy/images.txt; nothing to publish."
  exit 0
fi

while read -r name dockerfile context; do
  image="$registry/$repo-$name"
  echo "release-images: building $image:$version from $dockerfile"
  docker build -f "$dockerfile" -t "$image:$version" "${context:-.}"
  docker push "$image:$version"
done <<< "$entries"
