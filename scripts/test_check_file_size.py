"""Tests for scripts/check-file-size.py: python3 -m unittest discover -s scripts"""

from __future__ import annotations

import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "check_file_size", Path(__file__).with_name("check-file-size.py")
)
assert _SPEC is not None and _SPEC.loader is not None
gate = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(gate)

CORE = gate.THRESHOLDS["core"]


class CountLines(unittest.TestCase):
    def test_counts_terminated_and_unterminated_lines(self) -> None:
        self.assertEqual(gate.count_lines(b""), 0)
        self.assertEqual(gate.count_lines(b"a\nb\n"), 2)
        self.assertEqual(gate.count_lines(b"a\nb"), 2)

    def test_binary_is_not_measured(self) -> None:
        self.assertIsNone(gate.count_lines(b"\x89PNG\0\n"))


class IsMeasured(unittest.TestCase):
    def test_package_files_are_measured(self) -> None:
        self.assertTrue(gate.is_measured("core/src/lib.rs"))
        self.assertTrue(gate.is_measured("client/src/index.css"))

    def test_generated_lockfiles_and_other_paths_are_not(self) -> None:
        self.assertFalse(gate.is_measured("client/src/generated/crisp.d.ts"))
        self.assertFalse(gate.is_measured("client/bun.lock"))
        self.assertFalse(gate.is_measured("spec/openapi.json"))
        self.assertFalse(gate.is_measured("Cargo.lock"))
        self.assertFalse(gate.is_measured("core"))


class ParseBaseline(unittest.TestCase):
    def test_parses_entries_and_skips_comments(self) -> None:
        text = "# header\n\n700 core/a.rs\n  650 core/b c.rs  \n"
        self.assertEqual(gate.parse_baseline(text), {"core/a.rs": 700, "core/b c.rs": 650})

    def test_rejects_malformed_and_duplicate_lines(self) -> None:
        for text in ("core/a.rs\n", "x core/a.rs\n", "700 core/a.rs\n701 core/a.rs\n"):
            with self.subTest(text=text), self.assertRaises(gate.BaselineError):
                gate.parse_baseline(text)


class Evaluate(unittest.TestCase):
    def test_within_threshold_passes(self) -> None:
        self.assertEqual(gate.evaluate({"core/a.rs": CORE}, {}), [])

    def test_new_violator_fails(self) -> None:
        [problem] = gate.evaluate({"core/a.rs": CORE + 1}, {})
        self.assertIn("exceeds the core threshold", problem)

    def test_baselined_file_at_or_under_its_count_passes(self) -> None:
        baseline = {"core/a.rs": CORE + 10}
        self.assertEqual(gate.evaluate({"core/a.rs": CORE + 10}, baseline), [])
        self.assertEqual(gate.evaluate({"core/a.rs": CORE + 5}, baseline), [])

    def test_baselined_file_that_grows_fails(self) -> None:
        [problem] = gate.evaluate({"core/a.rs": CORE + 11}, {"core/a.rs": CORE + 10})
        self.assertIn("grew", problem)

    def test_baselined_file_back_within_threshold_is_stale(self) -> None:
        [problem] = gate.evaluate({"core/a.rs": CORE}, {"core/a.rs": CORE + 10})
        self.assertIn("remove the entry", problem)

    def test_baselined_file_that_is_gone_is_stale(self) -> None:
        [problem] = gate.evaluate({}, {"core/gone.rs": CORE + 10})
        self.assertIn("not a measured file", problem)

    def test_shrunk_entries_are_reported_as_notes(self) -> None:
        notes = gate.shrunk({"core/a.rs": CORE + 5}, {"core/a.rs": CORE + 10})
        self.assertEqual(len(notes), 1)


class Main(unittest.TestCase):
    """End to end against a scratch git repository."""

    def run_gate(self, files: dict[str, int], baseline: str) -> int:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            for path, lines in files.items():
                (root / path).parent.mkdir(parents=True, exist_ok=True)
                (root / path).write_text("x\n" * lines, encoding="utf-8")
            (root / "baseline.txt").write_text(baseline, encoding="utf-8")
            return gate.main(["--root", str(root), "--baseline", str(root / "baseline.txt")])

    def test_passes_with_a_matching_baseline(self) -> None:
        files = {"core/big.rs": CORE + 1, "core/small.rs": 3, "spec/huge.json": 10_000}
        self.assertEqual(self.run_gate(files, f"{CORE + 1} core/big.rs\n"), 0)

    def test_fails_on_an_untracked_new_violator(self) -> None:
        self.assertEqual(self.run_gate({"core/big.rs": CORE + 1}, ""), 1)

    def test_malformed_baseline_is_an_error(self) -> None:
        self.assertEqual(self.run_gate({}, "nonsense\n"), 2)


if __name__ == "__main__":
    unittest.main()
