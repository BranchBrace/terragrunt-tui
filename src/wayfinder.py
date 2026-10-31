from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
from collections.abc import Iterable
from dataclasses import asdict, dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import Any

PLAN_COUNTS = re.compile(r"Plan:\s*(\d+) to add,\s*(\d+) to change,\s*(\d+) to destroy\.")
NO_CHANGES = re.compile(r"No changes\. (?:Your infrastructure matches the configuration|Infrastructure is up-to-date)", re.IGNORECASE)
MODULE = re.compile(r"(?:\bin\s+|\bModule\s+)[\"']?((?:[A-Za-z]:[\\/]|[./])[^\s\"']+)", re.IGNORECASE)


def redact_sensitive(value: object, mask: object) -> object:
    if mask is True:
        return "[REDACTED]"
    if isinstance(value, dict) and isinstance(mask, dict):
        return {key: redact_sensitive(item, mask.get(key, False)) for key, item in value.items()}
    if isinstance(value, list) and isinstance(mask, list):
        return [redact_sensitive(item, mask[index] if index < len(mask) else False) for index, item in enumerate(value)]
    return value


def mark_unknown(value: object, mask: object) -> object:
    if mask is True:
        return "[UNKNOWN]"
    if isinstance(value, dict) and isinstance(mask, dict):
        return {key: mark_unknown(item, mask.get(key, False)) for key, item in value.items()}
    if isinstance(value, list) and isinstance(mask, list):
        return [mark_unknown(item, mask[index] if index < len(mask) else False) for index, item in enumerate(value)]
    return value


def parse_plan(plan: dict[str, object]) -> tuple[list[dict[str, object]], tuple[int, int, int]]:
    if plan.get("errored") is True:
        raise ValueError("Terraform plan reports an error")
    if "resource_changes" not in plan:
        raise TypeError("Terraform plan is missing resource_changes")
    changes = plan["resource_changes"]
    if not isinstance(changes, list):
        raise TypeError("Terraform plan resource_changes must be a list")
    resources: list[dict[str, object]] = []
    added = changed = destroyed = 0
    for item in changes:
        if not isinstance(item, dict):
            raise TypeError("Terraform plan contains an invalid resource change")
        change = item.get("change")
        if not isinstance(change, dict):
            raise TypeError("Terraform resource change is missing change data")
        if "actions" not in change:
            raise TypeError("Terraform resource change is missing actions")
        actions = change["actions"]
        if not isinstance(actions, list) or not all(isinstance(action, str) for action in actions):
            raise TypeError("Terraform resource actions must be a list of strings")
        if "create" in actions:
            added += 1
        if "update" in actions:
            changed += 1
        if "delete" in actions:
            destroyed += 1
        resources.append({
            "address": item.get("address", "<unknown address>"),
            "actions": actions,
            "before": redact_sensitive(
                mark_unknown(change.get("before"), change.get("before_unknown", False)),
                change.get("before_sensitive", False),
            ),
            "after": redact_sensitive(
                mark_unknown(change.get("after"), change.get("after_unknown", False)),
                change.get("after_sensitive", False),
            ),
        })
    return resources, (added, changed, destroyed)


@dataclass
class ModuleResult:
    path: str
    status: str = "unknown"
    added: int | None = None
    changed: int | None = None
    destroyed: int | None = None
    detail: list[str] = field(default_factory=list)
    resources: list[dict[str, object]] = field(default_factory=list)


@dataclass
class RunResult:
    timestamp: str
    command: list[str]
    exit_code: int
    added: int
    changed: int
    destroyed: int
    modules: list[ModuleResult]
    schema_version: int = 1
    complete: bool = True
    logs: list[str] = field(default_factory=list, repr=False)


def parse_output(lines: Iterable[str], exit_code: int = 0) -> list[ModuleResult]:
    modules: list[ModuleResult] = []
    modules_by_path: dict[str, ModuleResult] = {}
    current: ModuleResult | None = None
    for raw in lines:
        line = raw.rstrip("\r\n")
        header = MODULE.search(line)
        if header:
            path = header.group(1).rstrip("/\\")
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


def module_key(path: str, cwd: Path) -> str:
    candidate = Path(path)
    if not candidate.is_absolute():
        candidate = cwd / candidate
    return os.path.normcase(os.path.normpath(str(candidate.resolve())))


def result_json(result: RunResult) -> str:
    data = asdict(result)
    data.pop("logs", None)
    for module in data["modules"]:
        module.pop("detail", None)
    return json.dumps(data, indent=2, ensure_ascii=False)


