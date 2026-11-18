# Publication sur le Microsoft Store

Terragrunt TUI est publié sur le Microsoft Store sous le nom **ttui for Terragrunt** : « Terragrunt » est une marque de Gruntwork, et un nom d'application qui commence par une marque tierce est souvent refusé à la certification. La forme descriptive « … for Terragrunt » évite ce refus.

## Identité du paquet

Valeurs réservées dans Partner Center (compte développeur individuel « Antoine Monnier », commun à BranchBrace) et reprises dans [`packaging/msix/AppxManifest.xml`](../packaging/msix/AppxManifest.xml) :

| Champ | Valeur |
|---|---|
| Name | `AntoineMonnier.ttuiforTerragrunt` |
| Publisher | `CN=94794141-B9ED-4FA9-B74B-C1FC94E6B207` |
| PublisherDisplayName | `Antoine Monnier` |
| Store ID | `9MSXGP354MFL` |

## Le paquet

- Application console empaquetée en MSIX : `desktop4:Subsystem="console"`, et deux alias d'exécution (`terragrunt-tui.exe`, `ttui.exe`) qui pointent vers le même binaire et le mettent dans le PATH de tout terminal.
- Capacité restreinte `runFullTrust`, obligatoire pour une application Win32 empaquetée.
- Langue unique `fr-FR` : l'interface n'existe qu'en français, et chaque langue déclarée exigerait sa propre fiche Store.
- Paquet non signé : le Store le signe après certification. Aucun certificat n'est nécessaire.
- Version : le Store impose 4 parties, la dernière à 0, en hausse à chaque soumission. `X.Y.Z` devient `X.Y.Z.0`. Seules les releases stables sont publiées.
- Logos : `packaging/msix/Assets/`, générés depuis `packaging/msix/icon.svg` par `scripts/render-icons.sh` (ImageMagick 7). Le script d'empaquetage indexe ces variantes dans `resources.pri` (`makepri`).
- Comportement Store, dans [`src/store.rs`](../src/store.rs) :
  - `ttui update` ouvre la fiche du Store au lieu de recompiler une release ;
  - lancée depuis le menu Démarrer, la console attend Entrée avant de se fermer et rappelle que l'outil s'utilise dans un terminal.

## Publier une nouvelle version

Sur un poste Windows avec la toolchain Rust MSVC et le Windows SDK, depuis le tag de la release stable :

```powershell
git checkout vX.Y.Z
pwsh scripts/package-msix.ps1
```

Le paquet sort dans `dist\ttui-for-terragrunt-X.Y.Z.0-x64.msix`, une taille de l'ordre du mégaoctet. Dans Partner Center : **Mettre à jour** la soumission, **Packages**, téléverser le `.msix` puis supprimer l'ancien paquet, et **Soumettre à la Store**. La certification prend de quelques heures à 3 jours ouvrés.

Le CLI `msstore` peut automatiser cette étape une fois configuré. Il faut d'abord une application Entra ID associée à Partner Center (tenantId, sellerId, clientId, clientSecret), puis lancer `msstore reconfigure`. Il n'est pas configuré pour l'instant.

## Fiche Store

- Catégorie : Outils de développement.
- Prix : gratuit, tous marchés.
- Classification IARC : 3+.
- Politique de confidentialité : section [Confidentialité](../README.md#confidentialit%C3%A9) du README. Partner Center refuse une URL qui contient des accents bruts : utiliser l'URL encodée.
- Captures : `packaging/msix/screenshots/*.png` en 1920×1080, régénérables avec `vhs packaging/msix/screenshots.tape`.
- Justification de `runFullTrust` : application console Win32 qui lance le binaire `terragrunt` de l'utilisateur et lit les fichiers du dépôt choisi ; aucune élévation de privilèges ; Terragrunt n'est pas requis pour tester la revue d'un résultat de démonstration.
