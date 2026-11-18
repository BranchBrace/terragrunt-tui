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

C'est automatique. Quand le pipeline `quality-release` publie une release stable sur `main`, il appelle [`.github/workflows/store.yml`](../.github/workflows/store.yml). Ce workflow tourne sur un runner Windows :

1. il construit le MSIX depuis le tag avec `scripts/package-msix.ps1` ;
2. il le soumet avec [`scripts/publish-store.ps1`](../scripts/publish-store.ps1), qui passe par l'API de soumission du Microsoft Store.

La nouvelle soumission reprend la fiche, le prix et la classification de la précédente. Seul le paquet change : l'ancien est retiré. La soumission part en certification puis est publiée automatiquement si elle passe, ce qui prend de quelques heures à 3 jours ouvrés. Les release candidates de `rc` ne vont jamais au Store.

L'API refuse de créer une soumission tant qu'une autre est en cours. Le job échoue alors avec un message clair. Il faut terminer ou supprimer la soumission en cours dans Partner Center, puis relancer le workflow `microsoft-store` à la main avec le tag.

### Authentification

Aucun secret n'est stocké. Le job se connecte par OIDC à l'application Entra **github-store-submission** :

- tenant `55a83d13-da5a-48c1-8d14-32229bee09ec` ;
- client `d39b44b7-6b1a-47ec-beda-a212c4224024` ;
- variables du dépôt `STORE_TENANT_ID` et `STORE_CLIENT_ID` ;
- identités fédérées pour la branche `main` de terragrunt-tui et de branchbrace.

L'application doit être associée au compte Partner Center avec le rôle **Gestionnaire** : Paramètres du compte > Gestion des utilisateurs > Applications Microsoft Entra. Cela suppose que le tenant soit d'abord associé au compte (Paramètres du compte > Locataires).

Pour vérifier l'accès sans rien soumettre, lancer le workflow `microsoft-store` à la main, sur `main`, avec `dry_run` coché.

### À la main

Sur un poste Windows avec la toolchain Rust MSVC et le Windows SDK, depuis le tag :

```powershell
git checkout vX.Y.Z
pwsh scripts/package-msix.ps1
```

Le paquet sort dans `dist	tui-for-terragrunt-X.Y.Z.0-x64.msix`, une taille de l'ordre du mégaoctet. Deux façons de le soumettre :

- dans Partner Center : **Mettre à jour** la soumission, **Packages**, téléverser le `.msix`, puis **Soumettre pour certification** ;
- en ligne de commande, après un `az login` sur un compte qui a accès à Partner Center : `pwsh scripts/publish-store.ps1 -Msix <paquet>`.

## Fiche Store

- Catégorie : Outils de développement.
- Prix : gratuit, tous marchés.
- Classification IARC : 3+.
- Politique de confidentialité : section [Confidentialité](../README.md#confidentialit%C3%A9) du README. Partner Center refuse une URL qui contient des accents bruts : utiliser l'URL encodée.
- Captures : `packaging/msix/screenshots/*.png` en 1920×1080, régénérables avec `vhs packaging/msix/screenshots.tape`.
- Justification de `runFullTrust` : application console Win32 qui lance le binaire `terragrunt` de l'utilisateur et lit les fichiers du dépôt choisi ; aucune élévation de privilèges ; Terragrunt n'est pas requis pour tester la revue d'un résultat de démonstration.
