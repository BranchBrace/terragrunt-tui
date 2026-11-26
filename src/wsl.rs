//! Installation WSL de la version Microsoft Store (`docs/specs/wsl-store-install.md`).
//!
//! Le MSIX embarque le binaire Linux (`linux/terragrunt-tui`). `ttui wsl install`, lancé depuis
//! Windows, le copie dans `~/.local/bin` de chaque distribution, où la commande interne
//! `wsl adopt` pose un marqueur. Une copie marquée se compare ensuite au paquet Store, au plus
//! une fois par jour, et se remplace par le binaire Linux du paquet quand il est plus récent :
//! le Store ne met à jour que la partie Windows.

use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Subcommand)]
pub enum WslCommand {
    /// Install `ttui` into the WSL distributions (Microsoft Store version, from Windows).
    Install {
        /// Only this distribution (repeatable).
        #[arg(long = "distro")]
        distros: Vec<String>,
        /// Install even where `ttui` is already installed another way (apt, cargo).
        #[arg(long)]
        force: bool,
    },
    /// Remove the copies installed by `ttui wsl install`.
    Uninstall {
        /// Only this distribution (repeatable).
        #[arg(long = "distro")]
        distros: Vec<String>,
    },
    /// Internal: write the embedded Linux binary to stdout (Windows).
    #[command(hide = true)]
    Binary,
    /// Internal: put the copied binary in place and mark it (Linux).
    #[command(hide = true)]
    Adopt {
        #[arg(long)]
        force: bool,
    },
    /// Internal: remove the marked copy (Linux).
    #[command(hide = true)]
    Remove,
}

pub fn run(command: WslCommand) -> Result<i32, Box<dyn Error>> {
    match command {
        WslCommand::Install { distros, force } => {
            windows_side(|| imp_windows::install(&distros, force))
        }
        WslCommand::Uninstall { distros } => windows_side(|| imp_windows::uninstall(&distros)),
        WslCommand::Binary => windows_side(imp_windows::binary),
        WslCommand::Adopt { force } => linux_side(|| imp_linux::adopt(force)),
        WslCommand::Remove => linux_side(imp_linux::remove),
    }
}

#[cfg(windows)]
fn windows_side(
    action: impl FnOnce() -> Result<i32, Box<dyn Error>>,
) -> Result<i32, Box<dyn Error>> {
    if !crate::store::packaged() {
        return Err(
            "`ttui wsl` demande la version Microsoft Store de terragrunt-tui. Pour WSL \
                    sans le Store, installer le paquet du dépôt apt (README, section Ubuntu / \
                    Debian)."
                .into(),
        );
    }
    action()
}

#[cfg(not(windows))]
fn windows_side(
    _action: impl FnOnce() -> Result<i32, Box<dyn Error>>,
) -> Result<i32, Box<dyn Error>> {
    Err(
        "`ttui wsl install` et `ttui wsl uninstall` se lancent depuis Windows, avec la version \
         Microsoft Store."
            .into(),
    )
}

#[cfg(unix)]
fn linux_side(action: impl FnOnce() -> Result<i32, Box<dyn Error>>) -> Result<i32, Box<dyn Error>> {
    action()
}

#[cfg(not(unix))]
fn linux_side(
    _action: impl FnOnce() -> Result<i32, Box<dyn Error>>,
) -> Result<i32, Box<dyn Error>> {
    Err("commande interne de `ttui wsl install`, exécutée dans la distribution WSL".into())
}

/// Au démarrage d'une copie WSL marquée : resynchronise avec le paquet Store (au plus une fois
/// par jour) et se relance sur le nouveau binaire. Sans effet ailleurs.
pub fn on_startup() {
    #[cfg(unix)]
    imp_linux::on_startup();
}

/// `ttui update` d'une copie WSL marquée : resynchronise tout de suite avec le paquet Store.
/// `None` hors d'une telle copie, pour laisser `ttui update` suivre son cours habituel.
pub fn update_if_store_managed() -> Option<Result<i32, Box<dyn Error>>> {
    #[cfg(unix)]
    return imp_linux::update_if_store_managed();
    #[cfg(not(unix))]
    None
}

/// Lancement depuis le menu Démarrer : propose une fois d'installer aussi dans WSL.
pub fn offer_from_start_menu() {
    #[cfg(windows)]
    imp_windows::offer_from_start_menu();
}

