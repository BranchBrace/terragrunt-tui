import sys
from pathlib import Path

from textual.widgets import DataTable, Input, Static

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))

import wayfinder
from wayfinder_tui import WayfinderApp


def test_tui_shows_redacted_resource_details_and_hides_logs_by_default():
    module = wayfinder.ModuleResult(
        path="./prod/app",
        status="planned",
        added=1,
        changed=0,
        destroyed=0,
        resources=[
            {
                "address": "aws_instance.web",
                "actions": ["create"],
                "before": None,
                "after": {"name": "web", "password": "[REDACTED]"},
            },
            {"address": "aws_instance.old", "actions": ["delete"], "before": {"name": "old"}, "after": None},
            {"address": "aws_instance.replace", "actions": ["delete", "create"], "before": {"size": "small"}, "after": {"size": "large"}},
            {"address": "aws_instance.update", "actions": ["update"], "before": {"size": "small"}, "after": {"size": "large"}},
        ],
    )
    result = wayfinder.RunResult(
        timestamp="synthetic",
        command=["terragrunt", "run", "--all", "plan"],
        exit_code=0,
        added=1,
        changed=0,
        destroyed=0,
        modules=[module],
        complete=True,
        logs=["synthetic raw log"],
    )
    app = WayfinderApp(result)

    async def exercise():
        async with app.run_test() as pilot:
            await pilot.pause()
            search = app.query_one("#search", Input)
            assert search.content_region.height > 0
            assert app.query_one("#module-table", DataTable).row_count == 1
            resource_table = app.query_one("#resource-table", DataTable)
            assert resource_table.row_count == 4
            assert str(resource_table.get_row_at(1)[1]) == "DELETE"
            assert str(resource_table.get_row_at(2)[1]) == "REPLACE"
            assert str(resource_table.get_row_at(3)[1]) == "UPDATE"
            after = str(app.query_one("#after", Static).render())
            logs = app.query_one("#logs", Static)
            assert "[REDACTED]" in after
            assert "synthetic raw log" not in str(logs.render())
            await pilot.click("#toggle-logs")
            await pilot.pause()
            assert "synthetic raw log" in str(logs.render())
            await pilot.click("#search")
            await pilot.press("m")
            await pilot.pause()
            assert app.query_one("#module-table", DataTable).row_count == 0
            assert "No modules match" in str(app.query_one("#module-status", Static).render())
            await pilot.press("backspace")
            await pilot.press("p", "r", "o", "d")
            await pilot.pause()
            assert app.query_one("#module-table", DataTable).row_count == 1, app.query_one("#search", Input).value

    import asyncio

    asyncio.run(exercise())


def test_tui_arrow_navigation_updates_module_and_resource_details():
    first = wayfinder.ModuleResult(
        path="./prod/app",
        status="planned",
        added=1,
        changed=0,
        destroyed=0,
        resources=[
            {"address": "aws_instance.web", "actions": ["create"], "before": None, "after": {"name": "web"}},
            {"address": "aws_instance.db", "actions": ["update"], "before": {"size": "small"}, "after": {"size": "large"}},
        ],
    )
    second = wayfinder.ModuleResult(
        path="./prod/db",
        status="planned",
        added=0,
        changed=1,
        destroyed=0,
        resources=[
            {"address": "aws_db_instance.main", "actions": ["update"], "before": {"size": "db.t3"}, "after": {"size": "db.t4"}},
        ],
    )
    result = wayfinder.RunResult(
        timestamp="synthetic",
        command=["terragrunt", "run", "--all", "plan"],
        exit_code=0,
        added=1,
        changed=1,
        destroyed=0,
        modules=[first, second],
        complete=True,
    )
    app = WayfinderApp(result)

    async def exercise():
        async with app.run_test(size=(80, 24)) as pilot:
            await pilot.pause()
            assert app.query_one("#before", Static).region.right <= app.size.width
            assert app.query_one("#after", Static).region.bottom <= app.size.height
            await pilot.press("down")
            await pilot.pause()
            assert "./prod/db" in str(app.query_one("#module-status", Static).render())
            await pilot.press("m", "up", "r", "down")
            await pilot.pause()
            assert "large" in str(app.query_one("#after", Static).render())

    import asyncio

    asyncio.run(exercise())


def test_tui_quit_terminates_running_plan_and_cleans_temporary_directory(tmp_path, monkeypatch):
    import asyncio
    import os

    from wayfinder_tui import terminate_process

    terminated = []
    original_terminate = terminate_process

    def record_termination(process):
        terminated.append(process.pid)
        original_terminate(process)

    monkeypatch.setattr("wayfinder_tui.terminate_process", record_termination)

    script = tmp_path / "terragrunt-slow"
    details_file = tmp_path / "running-plan.txt"
    script.write_text(
        "#!/usr/bin/env python3\n"
        "import os, pathlib, sys, time\n"
        "args = sys.argv[1:]\n"
        "if args[0] == 'run':\n"
        "    temp_root = pathlib.Path(args[args.index('--out-dir') + 1]).parent\n"
        "    details = pathlib.Path(sys.argv[0]).with_name('running-plan.txt')\n"
        "    details.write_text(str(os.getpid()) + '\\n' + str(temp_root))\n"
        "    print('Running command: terraform plan in ./unit-a', flush=True)\n"
        "    time.sleep(30)\n",
        encoding="utf-8",
    )
    script.chmod(0o755)
    app = WayfinderApp(cwd=tmp_path, terragrunt=str(script), units=["./unit-a"])

    async def exercise():
        async with app.run_test() as pilot:
            await pilot.click("#start")
            for _ in range(100):
                await pilot.pause(0.05)
                if details_file.exists():
                    break
            assert details_file.exists(), "synthetic plan process did not start"
            await pilot.press("q")
            await pilot.pause()

    asyncio.run(exercise())
    process_id, temp_dir = details_file.read_text(encoding="utf-8").splitlines()
    assert terminated == [int(process_id)], "quitting the TUI did not invoke process-group termination"
    try:
        os.kill(int(process_id), 0)
    except ProcessLookupError:
        pass
    else:
        raise AssertionError("quitting the TUI left the Terragrunt process alive")
    assert not Path(temp_dir).exists(), "quitting the TUI left its temporary plan directory"


def test_tui_requires_explicit_start_and_executes_synthetic_plan(tmp_path):
    script = tmp_path / "terragrunt"
    script.write_text(
        "#!/usr/bin/env python3\n"
        "import json, pathlib, sys\n"
        "args = sys.argv[1:]\n"
        "if args[0] == 'run':\n"
        "    output = pathlib.Path(args[args.index('--json-out-dir') + 1]) / 'unit-a' / 'tfplan.json'\n"
        "    output.parent.mkdir(parents=True)\n"
        "    output.write_text(json.dumps({'resource_changes': []}))\n"
        "    print('Running command: terraform plan in ./unit-a')\n"
        "else:\n"
        "    raise SystemExit(9)\n",
        encoding="utf-8",
    )
    script.chmod(0o755)
    output = tmp_path / "result.json"
    app = WayfinderApp(cwd=tmp_path, terragrunt=str(script), units=["./unit-a"], output=output)

    async def exercise():
        async with app.run_test() as pilot:
            await pilot.pause()
            assert app.result is None
            await pilot.click("#start")
            for _ in range(40):
                await pilot.pause(0.05)
                if app.result is not None:
                    break
            assert app.result is not None
            assert app.result.complete is True
            assert app.result.modules[0].status == "no_changes"
            assert output.exists()

    import asyncio

    asyncio.run(exercise())
