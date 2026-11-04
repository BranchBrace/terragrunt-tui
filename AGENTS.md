# Terragrunt TUI — règles de livraison

- Branche par changement → PR vers `rc` → CI verte → merge **sans squash** (merge commit). `main` = stable, `rc` = prerelease.
- Release stable : `main` n'accepte que des PR, donc le commit de release (version + CHANGELOG) part sur `release/vX.Y.Z`, déjà tagué ; l'ouvrir en PR vers `main` et la merger (merge commit).
- Commits au format Conventional Commits : ils pilotent semantic-release (`feat` → minor, `fix` → patch, `!` → major).
- La CI (`.github/workflows/quality-release.yml`) détecte la stack : ajouter les scripts `lint`, `typecheck`, `test`, `build` (Node), une solution `.slnx` (.NET) ou un `pyproject.toml` (Python) suffit à les faire exécuter. Semgrep tourne toujours.
- Aucun artefact CI : pas de rapport, trace ni capture Playwright téléversés.
- Images de release : les déclarer dans `deploy/images.txt` ; elles sont publiées sur GHCR à chaque release. Le staging Argo CD se branche ensuite (`staging-gitops-tsuzuri/docs/onboarding.md`).
- Licence MIT : `LICENSE` et `license = "MIT"` dans `Cargo.toml` (`"license": "MIT"` dans tout `package.json`).
- Spécifications : `docs/specs/` ; suivi du travail : projet Paperclip « Terragrunt TUI ».