/// Distributions techniques de Docker Desktop, jamais ciblées par défaut.
#[cfg_attr(not(windows), allow(dead_code))]
const DOCKER_PREFIX: &str = "docker-desktop";
/// Chemins relatifs au `$HOME` de la distribution.
#[cfg_attr(not(unix), allow(dead_code))]
const BIN_DIR: &str = ".local/bin";
#[cfg_attr(not(unix), allow(dead_code))]
const MARKER: &str = ".local/share/terragrunt-tui/store-install.json";
#[cfg_attr(not(unix), allow(dead_code))]
const STAMP: &str = ".cache/terragrunt-tui/store-check";
/// Copie déposée par `ttui wsl install`, avant `wsl adopt`.
#[cfg_attr(not(unix), allow(dead_code))]
const INSTALL_TMP: &str = ".terragrunt-tui.tmp";
/// Variable transmise à WSL (`WSLENV`, traduite en chemin `/mnt/…`) : l'alias `ttui.exe`.
const ENV_WINDOWS_EXE: &str = "TTUI_WINDOWS_EXE";
/// Codes de sortie de `wsl adopt` / `wsl remove`.
const EXIT_SKIPPED: i32 = 3;
const EXIT_NOTHING: i32 = 4;
#[cfg_attr(not(unix), allow(dead_code))]
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
#[cfg_attr(not(unix), allow(dead_code))]
const CHECK_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg_attr(not(unix), allow(dead_code))]
const ORPHAN: &str = "paquet Microsoft Store introuvable depuis WSL (application désinstallée ou \
                      interop désactivée) : plus de mises à jour automatiques. `ttui update` \
                      réessaie ; le dépôt apt reste disponible.";

/// Marqueur d'une copie installée par `ttui wsl install`, à côté du binaire (identique à celui
/// du dépôt apt, il ne peut pas porter lui-même sa provenance).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(unix), allow(dead_code))]
struct Marker {
    source: String,
    version: String,
    files: Vec<PathBuf>,
    windows_exe: Option<PathBuf>,
}

/// Sortie de `wsl.exe` : UTF-16LE par défaut, UTF-8 avec `WSL_UTF8=1` (WSL récent). Un texte
/// UTF-8 ne contient jamais d'octet nul, un texte UTF-16 en contient dès qu'il a de l'ASCII.
#[cfg_attr(not(windows), allow(dead_code))]
fn decode_wsl_output(bytes: &[u8]) -> String {
    let text = if bytes.contains(&0) {
        let (pairs, _) = bytes.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    text.trim_start_matches('\u{feff}').to_owned()
}

/// Noms de `wsl.exe --list --quiet`, une distribution par ligne.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_distros(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\0'))
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Distributions visées : toutes sauf Docker Desktop, ou celles nommées par `--distro`.
#[cfg_attr(not(windows), allow(dead_code))]
fn select_distros(all: &[String], requested: &[String]) -> Result<Vec<String>, String> {
    if requested.is_empty() {
        return Ok(all
            .iter()
            .filter(|name| !name.to_ascii_lowercase().starts_with(DOCKER_PREFIX))
            .cloned()
            .collect());
    }
    let mut selected = Vec::new();
    let mut unknown = Vec::new();
    for name in requested {
        match all.iter().find(|known| known.eq_ignore_ascii_case(name)) {
            Some(known) if !selected.contains(known) => selected.push(known.clone()),
            Some(_) => {}
            None => unknown.push(name.as_str()),
        }
    }
    if unknown.is_empty() {
        Ok(selected)
    } else {
        Err(format!(
            "distribution WSL inconnue : {} (disponibles : {})",
            unknown.join(", "),
            all.join(", ")
        ))
    }
}

/// Version annoncée par `terragrunt-tui version` (`terragrunt-tui 1.5.0`).
#[cfg_attr(not(unix), allow(dead_code))]
fn parse_version_output(output: &str) -> Option<&str> {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("terragrunt-tui "))
        .map(str::trim)
        .filter(|version| !version.is_empty())
}

