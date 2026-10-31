# Terragrunt Wayfinder

Local, read-only CLI/TUI for reviewing a Terragrunt `run --all plan` across units. It summarizes complete plans and lets you inspect per-resource actions with before/after values. No apply, destroy, telemetry, or remote service.

## Prerequisites

- Python 3.11+ and `uv` (Textual is installed from the project dependency).
- Terragrunt with support for `run --all`, `--out-dir`, `--json-out-dir`, and `list`.
- The agent workspace did not include Terragrunt or Terraform/OpenTofu, so no real tool version is claimed as tested. See [the structured-plan spike](docs/specs/structured-plan-spike.md).

Install the project dependencies from the repository root:

```sh
uv sync
```

## Run locally

In an interactive terminal:

```sh
uv run wayfinder run --cwd ./fixture
```

Wayfinder first shows the discovered unit scope. Press the explicit start button to run the plan. The TUI then provides live module progress, searchable module navigation, resource actions, and before/after details. Logs stay hidden unless you press `l`.

Non-interactive usage runs after printing the scope:

```sh
uv run wayfinder run --cwd ./fixture --output ./wayfinder-result.json
uv run wayfinder show --run ./wayfinder-result.json
```

The saved result contains redacted structured resource data but does not contain raw logs or native plan files. A failed, unreadable, or incomplete run suppresses global totals and returns a nonzero exit code.

## Keyboard navigation

| Key | Action |
|---|---|
| `/` | Focus the module search field. |
| `m` | Focus the module list; use ↑/↓ to move, with details updating as you move. |
| `r` | Focus the resource list; use ↑/↓ to inspect before/after values live. |
| `l` | Show or hide captured logs. |
| `Esc` | Return focus to module search. |
| `q` | Quit; if a plan is running, stop its process and clean the temporary run directory. |

The footer also displays these shortcuts. Module statuses and resource actions use distinct colors; selection follows the cursor, so Enter is not required for routine browsing.

## Safety and temporary files

Wayfinder runs only `terragrunt run --all ... plan`; it does not offer apply, destroy, or arbitrary command passthrough. Use a disposable fixture for first runs, and inspect the proposed plan before any separate manual action.

Native plan and JSON files live under a unique temporary run directory, which is removed after normal completion or handled interruption. If interrupted abruptly, Wayfinder detects only temporary directories bearing its ownership marker. Interactive cleanup requires a separate confirmation; non-interactive mode reports leftovers but never deletes them automatically.

Terraform-marked sensitive attributes are redacted in structured before/after values. Logs are hidden by default and only available in the current process after pressing `l`. This masking is not a guarantee that logs, unknown values, provider output, memory, or other files contain no secrets.

## Shell instructions

Run from the repository root in each shell:

- Linux / WSL: `uv run wayfinder run --cwd ./fixture`
- Git Bash: install `uv` and Terragrunt for that shell, then use the same command.
- PowerShell: install `uv` and Terragrunt, then run `uv run wayfinder run --cwd .\fixture`.

Manual results on the previous revision: WSL was unavailable; Git Bash and PowerShell launched and accepted a few keys, but navigation and presentation were not usable. This improved revision has not yet been manually checked. Use only a disposable Terragrunt fixture and no sensitive credentials. For each shell, record OS version and `terragrunt --version`, then verify scope, progress, changed/no-change modules, create/update/delete/replace details, redaction, error handling, keyboard search/navigation, and cleanup. Report skipped shells as untested.

## Linux synthetic checks

```sh
uv run pytest -q
uv run ruff check src tests
```

Tests use synthetic JSON and a simulated Terragrunt executable; they do not perform a real infrastructure plan.

Propriétaire, tous droits réservés : voir `LICENSE`.
