# Product specification

## Goal

Run one local, read-only Terragrunt `run --all plan`, render a trustworthy global and per-module review, and allow interactive inspection of Terraform/OpenTofu resource actions and before/after values before any later manual apply.

## Command contract

- `wayfinder run [--cwd PATH] [--terragrunt PATH] [--output PATH]` discovers the unit scope before execution. In an interactive terminal, Wayfinder displays that scope and waits for an explicit start action. Non-interactive execution prints the scope before running.
- The only plan invocation is `terragrunt run --all --out-dir <unique-temp>/plans --json-out-dir <unique-temp>/json plan`. No apply, destroy, arbitrary passthrough, or remote API is used.
- `wayfinder show [--run PATH]` reads a saved, redacted result JSON.
- Structured resource data is read from per-unit `tfplan.json` files. Console logs are not used to infer resource values.
- Missing, malformed, errored, or incomplete module output suppresses global totals and returns a nonzero status; partial results are not presented as a complete global plan.

## Result model

A result stores schema version, timestamp, exact command, run exit code, completeness, global totals (only meaningful when complete), and modules. A module stores a stable path, status (`planned`, `no_changes`, `failed`, `unknown`), optional counts, and resource changes. Each resource stores address, action sequence, and before/after values; Terraform marks sensitive values as `[REDACTED]` and unknown values as `[UNKNOWN]`. Replacement (`delete` + `create`) is shown explicitly and counted as one addition plus one deletion.

## Interactive review

- TUI presents a scope confirmation before executing; progress updates as Terragrunt reports units.
- After execution, changed modules are listed first; all modules remain searchable. Keyboard navigation opens a module and its resource details.
- Create, update, delete, and replacement actions are labeled; before/after values are available per resource.
- Logs are hidden by default and can be shown explicitly. Logs are not saved in the JSON result.
- The interface warns that Terraform sensitive markers are redacted, but this does not guarantee that logs, unknown values, files, memory, or provider output contain no secrets.
- Incomplete runs/errors are prominent and never masquerade as successful totals.

## Temporary-file lifecycle

Native plans and JSON output are created under a unique `wayfinder-*` temporary directory with an ownership marker. Normal completion and handled interruption clean the directory. A later run detects only marked Wayfinder directories and offers deletion; it never deletes leftovers automatically. A cleanup action requires explicit confirmation in the TUI. Plans are not retained as part of the saved result.

## Compatibility and validation

Python 3.11+ and Textual are runtime dependencies. Terragrunt must support `run --all`, `--out-dir`, `--json-out-dir`, and unit listing. The exact minimum Terragrunt and Terraform/OpenTofu versions are not yet established because those binaries are unavailable in the agent workspace; the Linux suite uses synthetic fixtures and a simulated Terragrunt executable only. Unsupported flags or unreadable output fail closed. Agent verification is Linux-only; WSL, Git Bash, and PowerShell require manual validation by the user on the delivered revision.

## Safety and assumptions

Wayfinder invokes only plan and read-only inspection paths; it never modifies infrastructure state. Any later apply remains a separate, manual action. The product uses no telemetry or remote service. Do not use a production fixture or transmit real secrets during compatibility testing.
