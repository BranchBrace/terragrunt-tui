# Spécification produit — Terragrunt TUI

## Objectif et frontières

Fournir une CLI/TUI Rust locale en lecture seule pour découvrir les unités Terragrunt, lancer une unique commande `terragrunt run --all plan`, agréger les résultats structurés, puis permettre une revue interactive des modules, ressources et changements. Terragrunt TUI n’exécute ni `apply` ni `destroy`, n’accepte aucun passthrough de commande et n’utilise aucun service distant. Toute application ultérieure reste une action séparée, manuelle.

## Contrat d’exécution

- Le binaire est distribué sous deux noms équivalents : `terragrunt-tui` et l’alias court `ttui`.
- `terragrunt-tui run [--cwd PATH] [--terragrunt PATH] [--output PATH]` découvre d’abord les unités et leur graphe par `terragrunt find --dependencies --queue-construct-as=plan --format=json`. Il exige un terminal interactif et présente la commande, la racine, le périmètre, les effets et les limites avant d’attendre une confirmation explicite.
- Après confirmation, la seule commande de plan est `terragrunt run --all --out-dir <temp>/plans --json-out-dir <temp>/json plan`.
- `terragrunt-tui show [--run PATH]` lit un JSON de résultat Terragrunt TUI sauvegardé ; il n’exécute aucun plan.
- Les ressources sont lues exclusivement depuis les fichiers `tfplan.json` par unité. Les journaux ne servent ni à calculer des valeurs/actions/totaux ni à construire les diffs.
- Sortie manquante, illisible, erreur d’unité, échec de commande ou annulation : résultat incomplet, totaux globaux supprimés, code de sortie non nul. Une annulation identifie explicitement le résultat comme incomplet.

## Modèle de résultat

Le modèle versionné conserve la commande exacte, le code de sortie, la complétude, l’annulation, les totaux (uniquement si tout le run est complet) et les unités. Chaque unité porte un chemin, un statut (`planned`, `no_changes`, `failed`, `unknown`), une vague éventuellement inconnue, des compteurs et des ressources. Chaque ressource porte l’adresse, les actions et les valeurs avant/après. Les champs marqués `*_sensitive` sont remplacés par `[REDACTED]`, les champs `*_unknown` par `[UNKNOWN]`. Une action de remplacement (`delete` + `create`) est affichée `REPLACE`, comptée comme un ajout et une suppression. Les actions non reconnues restent visibles comme `UNKNOWN`, marquent l’unité et le run incomplets et suppriment les totaux ; les champs `before`/`after` absents font échouer le parsing plutôt que d’être inventés comme `null`.

## Revue interactive

- Avant le run, un écran de confirmation présente la racine, le nombre d’unités, la commande exacte, les effets et limites, puis la liste navigable de tous les modules groupés/triés par niveau de dépendance. Chaque ligne indique sa vague ; les dépendances inconnues sont explicitement marquées `VAGUE INCONNUE`. `↑/↓`, `PgUp/PgDn`, `Home/End` permettent de parcourir la liste avant confirmation. La vague est un niveau d’éligibilité dérivé du graphe, pas une garantie d’ordre runtime du scheduler concurrent.
- L’avancement est affiché à mesure que les sorties Terragrunt identifient des unités. `q`, Échap et Ctrl-C interrompent le processus ; les résultats sont incomplets et les fichiers temporaires sont nettoyés.
- L’écran de revue s’ouvre sur deux panneaux : Modules, qui a le focus, et Ressources, qui montre les ressources du module sélectionné. `→`/Tab avance vers Ressources : Modules se réduit à une bande (en-têtes de vague abrégés, arbre, nom court, compteurs conservés) et le troisième panneau Modifications apparaît. `→` une seconde fois donne le focus à Modifications, où `↑/↓` font défiler le diff ; Ressources se réduit à son tour. `←`/Échap revient sans perdre la sélection ; Échap sur Modules quitte. Changer de module remet la ressource et le défilement à zéro.
- Dans Modules, la liste porte un en-tête de colonnes `Path · Status · Δ`. Chaque vague est un en-tête collé à gauche (`VAGUE n`, `VAGUE INCONNUE`) et ses modules sont indentés dessous, sous un guide d’arbre coloré par vague. Chaque module tient sur une ligne : chemin (tronqué par la gauche), statut en toutes lettres, compteurs `+n ~n -n`. Tri : vague, puis modules modifiés d’abord, puis chemin.
- Tous les écrans partagent un thème unique (palette ELIA36 sur fond bleu nuit, voir `revue-maquette.html`). Chaque statut et chaque action portent un texte ou un glyphe et un repère de couleur : ajouts et `PLANNED` turquoise, suppressions et `FAILED` rouge, changements orange, remplacements doré, inconnu rose ; les compteurs à zéro sont atténués. Le sens doit rester compréhensible sans couleur.
- Le détail d’une ressource est par défaut un diff unifié façon git construit depuis les valeurs structurées : JSON mis en forme, trois lignes de contexte, plages identiques repliées, lignes retirées/ajoutées sur fond coloré, valeur changée surlignée, bandeau explicite pour un remplacement. `d` bascule de façon réversible vers le before/after brut. Si les valeurs ne permettent pas un diff honnête, l’interface le dit sans inventer.
- Les modules sont groupés par profondeur des dépendances explicites ; ces vagues décrivent l’éligibilité, pas l’ordre chronologique exact du scheduler concurrent. Graphe manquant ou ambigu : vague inconnue et ordre stable, jamais une déduction depuis les journaux.
- Recherche clavier, aide visible, journaux masqués par défaut ; les journaux ne sont pas sérialisés dans le résultat.
- Les erreurs et résultats incomplets ne sont jamais présentés comme un bilan complet.

## Cycle de vie temporaire et redaction

Plans natifs et JSON résident dans un répertoire temporaire unique portant un marqueur d’appartenance. La fin normale ou l’interruption gérée nettoie ce répertoire. Au démarrage, seuls les restes marqués sont détectés ; la suppression n’est jamais automatique et exige une confirmation distincte. Les plans ne sont jamais conservés dans le résultat exporté.

La redaction s’applique aux valeurs sensibles structurées affichées et sauvegardées. L’interface avertit que cette protection ne garantit pas l’absence de secrets dans les journaux, valeurs inconnues, sorties fournisseurs, mémoire ou fichiers natifs temporaires.

## Compatibilité et validation

La crate Rust cible l’édition 2024 et Rust 1.88 minimum, avec versions Cargo épinglées et vérifiées pour Ratatui/Crossterm. Les tests et rendus synthétiques sont exécutés sous Linux. Aucun exécutable Terragrunt, Terraform ou OpenTofu réel n’étant disponible, aucune version minimale de ces outils ni intégration réelle n’est revendiquée ; flags, formats et chemins inconnus doivent échouer de façon conservatrice. Git Bash et PowerShell attendent une validation manuelle après livraison ; WSL reste indisponible selon le dernier retour utilisateur. Aucun déploiement ni publication de binaire n’est autorisé par cette spécification.