/// Vrai seulement si `candidate` est une version publiée strictement plus récente : un build
/// de développement (`1.3.0-2-gabc1234`) n'est jamais comparé, une copie ne recule jamais.
#[cfg_attr(not(unix), allow(dead_code))]
fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |version: &str| crate::update::version(&format!("v{version}"));
    matches!((parse(candidate), parse(current)), (Some(candidate), Some(current)) if candidate > current)
}

/// Une vérification est due sans trace de la précédente, ou 24 h après elle.
#[cfg_attr(not(unix), allow(dead_code))]
fn check_due(last: Option<SystemTime>, now: SystemTime) -> bool {
    last.is_none_or(|last| {
        now.duration_since(last)
            .is_ok_and(|age| age >= CHECK_INTERVAL)
    })
}

/// Commandes `ttui` trouvées qui ne viennent pas de ce mécanisme.
#[cfg_attr(not(unix), allow(dead_code))]
fn conflicts(found: &[PathBuf], managed: &[PathBuf]) -> Vec<PathBuf> {
    let mut conflicts: Vec<PathBuf> = Vec::new();
    for path in found {
        if !managed.contains(path) && !conflicts.contains(path) {
            conflicts.push(path.clone());
        }
    }
    conflicts
}

#[cfg(windows)]
mod imp_windows {
    use super::*;
    use std::ffi::OsString;
    use std::fs::{self, File};
    use std::io::{self, Write};
    use std::path::Path;
    use std::process::{Command, Stdio};

    /// Copie le binaire reçu sur stdin, puis le laisse se mettre en place lui-même. Ni
    /// guillemet ni barre oblique inverse : l'argument traverse la ligne de commande Windows.
    const INSTALL_SCRIPT: &str = "mkdir -p ~/.local/bin && cat > ~/.local/bin/.terragrunt-tui.tmp \
        && chmod 755 ~/.local/bin/.terragrunt-tui.tmp \
        && exec ~/.local/bin/.terragrunt-tui.tmp wsl adopt";
    const UNINSTALL_SCRIPT: &str = "test -x ~/.local/bin/terragrunt-tui || exit 4; exec ~/.local/bin/terragrunt-tui wsl remove";
    const PROMPT_FILE: &str = "terragrunt-tui\\wsl-prompt";

    pub fn install(requested: &[String], force: bool) -> Result<i32, Box<dyn Error>> {
        let binary = linux_binary()?;
        let distros = select_distros(&all_distros()?, requested)?;
        if distros.is_empty() {
            println!("Aucune distribution WSL à équiper.");
            return Ok(0);
        }
        let script = if force {
            format!("{INSTALL_SCRIPT} --force")
        } else {
            INSTALL_SCRIPT.to_owned()
        };
        let mut failed = false;
        let mut summary = Vec::new();
        for distro in &distros {
            announce(distro);
            let line = match in_distro(distro, &script, Some(&binary)) {
                Ok((0, output)) => output,
                Ok((EXIT_SKIPPED, output)) => format!("ignorée : {output}"),
                Ok((code, output)) => {
                    failed = true;
                    format!("échec (code {code}) : {output}")
                }
                Err(error) => {
                    failed = true;
                    format!("échec : {error}")
                }
            };
            summary.push(format!("  {distro} — {line}"));
        }
        println!("\nRécapitulatif :\n{}", summary.join("\n"));
        Ok(if failed { 2 } else { 0 })
    }

    pub fn uninstall(requested: &[String]) -> Result<i32, Box<dyn Error>> {
        let distros = select_distros(&all_distros()?, requested)?;
        let mut failed = false;
        let mut summary = Vec::new();
        for distro in &distros {
            announce(distro);
            let line = match in_distro(distro, UNINSTALL_SCRIPT, None) {
                Ok((0, output)) => format!("retiré : {output}"),
                Ok((EXIT_NOTHING, output)) if !output.is_empty() => output,
                Ok((EXIT_NOTHING, _)) => {
                    "rien à retirer : aucune copie installée par le Microsoft Store".into()
                }
                Ok((code, output)) => {
                    failed = true;
                    format!("échec (code {code}) : {output}")
                }
                Err(error) => {
                    failed = true;
                    format!("échec : {error}")
                }
            };
            summary.push(format!("  {distro} — {line}"));
        }
        println!("\nRécapitulatif :\n{}", summary.join("\n"));
        Ok(if failed { 2 } else { 0 })
    }

