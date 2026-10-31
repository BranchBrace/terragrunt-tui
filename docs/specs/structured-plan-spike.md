# Structured-plan spike (Linux, synthetic only)

## Finding

The supported implementation path is Terragrunt's `run --all` stack mode with its documented `--out-dir` and `--json-out-dir` options. Terragrunt writes native plan files per unit and emits machine-readable `tfplan.json` files per unit by invoking Terraform/OpenTofu `show -json`. Wayfinder places both output roots under one unique temporary run directory and reads only the structured JSON; it does not parse resource values from console logs.

Official reference: https://terragrunt.gruntwork.io/docs/reference/cli/commands/run (sections `--json-out-dir`, `--out-dir`; examples show `terragrunt run --all --out-dir /tmp/all --json-out-dir /tmp/all plan`).

## Local environment and test boundary

- Agent environment: Linux; Python `3.14.7` verified via `uv run python --version`.
- `terragrunt` and `terraform` are not installed in this workspace (`command -v` returned no path for either).
- Therefore no Terragrunt or Terraform version is claimed as tested, and no real infrastructure plan was run.
- Synthetic Terragrunt executable exercised unit discovery, the exact planned argument list, per-unit structured output creation/parsing, interruption handling, and temporary-directory cleanup. This verifies Wayfinder's orchestration contract only; it does not substitute for a real-version integration run.

## Implemented contract

1. Discover units before plan execution; show the discovered scope and require an explicit TUI action to start in an interactive terminal.
2. Run only `terragrunt run --all --out-dir <private-run-dir>/plans --json-out-dir <private-run-dir>/json plan`.
3. Parse each `<unit>/tfplan.json`, redact `before_sensitive` and `after_sensitive` recursively, and compute counts from resource actions. Create+delete is counted as replacement (one add and one destroy).
4. Treat missing/malformed/errored structured output or failed Terragrunt exit as incomplete; suppress global totals rather than present partial counts.
5. Keep native and JSON plan files inside an isolated temporary directory; clean it on normal completion and handled interruption. Leftovers are detected by a Wayfinder ownership marker and are never removed automatically. Interactive cleanup requires a second, explicit confirmation.
6. Logs remain hidden by default and are excluded from saved JSON results. The UI warns that masking is not a guarantee of secret-free logs, unknown values, memory, or disk contents.

## Unresolved compatibility boundary

This implementation requires a Terragrunt version that supports `run --all`, `--out-dir`, `--json-out-dir`, and unit listing. Because no real executable is available, the minimum compatible Terragrunt version, Terraform/OpenTofu versions, and actual path/format behavior are not established here. Unsupported flags or structured output fail closed as an incomplete run. Validate on a real disposable fixture before claiming a Terragrunt version supported; do not plan against production infrastructure for this test.
