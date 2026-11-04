//! Embeds the version shown by `--version` and `version`.
//!
//! Inside the project's own git checkout the version comes from `git describe`, so a dev
//! build names its commit (`1.1.0-rc.3-2-gabc1234-dirty`). Anywhere else, notably a
//! `cargo install --git --tag` build, it is the package version that semantic-release
//! writes into `Cargo.toml` at each release (`scripts/set-version.sh`).

use std::path::Path;
use std::process::Command;

fn main() {
    let version = git_version().unwrap_or_else(|| env("CARGO_PKG_VERSION"));
    println!("cargo:rustc-env=TTUI_VERSION={version}");
    println!("cargo:rerun-if-changed=build.rs");
}

fn git_version() -> Option<String> {
    // A checkout without its own `.git` (cargo's git cache, a crate tarball) must not
    // pick up the tags of an enclosing repository.
    let toplevel = git(&["rev-parse", "--show-toplevel"])?;
    let manifest_dir = env("CARGO_MANIFEST_DIR");
    if Path::new(&toplevel).canonicalize().ok()? != Path::new(&manifest_dir).canonicalize().ok()? {
        return None;
    }
    // Rebuild when HEAD moves, a tag is added or the working tree changes state.
    for path in ["HEAD", "index", "refs/tags", "packed-refs"] {
        let path = git(&["rev-parse", "--path-format=absolute", "--git-path", path]);
        if let Some(path) = path.filter(|path| Path::new(path).exists()) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    let describe = git(&["describe", "--tags", "--match", "v*", "--dirty"])?;
    Some(describe.trim_start_matches('v').to_owned())
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}
