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
- Pendant le run, les journaux bruts de Terragrunt (stdout et stderr) sont **toujours visibles**, diffusés en direct dans l’écran d’exécution au fur et à mesure de leur arrivée, sans bascule ni option pour les cacher. L’avancement (unités détectées, module actif) s’affiche au-dessus. `q`, Échap et Ctrl-C interrompent le processus ; les résultats sont incomplets et les fichiers temporaires sont nettoyés.
- L’interface comporte trois zones Modules → Ressources → Modifications. La flèche droite/Tab avance et compacte les panneaux précédents en contexte/statut ; flèche gauche/Échap revient sans perdre la sélection. Le focus au troisième niveau rend le détail central.
- Chaque compteur/action porte un texte et un repère visuel : `ADD` vert, `DELETE` rouge, `CHANGE`/`REPLACE` orange, `UNKNOWN` rose/magenta. Le sens doit rester compréhensible sans couleur.
- Une ressource bascule de façon réversible entre le before/after et un diff unifié ciblé construit depuis les données structurées. Si les valeurs ne permettent pas un diff honnête, l’interface le dit sans inventer.
- Les modules sont groupés par profondeur des dépendances explicites ; ces vagues décrivent l’éligibilité, pas l’ordre chronologique exact du scheduler concurrent. Graphe manquant ou ambigu : vague inconnue et ordre stable, jamais une déduction depuis les journaux.
- Recherche clavier, aide visible. Dans la revue après le run, `l` bascule l’affichage des journaux conservés en mémoire pour le module sélectionné ; les journaux ne sont pas sérialisés dans le résultat.
- Les erreurs et résultats incomplets ne sont jamais présentés comme un bilan complet.

## Cycle de vie temporaire et redaction

Plans natifs et JSON résident dans un répertoire temporaire unique portant un marqueur d’appartenance. La fin normale ou l’interruption gérée nettoie ce répertoire. Au démarrage, seuls les restes marqués sont détectés ; la suppression n’est jamais automatique et exige une confirmation distincte. Les plans ne sont jamais conservés dans le résultat exporté.

La redaction s’applique aux valeurs sensibles structurées affichées et sauvegardées. L’interface avertit que cette protection ne garantit pas l’absence de secrets dans les journaux, valeurs inconnues, sorties fournisseurs, mémoire ou fichiers natifs temporaires.

## Compatibilité et validation

La crate Rust cible l’édition 2024 et Rust 1.88 minimum, avec versions Cargo épinglées et vérifiées pour Ratatui/Crossterm. Les tests et rendus synthétiques sont exécutés sous Linux. Aucun exécutable Terragrunt, Terraform ou OpenTofu réel n’étant disponible, aucune version minimale de ces outils ni intégration réelle n’est revendiquée ; flags, formats et chemins inconnus doivent échouer de façon conservatrice. Git Bash et PowerShell attendent une validation manuelle après livraison ; WSL reste indisponible selon le dernier retour utilisateur. Aucun déploiement ni publication de binaire n’est autorisé par cette spécification.