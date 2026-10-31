from __future__ import annotations

import json
from pathlib import Path
from typing import ClassVar

from textual import on
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, DataTable, Footer, Header, Input, Static

from wayfinder import (
    ModuleResult,
    RunResult,
    cleanup_stale_temp_dir,
    execute_plan,
    find_stale_temp_dirs,
    result_json,
)


class CleanupConfirmation(ModalScreen[bool]):
    CSS = """
    CleanupConfirmation { align: center middle; }
    #cleanup-dialog { width: 80%; height: auto; max-height: 80%; padding: 1 2; border: round $warning; background: $surface; }
    #cleanup-options { height: 3; }
    """

    def __init__(self, paths: list[Path]) -> None:
        super().__init__()
        self.paths = paths

    def compose(self) -> ComposeResult:
        with Vertical(id="cleanup-dialog"):
            yield Static(
                "Delete only these verified Wayfinder temporary directories?\n\n"
                + "\n".join(str(path) for path in self.paths)
                + "\n\nThis cannot be undone.",
            )
            with Horizontal(id="cleanup-options"):
                yield Button("Delete listed directories", id="confirm-cleanup", variant="error")
                yield Button("Keep them", id="keep-cleanup")

    @on(Button.Pressed, "#confirm-cleanup")
    def confirm(self) -> None:
        self.dismiss(True)

    @on(Button.Pressed, "#keep-cleanup")
    def keep(self) -> None:
        self.dismiss(False)


