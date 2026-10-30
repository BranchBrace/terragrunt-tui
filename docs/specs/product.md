# Product specification

## Goal

Run one Terragrunt `run --all plan` from the selected live working directory, summarize global and per-module create/update/delete counts, report failed modules, and allow a user to inspect each module's captured output.

## Command contract

- `wayfinder run [--cwd PATH] [--terragrunt PATH]`: run `terragrunt run --all plan` once, capture combined output, show it after the command exits, save module details in the JSON result, and parse Terraform plan summaries and module headers.
- `wayfinder show [--run PATH]`: display a prior JSON result.
- No apply/destroy or arbitrary pass-through command is accepted.
- Exit 0 when Terragrunt exits successfully; return its nonzero exit code on execution failure. Unknown module output is preserved and shown as unknown, not treated as a successful parse or silently guessed.

## Result model

A run has timestamp, command, overall exit code, global totals, and modules. A module has stable path, status (`planned`, `no_changes`, `failed`, `unknown`), optional added/changed/destroyed counts, and captured detail text.

## Safety and assumptions

The tool never modifies infrastructure state. Terragrunt output formats vary by version, so parsing is conservative: unknown lines remain in details and unknown module totals are not guessed. A run-level failure is surfaced, never represented as a successful plan. Execution is local and synchronous; there is no remote service or telemetry.

## TUI

Use an interactive numbered module list with an explicit detail action. If stdin is not a terminal, render a readable summary and leave details in the saved JSON result.

## Platform delivery

Ship a Python standard-library CLI to avoid runtime-specific native binaries. Provide installation and invocation instructions for Linux/WSL, Git Bash, and PowerShell. Only Linux is agent-verified; the user manually validates the other shells.
