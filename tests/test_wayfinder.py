import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))
import wayfinder


class ParseOutputTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()
