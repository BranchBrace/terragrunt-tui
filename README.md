# Terragrunt Wayfinder

CLI/TUI locale et en lecture seule pour préparer et inspecter un `terragrunt run --all plan`. Wayfinder découvre les unités avant l’exécution, montre la commande exacte, puis agrège les changements depuis les JSON de plan structurés. Il n’exécute jamais `apply` ni `destroy` et ne contacte aucun service distant.

## Prérequis et limites de compatibilité

- Rust 1.88+ et Cargo.
- Terragrunt disposant des commandes `find --dependencies --queue-construct-as=plan --format=json`, `run --all`, `--out-dir` et `--json-out-dir`.
- Terraform ou OpenTofu compatible avec la génération du JSON de plan par Terragrunt.
- Aucun binaire Terragrunt/Terraform/OpenTofu réel n’était disponible pour les vérifications de l’agent. Les versions minimales et la compatibilité d’une version Terragrunt donnée ne sont donc pas revendiquées ; seuls les tests Linux synthétiques sont vérifiés ici.

## Exécuter

Depuis la racine du dépôt, dans un terminal interactif. Remplacer `./fixture-terragrunt-jetable` par le chemin d’une fixture locale jetable (aucune fixture Terragrunt n’est fournie dans le dépôt) :

```sh
cargo run -- run --cwd ./fixture-terragrunt-jetable
```

Wayfinder découvre d’abord les unités puis affiche, avant toute exécution, la liste complète des modules triés par vagues de dépendances. Les vagues inconnues restent identifiées comme telles ; elles ne sont jamais déduites des journaux. Parcourir cette liste avec ↑/↓, PgUp/PgDn, Home/End ; Entrée confirme le plan affiché, `q` ou Échap annule. La vue précise que ces vagues représentent les dépendances, pas l’ordre runtime garanti. Pendant le plan, `q`, Échap ou Ctrl-C interrompent le processus et marquent le résultat incomplet. Après le run, le parcours de revue propose trois panneaux : Modules → Ressources → Modifications.

Pour ouvrir un résultat Wayfinder déjà sauvegardé :

```sh
cargo run -- show --run ./.wayfinder/latest.json
```

Par défaut, `run` écrit le résultat redacted dans `.wayfinder/latest.json` sans écraser un fichier existant. `--output PATH` choisit un autre chemin ; `show --run PATH` ouvre ce fichier. La commande `run` exige un terminal interactif afin qu’aucun plan ne parte sans confirmation visible.

## Navigation clavier

| Touche | Action |
|---|---|
| `→` ou `Tab` | Passer de Modules à Ressources, puis à Modifications ; le panneau quitté se compacte. |
| `←` ou `Échap` | Revenir au panneau précédent en gardant la sélection. |
| `↑` / `↓` | Parcourir les éléments du panneau focalisé. |
| `/` | Rechercher un module ou une ressource ; Entrée valide le filtre, Échap le termine. |
| `d` | Basculer entre les valeurs before/after et un diff ciblé. |
| `l` | Afficher ou masquer les journaux du processus en mémoire. Ils sont masqués par défaut et absents du JSON sauvegardé. |
| `q` | Quitter la revue. |

Les compteurs ont des libellés textuels et des couleurs : `ADD` vert, `DELETE` rouge, `CHANGE`/`REPLACE` orange, `UNKNOWN` rose/magenta. Le statut reste lisible sans couleur. Le diff est calculé uniquement depuis les données structurées et n’invente rien si le before/after manque.

Les groupes `W0`, `W1`, etc. représentent la profondeur du graphe de dépendances explicites découvertes. Ce sont des groupes d’éligibilité, pas l’ordre chronologique exact du scheduler : la concurrence peut changer l’ordre réel. Si la source de dépendances est absente ou ambiguë, Wayfinder affiche une vague inconnue au lieu de la déduire depuis les journaux.

## Sécurité et fichiers temporaires

Les plans natifs et JSON structurés restent dans un répertoire temporaire marqué, supprimé après fin normale ou interruption gérée. Au démarrage, seuls les restes munis du marqueur Wayfinder sont proposés ; rien n’est supprimé automatiquement et leur nettoyage demande une confirmation distincte. Le JSON sauvegardé ne contient ni fichiers natifs ni journaux bruts.

Les attributs signalés sensibles par le plan sont masqués dans les valeurs before/after et dans les changements sauvegardés. Cette protection ne garantit pas l’absence de secrets dans les journaux, les sorties fournisseur, les valeurs inconnues, la mémoire ou les fichiers natifs temporaires. Pour une première utilisation, choisir une fixture jetable sans données sensibles et inspecter le plan avant toute action séparée.

## Installation et tests par shell

Depuis la racine du dépôt :

- Linux / WSL : `cargo run -- run --cwd ./fixture-terragrunt-jetable`
- Git Bash : installer Rust/Cargo et Terragrunt pour cet environnement, puis utiliser la même commande.
- PowerShell : installer Rust/Cargo et Terragrunt, puis `cargo run -- run --cwd .\fixture-terragrunt-jetable`.

Dans chaque cas, remplacer ce chemin par la fixture jetable créée localement ; aucune fixture n’est fournie dans le dépôt.

L’agent vérifie uniquement Linux. WSL était indisponible lors du retour précédent ; Git Bash n’a pas été retesté sur la nouvelle version ; l’interface PowerShell était jugée insuffisante sur la version précédente. Ces plateformes ne sont pas revendiquées vérifiées. Après livraison, utiliser seulement une fixture jetable et consigner séparément pour chaque shell les versions, la confirmation, progression, navigation, recherche, changements, erreurs, redaction et nettoyage.

## Vérifications Linux synthétiques

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --locked
```

Les tests utilisent un Terragrunt simulé, des JSON synthétiques et le backend terminal de Ratatui ; aucun plan d’infrastructure réel n’est exécuté. Voir `docs/specs/structured-plan-spike.md` pour les limites d’intégration.

Propriétaire, tous droits réservés : voir `LICENSE`.