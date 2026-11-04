#!/usr/bin/env bash
# Commit the stable release files and push them to release/v<version>, since main only
# accepts changes through a pull request. semantic-release then tags this commit (it
# re-reads HEAD after the prepare step), so the tag carries the bumped Cargo.toml.
# Merge release/v<version> into main with a PR (merge commit) to make the tag reachable.
set -euo pipefail

version="${1:?usage: commit-release-branch.sh <version>}"
branch="release/v$version"

git add CHANGELOG.md Cargo.toml Cargo.lock
git -c user.name="semantic-release-bot" -c user.email="semantic-release-bot@users.noreply.github.com" \
  commit -m "chore(release): $version [skip ci]"
git push origin "HEAD:refs/heads/$branch"
echo "::notice::Release commit pushed to $branch — open a PR to main to merge it."