class WayfinderApp(App[None]):
    TITLE = "Terragrunt Wayfinder"
    SUB_TITLE = "Plan review only — no apply"
    CSS = """
    #notice { height: 3; padding: 0 1; background: $warning 20%; }
    #summary { height: 2; padding: 0 1; }
    #scope { height: 1fr; padding: 1; border: round $primary; }
    #modules { width: 34%; border: round $primary; }
    #review { width: 66%; border: round $primary; }
    #module-table, #resource-table { height: 1fr; }
    #resource-values { height: 10; }
    #before, #after { width: 1fr; overflow: auto; border: round $secondary; }
    #logs { height: 1fr; overflow: auto; border: round $warning; }
    #actions { height: 3; }
    """
    BINDINGS: ClassVar = [
        ("enter", "open_detail", "Open resource"),
        ("escape", "back", "Back"),
        Binding("l", "toggle_logs", "Toggle logs", priority=True),
        ("q", "quit", "Quit"),
    ]

    def __init__(
        self,
        result: RunResult | None = None,
        *,
        cwd: Path | None = None,
        terragrunt: str = "terragrunt",
        units: list[str] | None = None,
        output: Path | None = None,
    ) -> None:
        super().__init__()
        self.result = result
        self.cwd = cwd
        self.terragrunt = terragrunt
        self.units = units or []
        self.output = output
        self.filtered_modules: list[ModuleResult] = []
        self.active_module: ModuleResult | None = None
        self.logs_visible = False
        self.run_started = result is not None
        self.stale_dirs = find_stale_temp_dirs() if result is None else []

    def compose(self) -> ComposeResult:
        yield Header()
        yield Static(
            "Secrets are masked when Terraform marks them sensitive. This is not a guarantee that logs, unknown values, or other files contain no secrets.",
            id="notice",
        )
        yield Static(id="summary")
        if not self.run_started:
            yield Static(id="scope")
            with Horizontal(id="actions"):
                yield Button("Review leftover temp files", id="cleanup", disabled=not self.stale_dirs)
                yield Button("Run read-only plan", id="start", variant="primary")
                yield Button("Quit", id="quit")
        else:
            yield Input(placeholder="Search modules…", id="search")
            with Horizontal(id="body"):
                with Vertical(id="modules"):
                    yield Static("Modules — modified first", id="module-heading")
                    yield DataTable(id="module-table", cursor_type="row")
                with Vertical(id="review"):
                    yield Static("Select a module to inspect resources", id="module-status")
                    yield DataTable(id="resource-table", cursor_type="row")
                    with Horizontal(id="resource-values"):
                        yield Static("Before", id="before", markup=False)
                        yield Static("After", id="after", markup=False)
            yield Static("Logs hidden. Press l or Toggle logs to show captured output.", id="logs", markup=False)
            yield Button("Toggle logs", id="toggle-logs")
        yield Footer()

    def on_mount(self) -> None:
        if not self.run_started:
            leftovers = "\n\nLeftover Wayfinder temp directories (not deleted automatically):\n" + "\n".join(str(path) for path in self.stale_dirs) if self.stale_dirs else ""
            self.query_one("#scope", Static).update(
                f"Scope: {len(self.units)} unit(s) under {self.cwd}\n"
                + ("\n".join(f"• {unit}" for unit in self.units) if self.units else "No units discovered; run will not start.")
                + leftovers
            )
            self.query_one("#summary", Static).update("Review the scope, then explicitly start the plan.")
            self.query_one("#start", Button).disabled = not bool(self.units)
            return
        self._initialize_tables()
        self._refresh_summary()
        self._show_logs()

    def _initialize_tables(self) -> None:
        modules = self.query_one("#module-table", DataTable)
        modules.add_columns("Path", "Status", "Δ")
        resources = self.query_one("#resource-table", DataTable)
        resources.add_columns("Resource", "Actions")
        self._refresh_modules()
        self.query_one("#logs", Static).styles.display = "none"

    def _refresh_summary(self) -> None:
        if not self.result:
            return
        if self.result.complete:
            message = f"Complete: +{self.result.added}  ~{self.result.changed}  -{self.result.destroyed}"
        else:
            message = "INCOMPLETE — global totals suppressed; inspect failed/unknown modules."
        self.query_one("#summary", Static).update(message)

    def _refresh_modules(self, query: str = "") -> None:
        if not self.result:
            return
        table = self.query_one("#module-table", DataTable)
        table.clear()
        needle = query.casefold()
        self.filtered_modules = sorted(
            self.result.modules,
            key=lambda module: (module.status == "no_changes", module.status == "unknown", module.path.casefold()),
        )
        self.filtered_modules = [module for module in self.filtered_modules if needle in module.path.casefold()]
        for module in self.filtered_modules:
            counts = "unknown" if module.added is None else f"+{module.added} ~{module.changed} -{module.destroyed}"
            table.add_row(module.path, module.status, counts, key=module.path)
        if self.filtered_modules and not self.active_module:
            table.move_cursor(row=0)
            self._select_module(self.filtered_modules[0])

    @on(Input.Changed, "#search")
    def filter_modules(self, event: Input.Changed) -> None:
        self._refresh_modules(event.value)

    @on(DataTable.RowSelected, "#module-table")
    def module_selected(self, event: DataTable.RowSelected) -> None:
        module = next((item for item in self.filtered_modules if item.path == str(event.row_key.value)), None)
        if module:
            self._select_module(module)

    def _select_module(self, module: ModuleResult) -> None:
        self.active_module = module
        self.query_one("#module-status", Static).update(
            f"{module.path} — {module.status} — {len(module.resources)} resource change(s)"
        )
        table = self.query_one("#resource-table", DataTable)
        table.clear()
        for index, resource in enumerate(module.resources):
            actions = resource.get("actions", [])
            action_names = [str(action) for action in actions] if isinstance(actions, list) else []
            if "delete" in action_names and "create" in action_names:
                action_text = "REPLACE"
            elif "delete" in action_names:
                action_text = "DELETE"
            elif "create" in action_names:
                action_text = "CREATE"
            elif "update" in action_names:
                action_text = "UPDATE"
            else:
                action_text = "NO CHANGE"
            table.add_row(str(resource.get("address", "<unknown>")), action_text, key=str(index))
        if module.resources:
            table.move_cursor(row=0)
            self._select_resource(0)
        else:
            self.query_one("#before", Static).update("Before\nNo structured resource changes.")
            self.query_one("#after", Static).update("After\nNo structured resource changes.")

    @on(DataTable.RowSelected, "#resource-table")
    def resource_selected(self, event: DataTable.RowSelected) -> None:
        if event.row_key.value is None:
            return
        try:
            self._select_resource(int(event.row_key.value))
        except (TypeError, ValueError):
            return

    def _select_resource(self, index: int) -> None:
        if not self.active_module or not 0 <= index < len(self.active_module.resources):
            return
        resource = self.active_module.resources[index]
        self.query_one("#before", Static).update("Before\n" + json.dumps(resource.get("before"), indent=2, ensure_ascii=False))
        self.query_one("#after", Static).update("After\n" + json.dumps(resource.get("after"), indent=2, ensure_ascii=False))

    def action_open_detail(self) -> None:
        table = self.query_one("#resource-table", DataTable)
        if table.row_count:
            self._select_resource(table.cursor_row)

    async def action_back(self) -> None:
        self.query_one("#search", Input).focus()

    @on(Button.Pressed, "#toggle-logs")
    def toggle_logs_button(self) -> None:
        self.action_toggle_logs()

    def action_toggle_logs(self) -> None:
        if not self.result:
            return
        self.logs_visible = not self.logs_visible
        logs = self.query_one("#logs", Static)
        logs.styles.display = "block" if self.logs_visible else "none"
        self._show_logs()

    def _show_logs(self) -> None:
        if self.result:
            text = "\n".join(self.result.logs) if self.logs_visible else "Logs hidden. Press l to show or hide captured output."
            self.query_one("#logs", Static).update(text or "No log output captured.")

    @on(Button.Pressed, "#cleanup")
    def request_cleanup(self) -> None:
        if self.stale_dirs:
            self.push_screen(CleanupConfirmation(self.stale_dirs), self._cleanup_result)

    def _cleanup_result(self, confirmed: bool | None) -> None:
        if confirmed:
            for path in self.stale_dirs:
                cleanup_stale_temp_dir(path, confirmed=True)
            self.stale_dirs = find_stale_temp_dirs()
            self.query_one("#cleanup", Button).disabled = not self.stale_dirs
            self.query_one("#summary", Static).update("Confirmed leftover cleanup completed.")

    @on(Button.Pressed, "#quit")
    def quit_from_scope(self) -> None:
        self.exit()

    @on(Button.Pressed, "#start")
    def start_run(self) -> None:
        if self.run_started or not self.cwd:
            return
        self.run_started = True
        self.refresh(recompose=True)
        self.run_worker(self._run_in_thread, thread=True, exclusive=True)

    def _run_in_thread(self) -> None:
        assert self.cwd is not None
        def progress(path: str) -> None:
            self.call_from_thread(self._set_progress, path)
        try:
            result = execute_plan(self.cwd, self.terragrunt, self.units, on_progress=progress)
            if self.output:
                self.output.parent.mkdir(parents=True, exist_ok=True)
                self.output.write_text(result_json(result) + "\n", encoding="utf-8")
            self.call_from_thread(self._finish_run, result)
        except (OSError, RuntimeError) as exc:
            self.call_from_thread(self._fail_run, str(exc))

    def _set_progress(self, path: str) -> None:
        self.query_one("#summary", Static).update(f"Planning: {path}")

    def _finish_run(self, result: RunResult) -> None:
        self.result = result
        self._initialize_tables()
        self._refresh_modules()
        self._refresh_summary()
        self._show_logs()

    def _fail_run(self, message: str) -> None:
        self.query_one("#summary", Static).update(f"Run failed: {message}")
        self.query_one("#notice", Static).update("No complete result was saved. Correct the error, then retry with a fresh run.")