    pub fn binary() -> Result<i32, Box<dyn Error>> {
        let mut file = File::open(linux_binary()?)?;
        let mut stdout = io::stdout().lock();
        io::copy(&mut file, &mut stdout)?;
        stdout.flush()?;
        Ok(0)
    }

    pub fn offer_from_start_menu() {
        let Some(prompt) =
            std::env::var_os("LOCALAPPDATA").map(|dir| Path::new(&dir).join(PROMPT_FILE))
        else {
            return;
        };
        if prompt.exists() || linux_binary().is_err() {
            return;
        }
        let eligible = all_distros()
            .ok()
            .and_then(|all| select_distros(&all, &[]).ok())
            .unwrap_or_default();
        if eligible.is_empty() {
            return;
        }
        eprint!(
            "\nInstaller aussi `ttui` dans tes distributions WSL ({}) ? [o/N] ",
            eligible.join(", ")
        );
        let _ = io::stderr().flush();
        let mut answer = String::new();
        if io::stdin().read_line(&mut answer).is_err() {
            return;
        }
        let answer = answer.trim().to_lowercase();
        if let Some(parent) = prompt.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&prompt, &answer);
        if matches!(answer.as_str(), "o" | "oui" | "y" | "yes") {
            if let Err(error) = install(&[], false) {
                eprintln!("terragrunt-tui: {error}");
            }
        } else {
            eprintln!("Plus tard : `ttui wsl install` dans un terminal Windows.");
        }
    }

    /// Binaire Linux du paquet, à côté de l'exécutable Windows.
    fn linux_binary() -> Result<PathBuf, Box<dyn Error>> {
        let exe = std::env::current_exe()?;
        let binary = exe
            .parent()
            .ok_or("dossier du paquet introuvable")?
            .join("linux")
            .join("terragrunt-tui");
        if binary.is_file() {
            Ok(binary)
        } else {
            Err(format!("binaire Linux absent du paquet : {}", binary.display()).into())
        }
    }

    fn all_distros() -> Result<Vec<String>, Box<dyn Error>> {
        let output = wsl(&["--list", "--quiet"])
            .output()
            .map_err(|_| "WSL n'est pas installé (wsl.exe introuvable)")?;
        if !output.status.success() {
            return Err("aucune distribution WSL installée".into());
        }
        Ok(parse_distros(&decode_wsl_output(&output.stdout)))
    }

    /// Prévient avant de démarrer une distribution arrêtée.
    fn announce(distro: &str) {
        let running = wsl(&["--list", "--running", "--quiet"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| parse_distros(&decode_wsl_output(&output.stdout)))
            .unwrap_or_default();
        if running.iter().any(|name| name == distro) {
            println!("{distro}…");
        } else {
            println!("{distro} (démarrage de la distribution)…");
        }
    }

    /// Lance `script` dans la distribution, avec `stdin_file` sur l'entrée standard.
    fn in_distro(
        distro: &str,
        script: &str,
        stdin_file: Option<&Path>,
    ) -> Result<(i32, String), Box<dyn Error>> {
        let mut command = wsl(&["--distribution", distro, "--exec", "sh", "-c", script]);
        if let Some(exe) = windows_exe() {
            let mut forwarded = std::env::var_os("WSLENV").unwrap_or_default();
            if !forwarded.is_empty() {
                forwarded.push(":");
            }
            forwarded.push(format!("{ENV_WINDOWS_EXE}/p"));
            command.env(ENV_WINDOWS_EXE, exe).env("WSLENV", forwarded);
        }
        command
            .stdin(if stdin_file.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let writer = stdin_file.map(|path| {
            let path = path.to_owned();
            let mut stdin = child.stdin.take().expect("stdin redirigé");
            std::thread::spawn(move || -> io::Result<u64> {
                io::copy(&mut File::open(path)?, &mut stdin)
            })
        });
        let output = child.wait_with_output()?;
        let copied = writer.map(|writer| writer.join());
        // Si le script s'arrête tôt, la copie casse sur un tube fermé : son message prime.
        if output.status.success() {
            match copied {
                Some(Err(_)) => return Err("copie interrompue".into()),
                Some(Ok(Err(error))) => return Err(error.into()),
                _ => {}
            }
        }
        let mut text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let errors = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() && !errors.trim().is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(errors.trim());
        }
        Ok((output.status.code().unwrap_or(1), text))
    }

    fn wsl(args: &[&str]) -> Command {
        let mut command = Command::new("wsl.exe");
        command.args(args).env("WSL_UTF8", "1").stdin(Stdio::null());
        command
    }

    /// Alias d'exécution `ttui.exe`, joignable depuis WSL par l'interop.
    fn windows_exe() -> Option<OsString> {
        let dir = std::env::var_os("LOCALAPPDATA")?;
        Some(
            Path::new(&dir)
                .join("Microsoft\\WindowsApps\\ttui.exe")
                .into_os_string(),
        )
    }
}

