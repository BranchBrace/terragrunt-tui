import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))
import wayfinder


class ParseOutputTests(unittest.TestCase):
    def test_reuses_module_when_terragrunt_emits_init_then_plan_headers(self):
        modules = wayfinder.parse_output([
            "Running command: terraform init in ./prod/app",
            "init complete",
            "Running command: terraform plan in ./prod/app",
            "Plan: 2 to add, 1 to change, 0 to destroy.",
        ])
        self.assertEqual(len(modules), 1)
        self.assertEqual(modules[0].path, "./prod/app")
        self.assertEqual((modules[0].added, modules[0].changed, modules[0].destroyed), (2, 1, 0))
        self.assertIn("init complete", modules[0].detail)

    def test_parses_windows_drive_letter_module_headers(self):
        modules = wayfinder.parse_output([
            r"Running command: terraform plan in C:\work\prod\app",
            "Plan: 1 to add, 0 to change, 0 to destroy.",
        ])
        self.assertEqual(len(modules), 1)
        self.assertEqual(modules[0].path, r"C:\work\prod\app")
        self.assertEqual(modules[0].status, "planned")

    def test_parses_module_summaries_and_counts(self):
        modules = wayfinder.parse_output([
            "Running command: terraform plan in ./prod/app",
            "Plan: 2 to add, 1 to change, 0 to destroy.",
            "Running command: terraform plan in ./prod/db",
            "No changes. Your infrastructure matches the configuration.",
        ])
        self.assertEqual([(m.path, m.status, m.added, m.changed, m.destroyed) for m in modules], [
            ("./prod/app", "planned", 2, 1, 0), ("./prod/db", "no_changes", 0, 0, 0),
        ])
        self.assertEqual(wayfinder.totals(modules), (2, 1, 0))

    def test_marks_module_errors_failed_and_preserves_detail(self):
        modules = wayfinder.parse_output(["Module ./prod/api", "Error: provider failed", "stack trace"], 1)
        self.assertEqual(modules[0].status, "failed")
        self.assertIn("stack trace", modules[0].detail)

    def test_run_failure_without_error_text_marks_last_module_failed(self):
        modules = wayfinder.parse_output(["Module ./prod/api", "unexpected output"], 1)
        self.assertEqual(modules[0].status, "failed")

    def test_unknown_output_does_not_invent_counts(self):
        modules = wayfinder.parse_output(["unrecognized output"])
        self.assertIsNone(modules[0].added)
        self.assertEqual(wayfinder.totals(modules), (0, 0, 0))

    def test_show_reports_missing_or_invalid_result_without_traceback(self):
        script = Path(__file__).parents[1] / "src" / "wayfinder.py"
        with tempfile.TemporaryDirectory() as directory:
            missing = Path(directory) / "missing.json"
            invalid = Path(directory) / "invalid.json"
            invalid.write_text("not json", encoding="utf-8")
            unsupported = Path(directory) / "unsupported.json"
            unsupported.write_text('{"schema_version":99}', encoding="utf-8")
            for result_path in (missing, invalid, unsupported):
                proc = subprocess.run(
                    [sys.executable, str(script), "show", "--run", str(result_path)],
                    text=True, capture_output=True, check=False,
                )
                self.assertEqual(proc.returncode, 2)
                self.assertNotIn("Traceback", proc.stderr)
                self.assertIn("Cannot read result", proc.stderr)


if __name__ == "__main__":
    unittest.main()
