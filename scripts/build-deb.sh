#!/usr/bin/env bash
# Build the Linux release packages into dist/:
#   - terragrunt-tui_<debversion>_amd64.deb   (consumed by the apt repository)
#   - terragrunt-tui-<version>-x86_64-unknown-linux-musl.tar.gz
# The binaries are static (musl), so one package runs on every Ubuntu/Debian, WSL included.
# Called by semantic-release with the release version (rc: 1.2.0-rc.3, main: 1.2.0).
set -euo pipefail

version="${1:?usage: build-deb.sh <version>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
target="x86_64-unknown-linux-musl"
dist="$root/dist"

# Debian sorts "~" before anything, so 1.2.0~rc.3 < 1.2.0 (a "-" would sort it after).
debversion="${version/-/\~}"

CARGO_PROFILE_RELEASE_STRIP=true cargo build --release --locked --target "$target" --manifest-path "$root/Cargo.toml"
bin="$root/target/$target/release"

rm -rf "$dist"
mkdir -p "$dist"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

pkg="$stage/terragrunt-tui"
install -Dm755 "$bin/terragrunt-tui" "$pkg/usr/bin/terragrunt-tui"
install -Dm755 "$bin/ttui" "$pkg/usr/bin/ttui"
install -Dm644 "$root/LICENSE" "$pkg/usr/share/doc/terragrunt-tui/copyright"
mkdir -p "$pkg/DEBIAN"
cat > "$pkg/DEBIAN/control" <<EOF
Package: terragrunt-tui
Version: $debversion
Architecture: amd64
Maintainer: BranchBrace <noreply@github.com>
Section: devel
Priority: optional
Homepage: https://github.com/BranchBrace/terragrunt-tui
Installed-Size: $(du -sk "$pkg/usr" | cut -f1)
Description: Local read-only Terragrunt plan aggregator CLI and TUI
 Runs terragrunt plan across a stack and reviews the result in a terminal UI.
 Installs the terragrunt-tui command and its short alias ttui.
EOF
dpkg-deb --build --root-owner-group "$pkg" "$dist/terragrunt-tui_${debversion}_amd64.deb"

tarball="$stage/terragrunt-tui-$version-$target"
install -Dm755 "$bin/terragrunt-tui" "$tarball/terragrunt-tui"
install -Dm755 "$bin/ttui" "$tarball/ttui"
install -Dm644 "$root/LICENSE" "$tarball/LICENSE"
tar -C "$stage" -czf "$dist/terragrunt-tui-$version-$target.tar.gz" "$(basename "$tarball")"

ls -l "$dist"