def build_plan_command(terragrunt: str, plan_dir: Path, json_dir: Path) -> list[str]:
    return [
        terragrunt,
        "run",
        "--all",
        "--out-dir",
        str(plan_dir),
        "--json-out-dir",
        str(json_dir),
        "plan",
    ]


def discover_modules(cwd: Path, terragrunt: str) -> list[str]:
    proc = subprocess.run(
        [terragrunt, "list", "--format=json"], cwd=cwd, text=True, encoding="utf-8",
        capture_output=True, check=False,
    )
    if proc.returncode == 0:
        try:
            payload = json.loads(proc.stdout)
            if isinstance(payload, list):
                units = payload
            elif isinstance(payload, dict):
                units = payload.get("units", [])
            else:
                units = []
            paths = [unit if isinstance(unit, str) else unit.get("path") for unit in units if isinstance(unit, (str, dict))]
            if paths and all(isinstance(path, str) for path in paths):
                return [str(Path(path)) for path in paths]
        except (json.JSONDecodeError, TypeError, AttributeError):
            pass
    proc = subprocess.run(
        [terragrunt, "list"], cwd=cwd, text=True, encoding="utf-8", errors="replace",
        capture_output=True, check=False,
    )
    if proc.returncode:
        raise RuntimeError(f"Terragrunt unit discovery failed: {proc.stderr.strip() or proc.stdout.strip()}")
    return [token for token in proc.stdout.split() if token not in {"Type", "Path", "Dependencies", "unit", "stack"}]


def find_stale_temp_dirs() -> list[Path]:
    root = Path(tempfile.gettempdir()).resolve()
    found: list[Path] = []
    for candidate in root.glob("wayfinder-*"):
        if candidate.is_symlink() or not candidate.is_dir():
            continue
        marker = candidate / ".wayfinder-owned"
        try:
            if marker.read_text(encoding="ascii") == "WAYFINDER_TEMP_V1\n":
                found.append(candidate)
        except OSError:
            continue
    return found


def cleanup_stale_temp_dir(path: Path, *, confirmed: bool = False) -> None:
    root = Path(tempfile.gettempdir()).resolve()
    candidate = path.resolve()
    marker = candidate / ".wayfinder-owned"
    if not confirmed:
        raise PermissionError("Explicit confirmation is required to delete a leftover run directory")
    if candidate.parent != root or not candidate.name.startswith("wayfinder-") or path.is_symlink():
        raise ValueError("Refusing to delete a path outside Wayfinder's temporary directory")
    if marker.read_text(encoding="ascii") != "WAYFINDER_TEMP_V1\n":
        raise ValueError("Refusing to delete a directory without the Wayfinder ownership marker")
    shutil.rmtree(candidate)


