//! Version Microsoft Store (paquet MSIX, `scripts/package-msix.ps1`).
//!
//! Le Store installe et met à jour l'application : `ttui update` renvoie vers sa fiche au lieu
//! de recompiler une release. Lancée depuis le menu Démarrer, l'application reçoit une console
//! à elle seule, qui se fermerait dès la fin du processus : on attend alors Entrée.

use std::io::{self, IsTerminal, Write};

/// Fiche de l'application dans le Microsoft Store.
pub const STORE_PAGE: &str = "ms-windows-store://pdp/?productid=9MSXGP354MFL";

/// Vrai quand le processus tourne depuis le paquet MSIX installé par le Microsoft Store.
pub fn packaged() -> bool {
    imp::packaged()
}

/// `ttui update` de la version Store : le Store seul livre les mises à jour.
pub fn update() -> i32 {
    println!(
        "terragrunt-tui a été installé depuis le Microsoft Store : ses mises à jour sont \
         téléchargées et installées par le Store.\nFiche du Store : {STORE_PAGE}"
    );
    imp::open(STORE_PAGE);
    0
}

/// Garde ouverte une console créée pour ce seul processus (menu Démarrer, Explorateur), le
/// temps de lire ce qui s'y est affiché. Sans effet dans un terminal déjà ouvert.
pub fn pause_if_console_closes_on_exit() {
    // Depuis WSL (interop), le processus a aussi une console à lui, mais pas de terminal en
    // entrée : on ne doit ni proposer ni attendre quoi que ce soit.
    if !imp::owns_console() || !io::stdin().is_terminal() {
        return;
    }
    if packaged() {
        crate::wsl::offer_from_start_menu();
    }
    eprint!(
        "\nterragrunt-tui s'utilise dans un terminal : ouvrir Windows Terminal ou PowerShell \
         dans un dépôt Terragrunt et taper `ttui run`.\nAppuyer sur Entrée pour fermer cette fenêtre."
    );
    let _ = io::stderr().flush();
    let _ = io::stdin().read_line(&mut String::new());
}

#[cfg(windows)]
mod imp {
    use std::process::Command;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentPackageFullName(length: *mut u32, name: *mut u16) -> i32;
        fn GetConsoleProcessList(processes: *mut u32, count: u32) -> u32;
    }

    const APPMODEL_ERROR_NO_PACKAGE: i32 = 15700;

    pub fn packaged() -> bool {
        let mut length = 0u32;
        // Sans tampon, un processus empaqueté reçoit ERROR_INSUFFICIENT_BUFFER.
        let status = unsafe { GetCurrentPackageFullName(&mut length, std::ptr::null_mut()) };
        status != APPMODEL_ERROR_NO_PACKAGE
    }

    pub fn owns_console() -> bool {
        let mut processes = [0u32; 2];
        let count = unsafe { GetConsoleProcessList(processes.as_mut_ptr(), 2) };
        count == 1
    }

    pub fn open(target: &str) {
        let _ = Command::new("explorer.exe").arg(target).status();
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn packaged() -> bool {
        false
    }

    pub fn owns_console() -> bool {
        false
    }

    pub fn open(_target: &str) {}
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    const MANIFEST: &str = include_str!("../packaging/msix/AppxManifest.xml");

    #[test]
    fn a_test_run_is_not_the_store_package() {
        assert!(!super::packaged());
    }

    #[test]
    fn msix_manifest_exposes_both_commands_as_console_aliases() {
        for alias in ["terragrunt-tui.exe", "ttui.exe"] {
            assert!(MANIFEST.contains(&format!("<uap5:ExecutionAlias Alias=\"{alias}\" />")));
        }
        assert!(MANIFEST.contains("Executable=\"terragrunt-tui.exe\""));
        assert!(MANIFEST.contains("desktop4:Subsystem=\"console\""));
        assert!(MANIFEST.contains("<rescap:Capability Name=\"runFullTrust\" />"));
        assert!(MANIFEST.contains("Publisher=\"CN=94794141-B9ED-4FA9-B74B-C1FC94E6B207\""));
        assert!(MANIFEST.contains("Version=\"@VERSION@\""));
        assert!(MANIFEST.contains("Name=\"AntoineMonnier.ttuiforTerragrunt\""));
    }

    #[test]
    fn every_logo_of_the_msix_manifest_exists() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/msix/Assets");
        for logo in [
            "StoreLogo.scale-100.png",
            "StoreLogo.scale-200.png",
            "Square44x44Logo.scale-100.png",
            "Square44x44Logo.targetsize-16_altform-unplated.png",
            "Square150x150Logo.scale-100.png",
            "Wide310x150Logo.scale-100.png",
        ] {
            assert!(assets.join(logo).is_file(), "logo manquant : {logo}");
        }
        for reference in MANIFEST.split("Assets\\").skip(1) {
            let stem = reference.split(".png").next().unwrap();
            assert!(
                assets.join(format!("{stem}.scale-100.png")).is_file(),
                "logo du manifeste sans fichier : {stem}"
            );
        }
    }
}
