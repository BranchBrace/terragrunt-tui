//! `ttui update` : réinstalle la dernière release (ou release candidate) publiée sur GitHub.
//!
//! Les releases ne publient aucun binaire : la mise à jour recompile le tag choisi avec
//! `cargo install --git`, en passant par le client git pour réutiliser ses identifiants
//! (le dépôt est privé).

use std::path::{Path, PathBuf};
use std::process::Command;

const REPOSITORY: &str = concat!(env!("CARGO_PKG_REPOSITORY"), ".git");
const BINARIES: [&str; 2] = ["ttui", "terragrunt-tui"];

pub fn run(rc: bool) -> Result<i32, Box<dyn std::error::Error>> {
    let tags = remote_tags()?;
    let Some(target) = latest_tag(&tags, rc) else {
        let kind = if rc { "release" } else { "release stable" };
        return Err(format!("aucune {kind} trouvée sur {REPOSITORY}").into());
    };
    let installed = installed_tag();
    if installed.as_deref() == Some(target.as_str()) {
        println!("Déjà à jour : {target}.");
        return Ok(0);
    }
    println!(
        "Installation de {target} (actuellement : {}) depuis {REPOSITORY}…",
        installed.as_deref().unwrap_or("inconnue")
    );

    // Sous Windows, l'exécutable en cours ne peut pas être écrasé, mais il peut être renommé.
    let moved = move_aside_installed_binaries()?;
    let status = Command::new("cargo")
        .args([
            "install", "--locked", "--force", "--git", REPOSITORY, "--tag",
        ])
        .arg(&target)
        .env("CARGO_NET_GIT_FETCH_WITH_CLI", "true")
        .status()
        .map_err(|error| {
            format!("cargo introuvable ({error}) : la mise à jour compile depuis les sources")
        })?;
    if !status.success() {
        restore(&moved);
        return Ok(status.code().unwrap_or(1));
    }
    for (_, aside) in &moved {
        // Échoue pour l'exécutable encore en cours ; il sera retiré à la prochaine mise à jour.
        let _ = std::fs::remove_file(aside);
    }
    println!("terragrunt-tui {target} installé.");
    Ok(0)
}

fn remote_tags() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["ls-remote", "--tags", "--refs", REPOSITORY])
        .output()
        .map_err(|error| format!("git introuvable : {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "impossible de lister les tags de {REPOSITORY} : {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once("refs/tags/").map(|(_, tag)| tag.to_owned()))
        .collect())
}

/// Version d'un tag `vX.Y.Z` ou `vX.Y.Z-rc.N` ; une version stable passe après ses rc.
fn version(tag: &str) -> Option<(u64, u64, u64, u64)> {
    let (core, pre) = match tag.strip_prefix('v')?.split_once('-') {
        Some((core, pre)) => (core, Some(pre.strip_prefix("rc.")?.parse().ok()?)),
        None => (tag.strip_prefix('v')?, None),
    };
    let mut numbers = core.split('.').map(|part| part.parse().ok());
    let (major, minor, patch) = (numbers.next()??, numbers.next()??, numbers.next()??);
    if numbers.next().is_some() {
        return None;
    }
    Some((major, minor, patch, pre.unwrap_or(u64::MAX)))
}

/// Tag le plus récent : stable uniquement, ou toutes versions confondues avec `rc`.
fn latest_tag(tags: &[String], rc: bool) -> Option<String> {
    tags.iter()
        .filter_map(|tag| version(tag).map(|version| (version, tag)))
        .filter(|((.., pre), _)| rc || *pre == u64::MAX)
        .max()
        .map(|(_, tag)| tag.clone())
}

/// Tag installé d'après `cargo install --list`, quand l'installation vient d'un tag git.
fn installed_tag() -> Option<String> {
    let output = Command::new("cargo")
        .args(["install", "--list"])
        .output()
        .ok()?;
    installed_tag_in(&String::from_utf8_lossy(&output.stdout))
}

fn installed_tag_in(listing: &str) -> Option<String> {
    let line = listing
        .lines()
        .find(|line| line.starts_with(concat!(env!("CARGO_PKG_NAME"), " ")))?;
    let tag = line.split_once("?tag=")?.1;
    Some(tag.split(['#', ')']).next()?.to_owned())
}

fn cargo_bin_dir() -> Option<PathBuf> {
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|home| Path::new(&home).join(".cargo"))
        })?;
    Some(home.join("bin"))
}

fn move_aside_installed_binaries() -> Result<Vec<(PathBuf, PathBuf)>, Box<dyn std::error::Error>> {
    let Some(bin) = cargo_bin_dir() else {
        return Ok(Vec::new());
    };
    let mut moved = Vec::new();
    for name in BINARIES {
        let path = bin.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        if !path.exists() {
            continue;
        }
        let aside = path.with_extension("old");
        let _ = std::fs::remove_file(&aside);
        if let Err(error) = std::fs::rename(&path, &aside) {
            restore(&moved);
            return Err(format!("impossible de déplacer {} : {error}", path.display()).into());
        }
        moved.push((path, aside));
    }
    Ok(moved)
}

fn restore(moved: &[(PathBuf, PathBuf)]) {
    for (path, aside) in moved {
        if !path.exists() {
            let _ = std::fs::rename(aside, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn stable_update_ignores_release_candidates_and_rc_takes_the_newest_of_all() {
        let all = tags(&[
            "v1.0.0-rc.4",
            "v1.0.0",
            "v1.1.0-rc.1",
            "v1.1.0-rc.10",
            "v1.1.0-rc.2",
            "v0.9.0",
            "not-a-version",
        ]);
        assert_eq!(latest_tag(&all, false).as_deref(), Some("v1.0.0"));
        assert_eq!(latest_tag(&all, true).as_deref(), Some("v1.1.0-rc.10"));
    }

    #[test]
    fn a_stable_release_wins_over_its_own_release_candidates() {
        let all = tags(&["v1.1.0-rc.3", "v1.1.0"]);
        assert_eq!(latest_tag(&all, true).as_deref(), Some("v1.1.0"));
        assert_eq!(latest_tag(&tags(&["v1.1.0-rc.3"]), false), None);
    }

    #[test]
    fn installed_tag_is_read_from_the_cargo_install_listing() {
        let listing = "cargo-edit v0.13.0:\n    cargo-add.exe\n\
                       terragrunt-tui v0.2.0 (https://github.com/MonniHermes/terragrunt-tui.git?tag=v1.1.0-rc.1#e5398cfe):\n    terragrunt-tui.exe\n    ttui.exe\n";
        assert_eq!(installed_tag_in(listing).as_deref(), Some("v1.1.0-rc.1"));
        assert_eq!(
            installed_tag_in("terragrunt-tui v0.2.0 (C:\\src\\terragrunt-tui):\n"),
            None
        );
    }
}
