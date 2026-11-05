#!/usr/bin/env bash
# Publish the .deb from dist/ to the signed apt repository served by GitHub Pages
# (branch gh-pages, https://branchbrace.github.io/terragrunt-tui).
#
# Suites:  stable = releases from main only
#          rc     = rc prereleases and stable releases (apt picks the newest)
# Layout:  pool/stable/*.deb, pool/rc/*.deb, dists/<suite>/{InRelease,Release,Release.gpg}
#
# Signing key: secret APT_GPG_PRIVATE_KEY (armored), optional APT_GPG_PASSPHRASE.
# Without the key the step is skipped with a warning, so a release never fails on it.
# gh-pages is rewritten as a single commit on every publish so the binaries do not pile
# up in the repository history.
# Called by semantic-release with the release version (rc: 1.2.0-rc.3, main: 1.2.0).
set -euo pipefail

version="${1:?usage: publish-apt.sh <version>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
keep_rc=10

if [ -z "${APT_GPG_PRIVATE_KEY:-}" ]; then
  echo "::warning::publish-apt: APT_GPG_PRIVATE_KEY is not set; apt repository not updated."
  exit 0
fi

deb="$(ls "$root"/dist/terragrunt-tui_*_amd64.deb)"
case "$version" in
  *-*) channel=rc ;;
  *) channel=stable ;;
esac

export GNUPGHOME="$(mktemp -d)"
site="$(mktemp -d)"
trap 'rm -rf "$GNUPGHOME" "$site"' EXIT
printf '%s\n' "$APT_GPG_PRIVATE_KEY" | gpg --batch --import
key="$(gpg --list-secret-keys --with-colons | awk -F: '/^fpr:/ {print $10; exit}')"
sign=(gpg --batch --yes --pinentry-mode loopback --passphrase "${APT_GPG_PASSPHRASE:-}" --local-user "$key")

# Start from the current site, if any.
if git -C "$root" fetch --depth 1 origin gh-pages 2>/dev/null; then
  git -C "$root" archive FETCH_HEAD | tar -x -C "$site"
fi

cd "$site"
mkdir -p "pool/$channel"
cp "$deb" "pool/$channel/"
# Bound the rc channel; stable releases are all kept.
find pool -path 'pool/rc/*.deb' | sort -V | head -n -"$keep_rc" | xargs -r rm -f

for suite in stable rc; do
  dir="dists/$suite/main/binary-amd64"
  mkdir -p "$dir"
  if [ "$suite" = stable ]; then scan=pool/stable; else scan=pool; fi
  mkdir -p "$scan"
  apt-ftparchive packages "$scan" > "$dir/Packages"
  gzip -9nkf "$dir/Packages"
  apt-ftparchive \
    -o APT::FTPArchive::Release::Origin=terragrunt-tui \
    -o APT::FTPArchive::Release::Label=terragrunt-tui \
    -o APT::FTPArchive::Release::Suite="$suite" \
    -o APT::FTPArchive::Release::Codename="$suite" \
    -o APT::FTPArchive::Release::Components=main \
    -o APT::FTPArchive::Release::Architectures=amd64 \
    release "dists/$suite" > Release.tmp
  mv Release.tmp "dists/$suite/Release"
  "${sign[@]}" --clearsign -o "dists/$suite/InRelease" "dists/$suite/Release"
  "${sign[@]}" -abs -o "dists/$suite/Release.gpg" "dists/$suite/Release"
done

gpg --export "$key" > terragrunt-tui.gpg
gpg --armor --export "$key" > terragrunt-tui.asc
touch .nojekyll
cat > index.html <<'EOF'
<!doctype html>
<meta charset="utf-8">
<title>terragrunt-tui apt repository</title>
<h1>terragrunt-tui apt repository</h1>
<pre>
curl -fsSL https://branchbrace.github.io/terragrunt-tui/terragrunt-tui.gpg \
  | sudo tee /usr/share/keyrings/terragrunt-tui.gpg >/dev/null
echo "deb [arch=amd64 signed-by=/usr/share/keyrings/terragrunt-tui.gpg] https://branchbrace.github.io/terragrunt-tui stable main" \
  | sudo tee /etc/apt/sources.list.d/terragrunt-tui.list
sudo apt update &amp;&amp; sudo apt install terragrunt-tui
</pre>
<p>Use <code>rc</code> instead of <code>stable</code> to follow prereleases.</p>
EOF

git init -q -b gh-pages
git add -A
git -c user.name="github-actions[bot]" -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
  commit -q -m "chore(apt): publier terragrunt-tui $version"
# This fresh repository does not inherit the credentials actions/checkout set up.
if [ -n "${GITHUB_TOKEN:-}" ] && [ -n "${GITHUB_REPOSITORY:-}" ]; then
  remote="https://x-access-token:${GITHUB_TOKEN}@github.com/${GITHUB_REPOSITORY}.git"
else
  remote="$(git -C "$root" remote get-url origin)"
fi
git push -qf "$remote" gh-pages:gh-pages
echo "publish-apt: $version published to the $channel channel."
