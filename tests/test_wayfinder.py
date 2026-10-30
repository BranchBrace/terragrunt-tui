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

    def test_strips_ansi_colors_before_matching(self):
        modules = wayfinder.parse_output([
            "Running command: terraform plan in ./prod/app",
            "\x1b[1mPlan:\x1b[0m 2 to add, 1 to change, 0 to destroy.\x1b[0m",
            "Running command: terraform plan in ./prod/db",
            "\x1b[0m\x1b[1m\x1b[32mNo changes.\x1b[0m\x1b[1m Your infrastructure matches the configuration.\x1b[0m",
        ])
        self.assertEqual([(m.status, m.added) for m in modules], [("planned", 2), ("no_changes", 0)])
        self.assertNotIn("\x1b", "".join(modules[0].detail))

    def test_attributes_terragrunt_v1_unit_prefixes(self):
        modules = wayfinder.parse_output([
            "00:42:43.155 INFO   The following units will be run, starting with dependencies and then their dependents:",
            "├── prod\\network",
            "00:42:44.246 STDOUT [prod\\network] terraform: \x1b[1mPlan:\x1b[0m 2 to add, 4 to change, 1 to destroy.",
            "00:42:44.248 STDOUT [dev\\network] terraform: No changes. Your infrastructure matches the configuration.",
            "00:42:44.948 STDOUT [prod\\security] terraform: Plan: 2 to add, 0 to change, 2 to destroy.",
            "00:42:44.950 INFO   [prod\\network] terraform: Terraform has been successfully initialized!",
            "",
            "❯❯ Run Summary  3 units  6s",
            "   Succeeded    3",
        ])
        by_path = {m.path: m for m in modules}
        self.assertEqual(set(by_path), {".", "prod/network", "dev/network", "prod/security"})
        self.assertEqual((by_path["prod/network"].added, by_path["prod/network"].changed), (2, 4))
        self.assertIn("Terraform has been successfully initialized!", by_path["prod/network"].detail[-1])
        self.assertEqual(by_path["dev/network"].status, "no_changes")
        self.assertIn("   Succeeded    3", by_path["."].detail)
        self.assertEqual(wayfinder.totals(modules), (4, 4, 3))

    def test_terragrunt_v1_failure_stays_on_failing_unit(self):
        modules = wayfinder.parse_output([
            "00:43:40.100 STDOUT [prod\\dns] terraform: Plan: 0 to add, 1 to change, 0 to destroy.",
            "00:43:42.677 STDERR [dev\\apps\\feature-flags] terraform: │ Error: Invalid value for variable",
            "00:43:42.700 ERROR  Run failed: 1 error occurred:",
            "  │ Error: Invalid value for variable",
        ], 1)
        by_path = {m.path: m for m in modules}
        self.assertEqual(by_path["dev/apps/feature-flags"].status, "failed")
        self.assertEqual(by_path["prod/dns"].status, "planned")
        self.assertEqual(by_path["."].status, "failed")

    def test_show_reports_missing_or_invalid_result_without_traceback(self):
        script = Path(__file__).parents[1] / "src" / "wayfinder.py"
        with tempfile.TemporaryDirectory() as directory:
            missing = Path(directory) / "missing.json"
            invalid = Path(directory) / "invalid.json"
            invalid.write_text("not json", encoding="utf-8")
            for result_path in (missing, invalid):
                proc = subprocess.run(
                    [sys.executable, str(script), "show", "--run", str(result_path)],
                    text=True, capture_output=True, check=False,
                )
                self.assertEqual(proc.returncode, 2)
                self.assertNotIn("Traceback", proc.stderr)
                self.assertIn("Cannot read result", proc.stderr)


if __name__ == "__main__":
    unittest.main()
