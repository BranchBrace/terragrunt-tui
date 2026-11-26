# Installation WSL depuis la version Microsoft Store

Statut : conception validée le 2026-11-21, implémentée dans [`src/wsl.rs`](../../src/wsl.rs).

## Objectif

L'installation de **ttui for Terragrunt** depuis le Microsoft Store doit rendre la commande `ttui` disponible :

- sous Windows, dans tout terminal — **déjà le cas** : les alias d'exécution `ttui.exe` et `terragrunt-tui.exe` du MSIX ([`AppxManifest.xml`](../../packaging/msix/AppxManifest.xml)) mettent le binaire dans le PATH ;
- dans les distributions WSL de l'utilisateur, sous la forme du **binaire Linux natif** : il appelle le `terragrunt` de la distro, lit le système de fichiers Linux directement et voit la configuration et les credentials Linux.

Le `ttui.exe` Windows est déjà appelable depuis WSL par l'interop (`WindowsApps` est dans le PATH WSL par défaut), mais il exécuterait le Terragrunt Windows et lirait le dépôt à travers `\\wsl$` : ce n'est pas le comportement visé.

## Contrainte structurante

Un MSIX n'exécute rien à l'installation (ni script post-installation ni action personnalisée) et ne peut pas écrire dans une distro WSL. Le Store ne peut donc rien installer dans WSL de lui-même : l'installation WSL est une **action lancée après coup** par l'utilisateur, et la mise à jour des copies WSL est **pilotée depuis Linux**, puisque le Store ne met à jour que la partie Windows.

## Décisions

| Sujet | Décision |
|---|---|
| Ce qui tourne dans WSL | Le binaire Linux musl x64, natif. |
| Livraison | Copie du binaire embarqué dans le MSIX vers chaque distro. |
| Déclencheur | `ttui wsl install`, proposé aussi au lancement depuis le menu Démarrer. |
| Mise à jour | La copie WSL se compare au paquet Store au plus une fois par jour, se remplace et se relance. |
| Coexistence | Une distro où `ttui` existe déjà (apt, cargo) est ignorée, sauf `--force`. |
| Portée | Toutes les distros sauf `docker-desktop*` ; `--distro` pour cibler. |
| Désinstallation | `ttui wsl uninstall` ; une copie orpheline continue de fonctionner. |
| Build | Le binaire vient de l'archive musl de la release taguée. |

Hors périmètre : arm64 (le MSIX reste x64), l'amorçage du dépôt apt dans les distros.

Écartés, pour mémoire :

- **amorcer apt** dans chaque distro : exige `sudo`, ne couvre que Debian/Ubuntu ;
- **shim** vers le binaire Linux rangé dans `WindowsApps` : chemin qui change à chaque version, ACL de `WindowsApps`, dépendance permanente à l'interop, chargement depuis drvfs ;
- **installation automatique** au premier lancement ou par une `StartupTask` : `wsl.exe -d` démarre les distros arrêtées (VM WSL2, plusieurs secondes), ce qui ne doit pas arriver sans action de l'utilisateur ;
- **artefact CI** entre le job Linux et le job Windows : cassé en relance manuelle (`workflow_dispatch` est un nouveau run) et contraire à la règle « aucun artefact CI » d'AGENTS.md. L'asset de release contient les mêmes octets.

## Comportement

### Paquet MSIX

- Le paquet contient `linux/terragrunt-tui`, le binaire musl x64 de la même version que le binaire Windows.
- Le binaire Windows le localise par le dossier d'installation du paquet (répertoire de l'exécutable courant).
- `ttui wsl …` n'existe que dans la version empaquetée (`store::packaged()`). Hors paquet (`cargo install` sous Windows), la commande explique qu'elle demande la version Microsoft Store et renvoie vers le dépôt apt pour WSL.

### `ttui wsl install` (Windows)

Options : `--distro <nom>` (répétable), `--force`.

1. Vérifie que `wsl.exe` existe ; sinon, message « WSL n'est pas installé » et code de sortie non nul.
2. Liste les distros avec `wsl.exe -l -q`. La sortie est en **UTF-16LE** : la décoder comme telle et ignorer les lignes vides.
3. Exclut les distros dont le nom commence par `docker-desktop`. Avec `--distro`, ne garde que les distros nommées ; un nom inconnu arrête la commande avant toute installation, avec la liste des distros disponibles.
4. Pour chaque distro, prévient avant de la démarrer si elle est arrêtée (`wsl.exe -l -v`), puis :
   - écrit le binaire, transmis sur l'entrée standard de `wsl.exe`, dans `~/.local/bin/.terragrunt-tui.tmp` et le rend exécutable, puis lance sa commande interne `wsl adopt`, qui fait la suite dans la distro ;
   - cherche un `ttui` ou `terragrunt-tui` existant : PATH du shell de l'utilisateur (`$SHELL -lic`, puis `sh -lc`) et chemins usuels (`/usr/bin`, `/usr/local/bin`, `~/.cargo/bin`, `~/.local/bin`). S'il existe hors d'une installation marquée par ce mécanisme, la copie est supprimée et la distro **ignorée**, sauf `--force` ;
   - renomme atomiquement la copie en `~/.local/bin/terragrunt-tui` et crée le lien `~/.local/bin/ttui` ;
   - pose le **marqueur** (voir plus bas). Le marqueur est un fichier séparé, car le binaire est identique à celui du dépôt apt ;
   - prévient si `~/.local/bin` n'est pas dans le PATH du shell de connexion. Sur Ubuntu, `~/.profile` ne l'ajoute que si le dossier existait à l'ouverture de session : indiquer d'ouvrir un nouveau terminal.