#[cfg(not(windows))]
mod imp_windows {
    use std::error::Error;

    pub fn install(_requested: &[String], _force: bool) -> Result<i32, Box<dyn Error>> {
        unreachable!("filtré par windows_side")
    }

    pub fn uninstall(_requested: &[String]) -> Result<i32, Box<dyn Error>> {
        unreachable!("filtré par windows_side")
    }

    pub fn binary() -> Result<i32, Box<dyn Error>> {
        unreachable!("filtré par windows_side")
    }
}

#[cfg(unix)]
mod imp_linux {
    use super::*;
    use crate::cli::VERSION;
    use std::env;
    use std::fs::{self, File};
    use std::io::{self, Read};
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::os::unix::process::CommandExt;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::Instant;

    const SOURCE: &str = "microsoft-store";
    const UPDATE_TMP: &str = ".terragrunt-tui.update";

    enum SyncError {
        /// `ttui.exe` ne répond pas : paquet désinstallé ou interop désactivée.
        Unreachable,
        Failed(String),
    }

    pub fn adopt(force: bool) -> Result<i32, Box<dyn Error>> {
        let home = home().ok_or("HOME n'est pas défini")?;
        let bin = home.join(BIN_DIR);
        let exe = env::current_exe()?;
        if exe != bin.join(INSTALL_TMP) {
            return Err("commande interne de `ttui wsl install`".into());
        }
        let target = bin.join("terragrunt-tui");
        let alias = bin.join("ttui");
        let previous = read_marker(&home);
        let managed = previous
            .as_ref()
            .map(|marker| marker.files.clone())
            .unwrap_or_default();
        let found = conflicts(&installed_commands(&home), &managed);
        if !found.is_empty() && !force {
            let _ = fs::remove_file(&exe);
            let list = found
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            println!(
                "ttui est déjà installé hors du Microsoft Store ({}) ; relancer avec --force pour \
                 installer quand même",
                list.join(", ")
            );
            return Ok(EXIT_SKIPPED);
        }
        fs::rename(&exe, &target)?;
        match fs::symlink_metadata(&alias) {
            Ok(_) => fs::remove_file(&alias)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        symlink("terragrunt-tui", &alias)?;
        let windows_exe = env::var_os(ENV_WINDOWS_EXE)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute());
        write_marker(
            &home,
            &Marker {
                source: SOURCE.into(),
                version: VERSION.into(),
                files: vec![target, alias],
                windows_exe,
            },
        )?;
        let _ = fs::remove_file(home.join(STAMP));
        let action = match &previous {
            Some(marker) if marker.version != VERSION => {
                format!("mis à jour {} → {VERSION}", marker.version)
            }
            Some(_) => format!("{VERSION}, déjà à jour"),
            None => format!("{VERSION} dans ~/.local/bin"),
        };
        let path_note = if !login_path_contains(&bin) {
            format!(
                " ; ~/.local/bin n'est pas dans le PATH de ton shell : ajouter \
                 `export PATH=\"$HOME/.local/bin:$PATH\"` à {}",
                startup_file()
            )
        } else if previous.is_none() {
            " ; ouvrir un nouveau terminal si `ttui` est introuvable".into()
        } else {
            String::new()
        };
        println!("{action}{path_note}");
        Ok(0)
    }

