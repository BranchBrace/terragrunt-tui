# Terragrunt Wayfinder — règles de livraison

- Branche par changement → PR vers `rc` → CI verte → merge **sans squash** (merge commit). `main` = stable, `rc` = prerelease.
- Commits au format Conventional Commits : ils pilotent semantic-release (`feat` → minor, `fix` → patch, `!` → major).
- La CI (`.github/workflows/quality-release.yml`) détecte la stack : ajouter les scripts `lint`, `typecheck`, `test`, `build` (Node), une solution `.slnx` (.NET) ou un `pyproject.toml` (Python) suffit à les faire exécuter. Semgrep tourne toujours.
- Aucun artefact CI : pas de rapport, trace ni capture Playwright téléversés.
- Images de release : les déclarer dans `deploy/images.txt` ; elles sont publiées sur GHCR à chaque release. Le staging Argo CD se branche ensuite (`staging-gitops-tsuzuri/docs/onboarding.md`).
- Licence propriétaire : `LICENSE` (All Rights Reserved) et `"license": "UNLICENSED"` dans tout `package.json`.
- Spécifications : `docs/specs/` ; suivi du travail : projet Paperclip « Terragrunt Wayfinder ».
