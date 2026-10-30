from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections.abc import Iterable
from dataclasses import asdict, dataclass, field
from datetime import UTC, datetime
from pathlib import Path

PLAN_COUNTS = re.compile(r"Plan:\s*(\d+) to add,\s*(\d+) to change,\s*(\d+) to destroy\.")
NO_CHANGES = re.compile(r"No changes\. (?:Your infrastructure matches the configuration|Infrastructure is up-to-date)", re.IGNORECASE)
MODULE = re.compile(r"(?:\bin\s+|\bModule\s+)[\"']?([./][^\s\"']+)", re.IGNORECASE)


@dataclass
class ModuleResult:
    path: str
    status: str = "unknown"
    added: int | None = None
    changed: int | None = None
    destroyed: int | None = None
    detail: list[str] = field(default_factory=list)


@dataclass
class RunResult:
    timestamp: str
    command: list[str]
    exit_code: int
    added: int
    changed: int
    destroyed: int
    modules: list[ModuleResult]


def parse_output(lines: Iterable[str], exit_code: int = 0) -> list[ModuleResult]:
    modules: list[ModuleResult] = []
    modules_by_path: dict[str, ModuleResult] = {}
    current: ModuleResult | None = None
    for raw in lines:
        line = raw.rstrip("\r\n")
        header = MODULE.search(line)
        if header:
            path = header.group(1).rstrip("/")
            current = modules_by_path.get(path)
            if current is None:
                current = ModuleResult(path=path)
                modules_by_path[path] = current
                modules.append(current)
        if current is None:
            current = ModuleResult(path=".")
            modules.append(current)
        current.detail.append(line)
        counts = PLAN_COUNTS.search(line)
        if counts:
            current.added, current.changed, current.destroyed = map(int, counts.groups())
            current.status = "planned"
        elif NO_CHANGES.search(line):
            current.added = current.changed = current.destroyed = 0
            current.status = "no_changes"
        elif re.search(r"(?:error:|fatal:|failed to|error running)", line, re.IGNORECASE):
            current.status = "failed"
    if exit_code and modules and all(m.status != "failed" for m in modules):
        modules[-1].status = "failed"
    return modules


def totals(modules: list[ModuleResult]) -> tuple[int, int, int]:
    added = sum(m.added or 0 for m in modules)
    changed = sum(m.changed or 0 for m in modules)
    destroyed = sum(m.destroyed or 0 for m in modules)
    return added, changed, destroyed


def result_json(result: RunResult) -> str:
    return json.dumps(asdict(result), indent=2, ensure_ascii=False)


def print_summary(result: RunResult) -> None:
    print(f"Plan totals: +{result.added} ~{result.changed} -{result.destroyed}")
    if not result.modules:
        print(f"No module output was parsed (exit {result.exit_code}).")
        return
    for i, module in enumerate(result.modules, 1):
        counts = "unknown" if module.added is None else f"+{module.added} ~{module.changed} -{module.destroyed}"
        print(f"{i}. {module.path} [{module.status}] {counts}")


def run(args: argparse.Namespace) -> int:
    cwd = Path(args.cwd).resolve()
    command = [args.terragrunt, "run", "--all", "plan"]
    try:
        proc = subprocess.run(command, cwd=cwd, text=True, encoding="utf-8", errors="replace", stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    except OSError as exc:
        print(f"Cannot start Terragrunt: {exc}", file=sys.stderr)
        return 2
    for line in proc.stdout.splitlines():
        print(line)
    modules = parse_output(proc.stdout.splitlines(), proc.returncode)
    added, changed, destroyed = totals(modules)
    result = RunResult(datetime.now(UTC).isoformat(), command, proc.returncode, added, changed, destroyed, modules)
    output = Path(args.output).resolve() if args.output else cwd / ".wayfinder" / "latest.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(result_json(result) + "\n", encoding="utf-8")
    print_summary(result)
    print(f"Result: {output}")
    if sys.stdin.isatty() and modules:
        drill_down(modules)
    return proc.returncode


def drill_down(modules: list[ModuleResult]) -> None:
    while True:
        try:
            choice = input("Module number for details, or q to quit: ").strip()
        except EOFError:
            return
        if choice.lower() in {"q", "quit", ""}:
            return
        if not choice.isdigit() or not 1 <= int(choice) <= len(modules):
            print("Choose a listed module number or q.")
            continue
        module = modules[int(choice) - 1]
        print(f"--- {module.path} ({module.status}) ---")
        print("\n".join(module.detail) or "No captured detail.")


def show(args: argparse.Namespace) -> int:
    try:
        data = json.loads(Path(args.run).read_text(encoding="utf-8"))
        result = RunResult(**{**data, "modules": [ModuleResult(**m) for m in data["modules"]]})
    except (OSError, ValueError, KeyError, TypeError) as exc:
        print(f"Cannot read result {args.run}: {exc}", file=sys.stderr)
        return 2
    print_summary(result)
    if sys.stdin.isatty() and result.modules:
        drill_down(result.modules)
    return result.exit_code


def main() -> int:
    parser = argparse.ArgumentParser(prog="wayfinder", description="Run a read-only Terragrunt plan and inspect module results.")
    commands = parser.add_subparsers(dest="action", required=True)
    run_parser = commands.add_parser("run", help="run terragrunt run --all plan")
    run_parser.add_argument("--cwd", default=".")
    run_parser.add_argument("--terragrunt", default="terragrunt")
    run_parser.add_argument("--output")
    run_parser.set_defaults(func=run)
    show_parser = commands.add_parser("show", help="show a saved result")
    show_parser.add_argument("--run", default=".wayfinder/latest.json")
    show_parser.set_defaults(func=show)
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
