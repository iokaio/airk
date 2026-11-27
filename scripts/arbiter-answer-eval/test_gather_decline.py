# SPDX-License-Identifier: Apache-2.0
"""The approved decline rubric (27 November 2026) on small synthetic repositories."""
from __future__ import annotations

import types
import unittest

import gather_decline

# Literal from the user's wording, not read from the code under test.
REFUSAL = "This request is specific enough that gather_context is unlikely to be a good fit. Read files directly instead"


def repository(defines: dict[str, list[str]]) -> types.SimpleNamespace:
    """A stand-in with what the rule reads: each file's defined names, and the file list."""
    files: dict[str, dict] = {}
    for name, paths in defines.items():
        for path in paths:
            files.setdefault(path, {"defines": {}})["defines"][name] = [1]
    return types.SimpleNamespace(index={"files": files}, texts={path: [] for path in files})


class DeclineRule(unittest.TestCase):
    def test_a_narrow_task_naming_its_place_is_declined_with_the_files(self) -> None:
        repo = repository({"parse_window": ["src/report.py"], "TimeseriesTest": ["tests/test_report.py"]})
        seen = gather_decline.features(repo, {"task": "Add tests for parse_window beside TimeseriesTest."})
        declined, _ = gather_decline.decide(seen)
        self.assertTrue(declined)
        self.assertEqual(gather_decline.refusal(seen, with_files=True),
                         f"{REFUSAL}: src/report.py, tests/test_report.py.")
        self.assertEqual(gather_decline.refusal(seen), f"{REFUSAL}.")

    def test_wide_words_in_the_task_keep_the_tool_but_wide_words_in_needs_do_not(self) -> None:
        repo = repository({"parse_window": ["src/report.py"]})
        wide = gather_decline.features(repo, {"task": "Change parse_window everywhere it is used."})
        self.assertFalse(gather_decline.decide(wide)[0])
        needs = gather_decline.features(repo, {"task": "Fix parse_window.", "needs": ["every caller of it"]})
        self.assertTrue(gather_decline.decide(needs)[0])

    def test_names_defined_in_many_files_keep_the_tool(self) -> None:
        repo = repository({"run_job": [f"src/m{n}.py" for n in range(5)]})
        seen = gather_decline.features(repo, {"task": "Make run_job retry."})
        self.assertFalse(gather_decline.decide(seen)[0])

    def test_a_named_language_limits_the_files_and_a_dotted_name_counts(self) -> None:
        paths = ["client/rust/a.rs", "client/java/A.java", "client/go/a.go", "client/python/a.py"]
        repo = repository({"timeseries": paths})
        unscoped = gather_decline.features(repo, {"task": "Test reports.timeseries."})
        self.assertFalse(gather_decline.decide(unscoped)[0])
        scoped = gather_decline.features(repo, {"task": "Test reports.timeseries in the Python client."})
        self.assertEqual(scoped["files"], ["client/python/a.py"])
        self.assertTrue(gather_decline.decide(scoped)[0])

    def test_a_task_naming_nothing_defined_keeps_the_tool(self) -> None:
        repo = repository({"parse_window": ["src/report.py"]})
        seen = gather_decline.features(repo, {"task": "Improve how reports are rendered."})
        self.assertFalse(gather_decline.decide(seen)[0])


if __name__ == "__main__":
    unittest.main()