5. Affiche un récapitulatif par distro : installé (version), mis à jour, ignoré (raison), échec (message). Code de sortie non nul si une distro a échoué.

Relancer `ttui wsl install` est idempotent : une copie marquée est remplacée par la version du paquet.

### Marqueur

`~/.local/share/terragrunt-tui/store-install.json` dans la distro :

```json
{
  "source": "microsoft-store",
  "version": "1.5.0",
  "files": ["~/.local/bin/terragrunt-tui", "~/.local/bin/ttui"],
  "windows_exe": "/mnt/c/Users/<utilisateur>/AppData/Local/Microsoft/WindowsApps/ttui.exe"
}
```

`windows_exe` est le chemin WSL de l'alias d'exécution, transmis à l'installation par `WSLENV` (`TTUI_WINDOWS_EXE/p`, que WSL traduit en chemin `/mnt/…`). La copie Linux joint ainsi `ttui.exe` même quand `appendWindowsPath=false`, du moment que l'interop est active.

L'horodatage de la dernière vérification vit dans `~/.cache/terragrunt-tui/store-check`.

### Mise à jour depuis Linux

Une copie Linux qui trouve le marqueur, au démarrage de toute commande sauf `version`, `update` et `--help` :

1. Ne fait rien si la dernière vérification date de moins de 24 h.
2. Exécute `<windows_exe> version`, avec un **timeout de 2 s**. Toute erreur (interop désactivée, timeout, exécutable absent) est silencieuse ; voir « Copie orpheline ».
3. Si la version du paquet est **strictement supérieure** (comparaison semver) à la sienne :
   - récupère le binaire par `<windows_exe> wsl binary`, sous-commande interne et cachée qui écrit `linux/terragrunt-tui` sur la sortie standard ;
   - l'écrit dans un fichier temporaire de `~/.local/bin`, vérifie que `<tmp> version` répond la version attendue, puis le renomme atomiquement par-dessus `terragrunt-tui` et met le marqueur à jour ;
   - affiche une ligne sur stderr (« terragrunt-tui mis à jour 1.4.0 → 1.5.0 depuis le Microsoft Store ») et se relance (`exec`) avec les mêmes arguments.
4. Un échec de remplacement laisse l'ancienne version en place et réessaie au prochain créneau.

`ttui update`, lancé dans une copie marquée, fait les étapes 2 et 3 immédiatement, sans limite de fréquence, et signale clairement une erreur. Sans marqueur, `ttui update` garde son comportement actuel.

### Lancement depuis le menu Démarrer

`store::pause_if_console_closes_on_exit` propose, avant d'attendre Entrée :

> Installer aussi `ttui` dans tes distributions WSL ? [o/N]

- La proposition n'apparaît que si `wsl.exe` existe et liste au moins une distro éligible.
- Elle n'apparaît **qu'une fois**. La réponse est mémorisée dans `%LOCALAPPDATA%\terragrunt-tui\wsl-prompt` ; pour une application empaquetée, Windows redirige cette écriture vers le stockage du paquet.
- « o » lance l'équivalent de `ttui wsl install` sans option.

### `ttui wsl uninstall` (Windows)

Options : `--distro <nom>` (répétable). Mêmes distros que `install`. Dans chaque distro, retire uniquement les fichiers listés par le marqueur, puis le marqueur et l'horodatage. Une distro sans marqueur est signalée « rien à retirer ». Récapitulatif par distro.

La documentation indique de lancer `ttui wsl uninstall` **avant** de désinstaller l'application, puisque la désinstallation Store ne peut pas toucher aux distros.

### Copie orpheline

Si `<windows_exe>` est introuvable ou ne répond pas lors d'une vérification, la copie Linux continue de fonctionner normalement. Au plus une fois par jour, elle affiche sur stderr :

> Paquet Microsoft Store introuvable depuis WSL (application désinstallée ou interop désactivée) : plus de mises à jour automatiques. `ttui update` réessaie ; le dépôt apt reste disponible.

## Build et CI

