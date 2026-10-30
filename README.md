# Terragrunt Wayfinder

Local read-only CLI/TUI for `terragrunt run --all plan`, aggregation, and per-module drill-down.

## Safety boundary

Wayfinder invokes only Terragrunt `plan`; it does not invoke `apply` or `destroy`. Review generated plans before any later action.

## Run locally

Requires Python 3.11+ and Terragrunt in `PATH`.

```sh
python src/wayfinder.py run --cwd .
python src/wayfinder.py show --run .wayfinder/latest.json
```

The result JSON is written to `.wayfinder/latest.json`; module details are preserved there. Interactive terminals also offer numbered drill-down.

On Windows shells, use the same Python invocation (`python src/wayfinder.py ...`) from the repository root. Use `py -3` if `python` does not select Python 3.11+. In PowerShell, quote paths containing spaces. WSL uses the Linux instructions. Git Bash uses `python` as configured in that shell.

## Manual platform validation (not yet verified)

For each environment (WSL, Git Bash, PowerShell):

1. Install Python 3.11+ and Terragrunt; confirm `python --version` and `terragrunt --version`.
2. From a disposable Terragrunt fixture, run `python src/wayfinder.py run --cwd <fixture>`.
3. Confirm the command invokes only `terragrunt run --all plan`, prints totals and per-module status, and writes `.wayfinder/latest.json`.
4. Confirm a failing module produces a non-zero exit and remains individually inspectable.
5. Run `python src/wayfinder.py show --run .wayfinder/latest.json`; in an interactive terminal, inspect module details.
6. Report shell, OS/version, Python/Terragrunt versions, results, and any error output on the Paperclip issue.

No agent-side verification has been performed on WSL, Git Bash, or PowerShell.

Propriétaire, tous droits réservés : voir `LICENSE`.