    pub fn remove() -> Result<i32, Box<dyn Error>> {
        let home = home().ok_or("HOME n'est pas défini")?;
        let Some(marker) = read_marker(&home) else {
            println!("rien à retirer : aucune copie installée par le Microsoft Store");
            return Ok(EXIT_NOTHING);
        };
        for file in &marker.files {
            remove_if_present(file)?;
        }
        for file in [MARKER, STAMP] {
            let path = home.join(file);
            remove_if_present(&path)?;
            // Dossier propre à terragrunt-tui : retiré seulement s'il est vide.
            if let Some(parent) = path.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
        println!("terragrunt-tui {}", marker.version);
        Ok(0)
    }

    pub fn on_startup() {
        let Some((home, marker)) = managed_copy() else {
            return;
        };
        let stamp = home.join(STAMP);
        let last = fs::metadata(&stamp).and_then(|meta| meta.modified()).ok();
        if !check_due(last, SystemTime::now()) {
            return;
        }
        touch(&stamp);
        match sync(&home, &marker) {
            Ok(Some(version)) => {
                eprintln!(
                    "terragrunt-tui mis à jour {VERSION} → {version} depuis le Microsoft Store"
                );
                reexec(&marker.files[0]);
            }
            Ok(None) => {}
            Err(SyncError::Unreachable) => eprintln!("terragrunt-tui : {ORPHAN}"),
            Err(SyncError::Failed(error)) => eprintln!(
                "terragrunt-tui : mise à jour depuis le Microsoft Store impossible ({error}), \
                 nouvel essai demain ou avec `ttui update`"
            ),
        }
    }

    pub fn update_if_store_managed() -> Option<Result<i32, Box<dyn Error>>> {
        let (home, marker) = managed_copy()?;
        touch(&home.join(STAMP));
        Some(match sync(&home, &marker) {
            Ok(Some(version)) => {
                println!(
                    "terragrunt-tui mis à jour {VERSION} → {version} depuis le Microsoft Store"
                );
                Ok(0)
            }
            Ok(None) => {
                println!("terragrunt-tui {VERSION} est à jour (version du Microsoft Store)");
                Ok(0)
            }
            Err(SyncError::Unreachable) => Err(ORPHAN.into()),
            Err(SyncError::Failed(error)) => Err(error.into()),
        })
    }

    /// Le processus courant est la copie marquée (et non un `ttui` d'apt ou de cargo).
    fn managed_copy() -> Option<(PathBuf, Marker)> {
        let home = home()?;
        let marker = read_marker(&home)?;
        let target = marker.files.first()?.canonicalize().ok()?;
        let exe = env::current_exe().ok()?.canonicalize().ok()?;
        (exe == target).then_some((home, marker))
    }

    fn sync(home: &Path, marker: &Marker) -> Result<Option<String>, SyncError> {
        let exe = marker
            .windows_exe
            .clone()
            .unwrap_or_else(|| PathBuf::from("ttui.exe"));
        let output = run_with_timeout(Command::new(&exe).arg("version"), CHECK_TIMEOUT)
            .map_err(|_| SyncError::Unreachable)?;
        let available = parse_version_output(&output)
            .ok_or(SyncError::Unreachable)?
            .to_owned();
        if !is_newer(&available, VERSION) {
            return Ok(None);
        }
        let target = &marker.files[0];
        let staged = target.with_file_name(UPDATE_TMP);
        let result = fetch(&exe, &staged, &available).and_then(|()| {
            fs::rename(&staged, target).map_err(|error| error.to_string())?;
            write_marker(
                home,
                &Marker {
                    version: available.clone(),
                    ..marker.clone()
                },
            )
            .map_err(|error| error.to_string())
        });
        if let Err(error) = result {
            let _ = fs::remove_file(&staged);
            return Err(SyncError::Failed(error));
        }
        Ok(Some(available))
    }

    /// Récupère le binaire Linux du paquet et vérifie qu'il annonce la version attendue.
    fn fetch(exe: &Path, staged: &Path, expected: &str) -> Result<(), String> {
        let file = File::create(staged).map_err(|error| error.to_string())?;
        let status = Command::new(exe)
            .args(["wsl", "binary"])
            .stdin(Stdio::null())
            .stdout(file)
            .stderr(Stdio::null())
            .status()
            .map_err(|error| error.to_string())?;
        if !status.success() {
            return Err(format!("`ttui.exe wsl binary` a échoué ({status})"));
        }
        fs::set_permissions(staged, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
        let announced =
            run_with_timeout(Command::new(staged).arg("version"), Duration::from_secs(10))
                .map_err(|error| format!("le binaire reçu ne démarre pas : {error}"))?;
        match parse_version_output(&announced) {
            Some(version) if version == expected => Ok(()),
            other => Err(format!(
                "le binaire reçu annonce {} au lieu de {expected}",
                other.unwrap_or("une version illisible")
            )),
        }
    }

    fn reexec(target: &Path) {
        let mut args = env::args_os();
        let arg0 = args.next().unwrap_or_else(|| "terragrunt-tui".into());
        let error = Command::new(target).arg0(arg0).args(args).exec();
        eprintln!(
            "terragrunt-tui : relance impossible ({error}), ancienne version conservée pour cette fois"
        );
    }

    /// Sortie standard d'une commande courte, tuée au-delà de `timeout`.
    fn run_with_timeout(command: &mut Command, timeout: Duration) -> io::Result<String> {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if start.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::ErrorKind::TimedOut.into());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let mut output = String::new();
        if let Some(mut stdout) = child.stdout.take() {
            stdout.read_to_string(&mut output)?;
        }
        if status.success() {
            Ok(output)
        } else {
            Err(io::Error::other(format!("{status}")))
        }
    }

    /// `ttui` / `terragrunt-tui` déjà accessibles : PATH des shells de l'utilisateur et chemins
    /// usuels.
    fn installed_commands(home: &Path) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> =
            user_shells("for c in terragrunt-tui ttui; do command -v $c; done")
                .iter()
                .flat_map(|output| output.lines())
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .collect();
        for dir in [
            Path::new("/usr/bin"),
            Path::new("/usr/local/bin"),
            &home.join(".cargo/bin"),
            &home.join(BIN_DIR),
        ] {
            for name in ["terragrunt-tui", "ttui"] {
                let path = dir.join(name);
                if fs::symlink_metadata(&path).is_ok() {
                    found.push(path);
                }
            }
        }
        found
    }

