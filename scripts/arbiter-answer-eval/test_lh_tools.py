# SPDX-License-Identifier: Apache-2.0
"""Synthetic tests for the LH acceptance tools: ``smoke_summary.py`` and ``lifetime_probe.py``.

Replies come from the Rust-emitted Answer /3 cases through ``test_loop``'s builders; the lifetime's commands are a
stub that writes canned output files. No service, model or index starts.

Run: python -I -B -m unittest discover -s scripts/arbiter-answer-eval
"""

from __future__ import annotations

import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import lifetime_probe  # noqa: E402
import loop_reply  # noqa: E402
import smoke_summary  # noqa: E402
import test_loop as builders  # noqa: E402


class TempDir(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()


class SmokeSummary(TempDir):
    def smoke_file(self, arm, bodies):
        rows = []
        for number, body in enumerate(bodies, 1):
            result = self.tmp / f"{arm}-{number}.json"
            if body is not None:
                result.write_bytes(builders.envelope(body))
            rows.append({"scenario_id": f"munarium-A{number}", "repo": "munarium", "elapsed_ms": 1000 * number,
                         "result_path": str(result) if body is not None else None,
                         "problems": [] if body is not None else ["call failed: refused"], "loop": None})
        path = self.tmp / f"{arm}.smoke.json"
        path.write_text(json.dumps({"arm": arm, "rows": rows}), encoding="utf-8")
        return path

    def test_a_model_arm_counts_model_decisions_and_a_deterministic_arm_does_not(self):
        base = builders.v3_reply()
        model = builders.top_score(base, "arbiter")
        neither = builders.focus_only(base)
        rows = (smoke_summary.summarize(self.smoke_file("C", [model, None]))
                + smoke_summary.summarize(self.smoke_file("N", [neither])))
        arm_c, failed, arm_n = rows
        # Independent expectations: counts come from the reply's own decision rows.
        self.assertEqual(sum(arm_c["included_decisions"].values()), len(model["decisions"]))
        model_rows = {k: n for k, n in arm_c["included_decisions"].items() if k.split("|")[1] == "model"}
        self.assertEqual(sum(model_rows.values()), sum(1 for d in model["decisions"] if d["source"]["kind"] == "model"))
        self.assertGreater(sum(model_rows.values()), 0)
        self.assertTrue(all(k.endswith("|arbiter|cuda|float16") for k in model_rows))
        self.assertEqual(sum(arm_n["included_decisions"].values()), len(neither["decisions"]))
        self.assertTrue(all(k.split("|")[1] == "deterministic" for k in arm_n["included_decisions"]))
        self.assertEqual((arm_c["arm"], arm_c["scenario"], arm_c["elapsed_ms"], arm_c["problems"]),
                         ("C", "munarium-A1", 1000, []))
        self.assertEqual(arm_c["records"], len(model["records"]))
        self.assertEqual(arm_c["focus_calls"], model["decision_log"]["focus_calls"])
        self.assertEqual(arm_c["log_counts"], loop_reply.log_counts(model))
        # A call that kept no reply reports its problems and no reply figures.
        self.assertEqual(failed, {"arm": "C", "scenario": "munarium-A2", "elapsed_ms": 2000,
                                  "problems": ["call failed: refused"], "reply": None})

    def test_main_prints_one_json_list_for_every_file(self):
        first = self.smoke_file("C", [builders.top_score(builders.v3_reply(), "arbiter")])
        second = self.smoke_file("N", [builders.focus_only(builders.v3_reply())])
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(smoke_summary.main([str(first), str(second)]), 0)
        self.assertEqual([r["arm"] for r in json.loads(out.getvalue())], ["C", "N"])
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(smoke_summary.main([]), 2)


class StubCommands:
    """Stands in for ``fixture.Commands``: ``run`` writes the canned stdout for a label, or raises."""

    def __init__(self, folder, canned, failing=()):
        self.folder = Path(folder)
        self.folder.mkdir(parents=True, exist_ok=True)
        self.canned = canned
        self.failing = set(failing)
        self.calls = []

    def run(self, label, *argv):
        self.calls.append((label, argv))
        if label in self.failing:
            (self.folder / f"0001-{label}.stderr").write_text("arbiter_unavailable", encoding="utf-8")
            raise ValueError(f"{label} exited 1")
        path = self.folder / f"0001-{label}.stdout"
        path.write_text(json.dumps(self.canned.get(label, {})), encoding="utf-8")
        return path, 2_500_000_000


HANDLE = {"indexes": {"munarium": {"index_id": "idx-m"}, "bookbuilder": {"index_id": "idx-b"}}}


class LifetimeProbe(TempDir):
    def test_help_saves_the_reply_and_counts_the_words(self):
        reply = {"text": "Focus and Arbiter are separate switches. Arbiter needs no Focus. Focus alone works."}
        commands = StubCommands(self.tmp / "c", {"help-arbiter": reply})
        out = self.tmp / "out"
        result = lifetime_probe.probe_help(commands, HANDLE, out, "munarium", "arbiter")
        self.assertEqual(commands.calls, [("help-arbiter", ("tools", "call", "airk_help", "--index", "idx-m",
                                                            "--args", '{"topic": "arbiter"}'))])
        self.assertEqual(json.loads((out / "help-arbiter.json").read_text(encoding="utf-8")), reply)
        self.assertEqual(result["word_counts"], {"Focus": 3, "Arbiter": 2, "Large": 0, "context_compact": 0,
                                                 "context_large": 0, "separate": 1})
        self.assertEqual((result["ok"], result["seconds"]), (True, 2.5))

    def test_arbiter_status_reads_both_forms_and_keeps_a_refused_one(self):
        commands = StubCommands(self.tmp / "c", {"after-status-legacy": {"focus": {"queued": 0}}},
                                failing={"after-status-device"})
        out = self.tmp / "out"
        legacy, device = lifetime_probe.probe_arbiter_status(commands, out, "after")
        self.assertEqual([c[1] for c in commands.calls], [("arbiter", "status-legacy"), ("arbiter", "status")])
        self.assertTrue(legacy["ok"])
        self.assertEqual(json.loads((out / "after-status-legacy.json").read_text(encoding="utf-8")),
                         {"focus": {"queued": 0}})
        self.assertFalse(device["ok"])
        self.assertIn("exited 1", device["error"])
        self.assertFalse((out / "after-status-device.json").exists())
        self.assertEqual((out / "0001-after-status-device.stderr").read_text(encoding="utf-8"), "arbiter_unavailable")

    def test_index_status_calls_every_index_of_the_handle(self):
        commands = StubCommands(self.tmp / "c", {}, failing={"diag-airk-status-bookbuilder"})
        out = self.tmp / "out"
        results = lifetime_probe.probe_index_status(commands, HANDLE, out)
        self.assertEqual([(r["label"], r["ok"]) for r in results],
                         [("diag-airk-status-munarium", True), ("diag-airk-status-bookbuilder", False)])
        self.assertEqual(commands.calls[0][1], ("tools", "call", "airk_status", "--index", "idx-m", "--args", "{}"))
        self.assertTrue((out / "diag-airk-status-munarium.json").is_file())


if __name__ == "__main__":
    unittest.main()