def execute_plan(cwd: Path, terragrunt: str, units: list[str] | None = None, on_progress=None) -> RunResult:
    units = discover_modules(cwd, terragrunt) if units is None else units
    with TemporaryDirectory(prefix="wayfinder-") as temporary:
        work_dir = Path(temporary)
        (work_dir / ".wayfinder-owned").write_text("WAYFINDER_TEMP_V1\n", encoding="ascii")
        plan_dir = work_dir / "plans"
        json_dir = work_dir / "json"
        command = build_plan_command(terragrunt, plan_dir, json_dir)
        process_options: dict[str, Any] = {"start_new_session": True} if os.name == "posix" else {}
        if os.name == "nt":
            process_options["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
        try:
            process = subprocess.Popen(
                command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, **process_options,
            )
        except OSError as exc:
            raise RuntimeError(f"Cannot start Terragrunt: {exc}") from exc
        logs: list[str] = []
        assert process.stdout is not None
        try:
            for raw in process.stdout:
                line = raw.decode("utf-8", errors="replace").rstrip("\r\n")
                logs.append(line)
                header = MODULE.search(line)
                if on_progress and header:
                    on_progress(header.group(1).rstrip("/\\"))
            exit_code = process.wait()
        except KeyboardInterrupt:
            if process.poll() is None:
                if os.name == "posix":
                    os.killpg(process.pid, signal.SIGTERM)
                elif os.name == "nt":
                    process.send_signal(signal.CTRL_BREAK_EVENT)
                else:
                    process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    if os.name == "posix":
                        os.killpg(process.pid, signal.SIGKILL)
                    else:
                        process.kill()
                    process.wait()
            raise
        modules = parse_output(logs, exit_code)
        by_path = {module_key(module.path, cwd): module for module in modules}
        unit_paths = {module_key(unit, cwd) for unit in units}
        planned_files = list(json_dir.rglob("tfplan.json")) if json_dir.exists() else []
        parsed_paths: set[str] = set()
        for plan_file in planned_files:
            relative = plan_file.parent.relative_to(json_dir).as_posix()
            module_path = f"./{relative}" if relative != "." else "."
            key = module_key(module_path, cwd)
            module = by_path.get(key)
            if module is None:
                module = ModuleResult(path=module_path)
                modules.append(module)
                by_path[key] = module
            try:
                plan = json.loads(plan_file.read_text(encoding="utf-8"))
                resources, counts = parse_plan(plan)
            except (OSError, json.JSONDecodeError, TypeError, ValueError) as exc:
                module.status = "failed"
                module.detail.append(f"Unreadable structured plan: {exc}")
                continue
            module.resources = resources
            module.added, module.changed, module.destroyed = counts
            has_actions = False
            for resource in resources:
                actions = resource.get("actions")
                if isinstance(actions, list) and any(
                    isinstance(action, str) and action != "no-op" for action in actions
                ):
                    has_actions = True
                    break
            module.status = "planned" if has_actions else "no_changes"
            parsed_paths.add(key)
        complete = exit_code == 0 and bool(unit_paths) and parsed_paths == unit_paths
        if not complete:
            for module in modules:
                if module_key(module.path, cwd) not in parsed_paths and module.status != "failed":
                    module.status = "unknown" if exit_code == 0 else "failed"
        if not complete:
            totals_value = (0, 0, 0)
        else:
            totals_value = totals(modules)
        return RunResult(
            datetime.now(UTC).isoformat(), command, exit_code, *totals_value,
            modules, complete=complete, logs=logs,
        )


def print_summary(result: RunResult) -> None:
    if result.complete:
        print(f"Plan totals: +{result.added} ~{result.changed} -{result.destroyed}")
    else:
        print("Plan totals: unavailable (run failed or structured results are incomplete).")
    if not result.modules:
        print(f"No module output was parsed (exit {result.exit_code}).")
        return
    ordered = sorted(result.modules, key=lambda module: (module.status == "no_changes", module.status == "unknown", module.path))
    for i, module in enumerate(ordered, 1):
        counts = "unknown" if module.added is None else f"+{module.added} ~{module.changed} -{module.destroyed}"
        print(f"{i}. {module.path} [{module.status}] {counts}")


def run(args: argparse.Namespace) -> int:
    cwd = Path(args.cwd).resolve()
    output = Path(args.output).resolve() if args.output else cwd / ".wayfinder" / "latest.json"
    try:
        units = discover_modules(cwd, args.terragrunt)
        stale_dirs = find_stale_temp_dirs()
        if stale_dirs and not (sys.stdin.isatty() and sys.stdout.isatty()):
            print("Leftover Wayfinder temp directories found (not deleted automatically):")
            for path in stale_dirs:
                print(f"  - {path}")
            print("Run interactively to review explicit cleanup.")
        if sys.stdin.isatty() and sys.stdout.isatty():
            from wayfinder_tui import WayfinderApp

            app = WayfinderApp(cwd=cwd, terragrunt=args.terragrunt, units=units, output=output)
            app.run()
            if app.result is None:
                return 2
            return app.result.exit_code if app.result.complete else (app.result.exit_code or 2)
        print(f"Scope: {len(units)} Terragrunt unit(s) under {cwd}")
        for unit in units:
            print(f"  - {unit}")
        result = execute_plan(cwd, args.terragrunt, units)
    except (OSError, RuntimeError) as exc:
        print(str(exc), file=sys.stderr)
        return 2
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(result_json(result) + "\n", encoding="utf-8")
    print_summary(result)
    print(f"Result: {output}")
    return result.exit_code or (0 if result.complete else 2)


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
        if data.get("schema_version", 1) != 1:
            raise ValueError(f"Unsupported result schema version: {data.get('schema_version')}")
        result = RunResult(**{**data, "modules": [ModuleResult(**m) for m in data["modules"]]})
    except (OSError, ValueError, KeyError, TypeError) as exc:
        print(f"Cannot read result {args.run}: {exc}", file=sys.stderr)
        return 2
    print_summary(result)
    if sys.stdin.isatty() and sys.stdout.isatty() and result.modules:
        from wayfinder_tui import WayfinderApp

        WayfinderApp(result).run()
    return result.exit_code if result.complete else (result.exit_code or 2)


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
