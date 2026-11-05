// main is protected by a "changes through a pull request" ruleset, so the stable release
// commit cannot be pushed to it. On main the release commit is made locally, tagged by
// semantic-release, and pushed to release/vX.Y.Z; a PR then merges it into main.
// rc has no such rule and keeps the direct push of @semantic-release/git.
const onMain = process.env.GITHUB_REF_NAME === 'main';

const notes = 'chore(release): ${nextRelease.version} [skip ci]\n\n${nextRelease.notes}';

module.exports = {
  branches: ['main', { name: 'rc', prerelease: 'rc' }],
  plugins: [
    '@semantic-release/commit-analyzer',
    '@semantic-release/release-notes-generator',
    '@semantic-release/changelog',
    ['@semantic-release/exec', {
      prepareCmd: onMain
        ? './scripts/set-version.sh ${nextRelease.version} && ./scripts/commit-release-branch.sh ${nextRelease.version}'
        : './scripts/set-version.sh ${nextRelease.version}',
      publishCmd: './scripts/release-images.sh ${nextRelease.version}'
        + ' && ./scripts/build-deb.sh ${nextRelease.version}'
        + ' && ./scripts/publish-apt.sh ${nextRelease.version}',
    }],
    ['@semantic-release/github', {
      assets: [{ path: 'dist/*.deb' }, { path: 'dist/*.tar.gz' }],
    }],
    ...(onMain ? [] : [['@semantic-release/git', {
      assets: ['CHANGELOG.md', 'Cargo.toml', 'Cargo.lock'],
      message: notes,
    }]]),
  ],
};