    /// Fichier de démarrage où compléter le PATH, selon le shell de l'utilisateur.
    fn startup_file() -> &'static str {
        let shell = env::var("SHELL").unwrap_or_default();
        match shell.rsplit('/').next() {
            Some("zsh") => "~/.zshrc",
            Some("bash") => "~/.bashrc",
            _ => "~/.profile",
        }
    }

    /// Le PATH du shell de l'utilisateur (le premier qui répond) contient `dir`.
    fn login_path_contains(dir: &Path) -> bool {
        user_shells("printf %s \"$PATH\"")
            .first()
            .is_some_and(|path| path.split(':').any(|entry| Path::new(entry) == dir))
    }

    /// Sortie de `script` dans le shell de l'utilisateur en connexion interactive (`$SHELL
    /// -lic` : zsh ne lit `~/.zshrc`, où le PATH est souvent complété, qu'en interactif), puis
    /// dans `sh -l`. Des balises isolent la sortie du bruit des fichiers de démarrage.
    fn user_shells(script: &str) -> Vec<String> {
        const BEGIN: &str = "--ttui-begin--";
        const END: &str = "--ttui-end--";
        let wrapped = format!("printf '\\n{BEGIN}\\n'; {script}; printf '\\n{END}\\n'");
        let mut shells = Vec::new();
        if let Some(shell) = env::var_os("SHELL")
            .map(PathBuf::from)
            .filter(|shell| shell.is_file())
        {
            shells.push((shell, "-lic"));
        }
        shells.push((PathBuf::from("/bin/sh"), "-lc"));
        shells
            .iter()
            .filter_map(|(shell, flags)| {
                run_with_timeout(
                    Command::new(shell).args([*flags, wrapped.as_str()]),
                    Duration::from_secs(5),
                )
                .ok()
            })
            .filter_map(|output| {
                let (_, rest) = output.split_once(&format!("\n{BEGIN}\n"))?;
                let (inside, _) = rest.rsplit_once(&format!("\n{END}\n"))?;
                Some(inside.trim().to_owned())
            })
            .collect()
    }

    fn home() -> Option<PathBuf> {
        env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute())
    }

    fn read_marker(home: &Path) -> Option<Marker> {
        let marker: Marker =
            serde_json::from_str(&fs::read_to_string(home.join(MARKER)).ok()?).ok()?;
        (marker.source == SOURCE && !marker.files.is_empty()).then_some(marker)
    }

    fn write_marker(home: &Path, marker: &Marker) -> io::Result<()> {
        let path = home.join(MARKER);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let staged = path.with_extension("json.tmp");
        fs::write(&staged, serde_json::to_vec_pretty(marker)?)?;
        fs::rename(staged, path)
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, b"");
    }

    fn remove_if_present(path: &Path) -> io::Result<()> {
        match fs::remove_file(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

#[cfg(not(unix))]
mod imp_linux {
    use std::error::Error;

    pub fn adopt(_force: bool) -> Result<i32, Box<dyn Error>> {
        unreachable!("filtré par linux_side")
    }

    pub fn remove() -> Result<i32, Box<dyn Error>> {
        unreachable!("filtré par linux_side")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn wsl_list_output_is_read_in_utf16_and_utf8() {
        let utf16: Vec<u8> = "\u{feff}Ubuntu-24.04\r\n\r\ndocker-desktop\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(
            parse_distros(&decode_wsl_output(&utf16)),
            names(&["Ubuntu-24.04", "docker-desktop"])
        );
        assert_eq!(
            parse_distros(&decode_wsl_output(b"Debian\nAlpine\n")),
            names(&["Debian", "Alpine"])
        );
        assert!(parse_distros(&decode_wsl_output(b"")).is_empty());
    }

    #[test]
    fn docker_desktop_is_skipped_unless_named_and_unknown_names_fail() {
        let all = names(&["Ubuntu", "docker-desktop", "docker-desktop-data", "Debian"]);
        assert_eq!(
            select_distros(&all, &[]).unwrap(),
            names(&["Ubuntu", "Debian"])
        );
        assert_eq!(
            select_distros(&all, &names(&["debian", "Debian"])).unwrap(),
            names(&["Debian"])
        );
        let error = select_distros(&all, &names(&["Fedora"])).unwrap_err();
        assert!(error.contains("Fedora"), "{error}");
    }

    #[test]
    fn version_is_read_from_the_version_command() {
        assert_eq!(
            parse_version_output("terragrunt-tui 1.5.0\n"),
            Some("1.5.0")
        );
        assert_eq!(parse_version_output("rien d'utile"), None);
        assert_eq!(parse_version_output("terragrunt-tui \n"), None);
    }

    #[test]
    fn only_a_strictly_newer_published_version_replaces_the_copy() {
        assert!(is_newer("1.5.0", "1.4.2"));
        assert!(is_newer("1.5.0", "1.5.0-rc.3"));
        assert!(!is_newer("1.5.0", "1.5.0"));
        assert!(!is_newer("1.4.0", "1.5.0"));
        assert!(!is_newer("1.5.0", "1.3.0-2-gabc1234-dirty"));
        assert!(!is_newer("pas-une-version", "1.0.0"));
    }

    #[test]
    fn the_store_is_checked_at_most_once_a_day() {
        let now = SystemTime::now();
        assert!(check_due(None, now));
        assert!(!check_due(Some(now - Duration::from_secs(3600)), now));
        assert!(check_due(Some(now - CHECK_INTERVAL), now));
        assert!(!check_due(Some(now + Duration::from_secs(60)), now));
    }

    #[test]
    fn only_commands_not_installed_by_the_store_conflict() {
        let managed = vec![
            PathBuf::from("/home/u/.local/bin/terragrunt-tui"),
            PathBuf::from("/home/u/.local/bin/ttui"),
        ];
        let found = vec![
            PathBuf::from("/home/u/.local/bin/ttui"),
            PathBuf::from("/usr/bin/ttui"),
            PathBuf::from("/usr/bin/ttui"),
        ];
        assert_eq!(
            conflicts(&found, &managed),
            vec![PathBuf::from("/usr/bin/ttui")]
        );
        assert!(conflicts(&managed, &managed).is_empty());
    }

    #[test]
    fn the_marker_round_trips_through_json() {
        let marker = Marker {
            source: "microsoft-store".into(),
            version: "1.5.0".into(),
            files: vec![PathBuf::from("/home/u/.local/bin/terragrunt-tui")],
            windows_exe: Some(PathBuf::from(
                "/mnt/c/Users/u/AppData/Local/Microsoft/WindowsApps/ttui.exe",
            )),
        };
        let json = serde_json::to_string(&marker).unwrap();
        assert_eq!(serde_json::from_str::<Marker>(&json).unwrap(), marker);
    }
}