- `scripts/package-msix.ps1` prend un paramètre `-LinuxTarball <chemin>` vers `terragrunt-tui-<version>-x86_64-unknown-linux-musl.tar.gz` et copie le binaire dans `linux/` du paquet. Le script échoue si l'archive manque ou si la version de son nom ne correspond pas à la version empaquetée : un paquet Store sans binaire Linux ne doit pas partir.
- `.github/workflows/store.yml`, hors `dry_run`, télécharge l'archive du tag avant d'empaqueter :
  `gh release download <tag> --pattern '*-x86_64-unknown-linux-musl.tar.gz'` (avec `GH_TOKEN`, permission `contents: read` déjà accordée). Ça fonctionne aussi en relance manuelle.
- Le runner Windows ne peut pas exécuter le binaire Linux : la vérification de version côté CI se fait sur le nom de l'archive, publiée par le job `release` de la même release (`scripts/build-deb.sh`, assets de [`release.config.cjs`](../../release.config.cjs)). La vérification par exécution a lieu à l'installation et à chaque mise à jour, côté Linux.
- Le MSIX grossit d'environ la taille du binaire musl.

## Documentation à mettre à jour

- [`docs/microsoft-store.md`](../microsoft-store.md) : contenu du paquet (`linux/`), paramètre `-LinuxTarball` pour l'empaquetage manuel, étape de téléchargement dans le workflow, justification `runFullTrust` complétée (l'application lance `wsl.exe` et écrit dans les distros de l'utilisateur à sa demande explicite).
- `README.md`, section Microsoft Store : `ttui wsl install`, mise à jour automatique des copies, `ttui wsl uninstall` avant de désinstaller. Section apt : rappeler que les deux voies ne se mélangent pas dans une même distro.
- Fiche Store : mentionner la commande WSL dans la description.

## Tests

Automatisés (Linux et Windows, sans WSL réel) :

- décodage UTF-16LE de `wsl.exe -l -q` (BOM, `\r\n`, lignes vides) et filtrage `docker-desktop*` / `--distro` ;
- comparaison semver « strictement supérieure » (prerelease, égalité, rétrogradation refusée) ;
- lecture et écriture du marqueur, limite de 24 h de l'horodatage ;
- décision « ignorer / installer / remplacer » selon le résultat de `command -v` et la présence du marqueur ;
- `wsl binary` écrit exactement les octets du fichier embarqué.

Manuels, sur un poste avec WSL, à consigner avec les versions :

- installation Store puis réponse « o » au menu Démarrer, sur une distro Ubuntu arrêtée et une autre en marche ;
- distro avec `terragrunt-tui` installé par apt : ignorée, puis `--force` ;
- mise à jour Store simulée (paquet local de version supérieure installé par `Add-AppxPackage`) : la copie WSL se met à jour et se relance au lancement suivant ;
- `appendWindowsPath=false` dans `/etc/wsl.conf` : la mise à jour passe par `windows_exe` ;
- interop désactivée puis application désinstallée : rappel quotidien, la copie fonctionne ;
- `ttui wsl uninstall` : seuls les fichiers marqués disparaissent.

### Vérifié le 2026-11-21

Sur Windows 11 avec WSL (Ubuntu, shell zsh) et la version Store 1.3.0 installée, avec un binaire musl cross-compilé depuis Windows (`rust-lld`) :

- appel `wsl.exe` identique à celui de `ttui wsl install`, lancé depuis Rust : copie, `wsl adopt`, lien `ttui`, marqueur, chemin de l'alias traduit par `WSLENV` ;
- l'alias d'exécution Store `ttui.exe`, lancé depuis WSL, répond `version` en 200 à 560 ms ;
- resynchronisation 1.3.0 → 9.9.0 au démarrage (faux `ttui.exe`), relance sur le nouveau binaire, pas de nouvelle vérification dans les 24 h ;
- copie orpheline (rappel, `ttui update` en erreur), conflit avec un `ttui` de `~/.cargo/bin` puis `--force`, double `wsl remove` ;
- avertissement PATH correct : le `~/.zshrc` de la distro de test ne contient pas `~/.local/bin` ;
- `scripts/package-msix.ps1` refuse une archive d'une autre version et embarque `linux/terragrunt-tui`.

Non vérifié : `ttui wsl install` / `uninstall` et la proposition du menu Démarrer **depuis le paquet installé**, car il aurait fallu remplacer la version Store et activer le mode développeur sur le poste de test. À faire sur la première release qui embarque ce code.

## Risques

- **Certification Store** : une application qui écrit hors de son conteneur peut susciter des questions. Mitigation : action toujours explicite de l'utilisateur, binaire embarqué dans le paquet (rien n'est téléchargé), justification `runFullTrust` mise à jour.
- **Démarrage des distros** : `ttui wsl install` démarre les distros arrêtées. Le récapitulatif le signale, et `--distro` permet d'éviter les distros lourdes.
- **Interop** : sans interop, plus de mise à jour automatique. Le rappel quotidien le signale ; `ttui wsl install` relancé depuis Windows resynchronise.
