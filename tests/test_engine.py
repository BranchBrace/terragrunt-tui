import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))

import wayfinder


def test_run_uses_simulated_terragrunt_structured_output_and_cleans_temp_files(tmp_path):
    script = tmp_path / "terragrunt"
    script.write_text(
        "#!/usr/bin/env python3\n"
        "import json, pathlib, sys\n"
        "args = sys.argv[1:]\n"
        "if args[0] == 'list':\n"
        "    print(json.dumps(['./unit-a']))\n"
        "elif args[0] == 'run':\n"
        "    output = pathlib.Path(args[args.index('--json-out-dir') + 1]) / 'unit-a' / 'tfplan.json'\n"
        "    output.parent.mkdir(parents=True)\n"
        "    output.write_text(json.dumps({'resource_changes': [{'address': 'aws_instance.web', 'change': {'actions': ['create'], 'before': None, 'after': {'name': 'web'}, 'before_sensitive': False, 'after_sensitive': False}}]}))\n"
        "    print('Running command: terraform plan in ./unit-a')\n"
        "else:\n"
        "    raise SystemExit(9)\n",
        encoding="utf-8",
    )
    script.chmod(0o755)

    result = wayfinder.execute_plan(tmp_path, str(script))

    assert result.complete is True
    assert (result.added, result.changed, result.destroyed) == (1, 0, 0)
    assert [(module.path, module.status) for module in result.modules] == [("./unit-a", "planned")]
    run_args = result.command
    temp_root = Path(run_args[run_args.index("--out-dir") + 1]).parent
    assert not temp_root.exists()
    saved_json = wayfinder.result_json(result)
    assert '"schema_version": 1' in saved_json
    assert '"logs"' not in saved_json
    assert '"detail"' not in saved_json


def test_unreadable_structured_output_suppresses_global_totals(tmp_path):
    script = tmp_path / "terragrunt-bad-output"
    script.write_text(
        "#!/usr/bin/env python3\n"
        "import json, pathlib, sys\n"
        "args = sys.argv[1:]\n"
        "if args[0] == 'list':\n"
        "    print(json.dumps(['./unit-a']))\n"
        "elif args[0] == 'run':\n"
        "    output = pathlib.Path(args[args.index('--json-out-dir') + 1]) / 'unit-a' / 'tfplan.json'\n"
        "    output.parent.mkdir(parents=True)\n"
        "    output.write_text('{unreadable')\n"
        "    print('Running command: terraform plan in ./unit-a')\n",
        encoding="utf-8",
    )
    script.chmod(0o755)

    result = wayfinder.execute_plan(tmp_path, str(script))

    assert result.complete is False
    assert (result.added, result.changed, result.destroyed) == (0, 0, 0)
    assert result.modules[0].status == "failed"


def test_interrupt_terminates_process_and_removes_its_temporary_directory(tmp_path):
    script = tmp_path / "terragrunt-interruptible"
    temp_root_file = tmp_path / "temp-root.txt"
    script.write_text(
        "#!/usr/bin/env python3\n"
        "import json, pathlib, sys, time\n"
        "args = sys.argv[1:]\n"
        "if args[0] == 'list':\n"
        "    print(json.dumps(['./unit-a']))\n"
        "elif args[0] == 'run':\n"
        "    pathlib.Path(sys.argv[0]).with_name('temp-root.txt').write_text(str(pathlib.Path(args[args.index('--out-dir') + 1]).parent))\n"
        "    print('Running command: terraform plan in ./unit-a', flush=True)\n"
        "    time.sleep(30)\n",
        encoding="utf-8",
    )
    script.chmod(0o755)

    def interrupt(_path):
        raise KeyboardInterrupt

    interrupted = False
    try:
        wayfinder.execute_plan(tmp_path, str(script), on_progress=interrupt)
    except KeyboardInterrupt:
        interrupted = True

    assert interrupted
    assert temp_root_file.exists()
    assert not Path(temp_root_file.read_text(encoding="utf-8")).exists()


def test_stale_temp_cleanup_requires_confirmation_and_only_removes_marked_directory(tmp_path, monkeypatch):
    monkeypatch.setattr(wayfinder.tempfile, "gettempdir", lambda: str(tmp_path))
    stale = tmp_path / "wayfinder-interrupted"
    stale.mkdir()
    (stale / ".wayfinder-owned").write_text("WAYFINDER_TEMP_V1\n", encoding="ascii")

    assert wayfinder.find_stale_temp_dirs() == [stale]
    try:
        wayfinder.cleanup_stale_temp_dir(stale)
    except PermissionError:
        pass
    else:
        raise AssertionError("cleanup must require an explicit confirmation")
    assert stale.exists()

    wayfinder.cleanup_stale_temp_dir(stale, confirmed=True)
    assert not stale.exists()
