# SPDX-License-Identifier: Apache-2.0
"""Synthetic tests for the oracle fork scorer and the collector input; hand-written expectations."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import prepare_forks  # noqa: E402
import score_forks  # noqa: E402

CHOICE = {slot: (f"{slot:x}" * 64)[:64] for slot in range(1, 6)}


def note(nid: str, path: str, lines: list[int], mark: str = "essential") -> dict:
    return {"id": nid, "mark": mark, "path": path, "lines": lines, "symbol": None,
            "fact": f"fact {nid}", "alternatives": []}


SCENARIO = {
    "schema": "airk.private.answer-eval-scenario/1", "id": "airk-A1", "repository": "airk", "set": "development",
    "slot": "A1", "kind": "architecture", "instruction": "Trace a call.", "source_commit": "0" * 40,
    "notes": [note("N1", "src/a.rs", [10, 19]), note("N2", "src/b.rs", [100, 109]),
              note("N3", "docs/c.md", [1, 3]), note("H1", "src/d.rs", [5, 9], "helpful")],
}


def hit(path: str, start: int, end: int) -> dict:
    return {"citation": {"path": path, "start_line": start, "end_line": end}, "text": "evidence", "truncated": False}


def packet(*hits: dict) -> dict:
    return {"records": [{"operation": "search", "arguments": {"query": "q"},
                         "canonical_result": {"hits": list(hits)}}]}


def branch(name: str, hits: list[dict], *, slot: int | None = None, valid: bool = True,
           requests: list[dict] | None = None) -> dict:
    row = {"branch": name, "valid": valid, "reply_is_error": False, "packet": packet(*hits),
           "native": {"stop": "no_progress"}, "elapsed_ms": 10, "requests": requests or []}
    if slot is not None:
        row.update(forced={"ordinal": 1, "choice": CHOICE[slot]}, read_slot=slot, same_roster_as_queue=True)
    return row


REQUEST = {"ordinal": 1, "cards": [{"action": "read", "read_slot": s, "choice": CHOICE[s]} for s in (1, 2, 3, 4)]
           + [{"action": "sufficient"}]}


def rows_file(directory: Path) -> Path:
    call1 = {"event": "call", "id": "airk-A1", "scenario_id": "airk-A1", "repository": "airk",
             "queue": branch("queue", [hit("src/a.rs", 10, 19)], requests=[REQUEST]),
             "forks": [branch("o1-r1", [hit("src/a.rs", 10, 19)], slot=1),
                       # N2 by 6 overlapping lines, N3 whole (3 lines), N1 again.
                       branch("o1-r2", [hit("src/a.rs", 12, 30), hit("src/b.rs", 95, 105),
                                        hit("docs/c.md", 1, 3)], slot=2),
                       # Three lines of N1's ten: rule 2 needs five, so nothing is covered.
                       branch("o1-r3", [hit("src/a.rs", 17, 19)], slot=3),
                       # An invalid branch never counts, even with full coverage.
                       branch("o1-r4", [hit("src/a.rs", 10, 19), hit("src/b.rs", 100, 109),
                                        hit("docs/c.md", 1, 3)], slot=4, valid=False)]}
    call2 = {"event": "call", "id": "airk-A1#second", "scenario_id": "airk-A1", "repository": "airk",
             "queue": branch("queue", [hit("docs/c.md", 2, 2)], requests=[]), "forks": []}
    path = directory / "rows.jsonl"
    lines = [{"event": "start", "schema": score_forks.ROWS_SCHEMA, "input_digest": "d" * 64,
              "layers": ["followups"], "fork_ordinals": [1]}, call1, call2, {"event": "complete", "calls": 2}]
    path.write_text("".join(json.dumps(line) + "\n" for line in lines), encoding="utf-8", newline="\n")
    return path


def scenarios_dir(directory: Path) -> Path:
    (directory / "scenarios" / "airk").mkdir(parents=True)
    (directory / "scenarios" / "airk" / "A1.json").write_text(json.dumps(SCENARIO), encoding="utf-8")
    return directory / "scenarios"


class ScoreForks(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        root = Path(self.temp.name)
        self.rows, self.scenarios = rows_file(root), scenarios_dir(root)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def test_decision_points_use_rule_two_and_skip_invalid_forks(self) -> None:
        report = score_forks.score(self.rows, self.scenarios)
        first, second = report["ordinals"]["1"]["points"]
        self.assertEqual(first["queue_coverage"], 1)
        self.assertEqual(first["queue_covered"], ["N1"])
        self.assertEqual([f["count"] for f in first["forks"]], [1, 3, 0, 0])
        self.assertEqual(first["forks_valid"], 3)
        self.assertEqual(first["forks_invalid"], 1)
        self.assertEqual(first["best_forced_coverage"], 3)
        self.assertEqual(first["best_forced_gain"], 2)
        self.assertEqual(first["oracle_gain"], 2)
        self.assertEqual(first["best_slots"], [2])
        self.assertEqual(first["gained_notes"], ["N2", "N3"])
        self.assertEqual(first["worse_forks"], 1)
        self.assertEqual(first["offered_reads"], 4)
        # The second call made no Context request: a gain of zero, still counted.
        self.assertFalse(second["has_request"])
        self.assertEqual(second["queue_coverage"], 0)  # one line of a three-line note needs three
        self.assertEqual(second["oracle_gain"], 0)
        summary = report["ordinals"]["1"]["summary"]
        self.assertEqual(summary["calls_counted"], 2)
        self.assertEqual(summary["leverage"], 0.5)
        self.assertEqual(summary["mean_oracle_gain"], 1.0)
        self.assertEqual(summary["mean_queue_coverage"], 0.5)
        self.assertEqual(summary["mean_best_forced_coverage"], 3)
        self.assertEqual(summary["invalid_forks"], 1)
        self.assertTrue(report["complete"])
        self.assertTrue(report["ordinals"]["1"]["verdict"]["decision"].startswith("undetermined"))

    def test_compact_capture_and_go(self) -> None:
        report = score_forks.score(self.rows, self.scenarios, {"airk-A1": {"choice": CHOICE[1]},
                                                               "airk-A1#second": {"abstained": True}})
        compact = report["ordinals"]["1"]["compact"]
        self.assertEqual(compact["matched"], 2)
        self.assertEqual(compact["compact_gain_total"], 0)
        self.assertEqual(compact["oracle_gain_total"], 2)
        self.assertEqual(compact["capture"], 0.0)
        self.assertTrue(report["ordinals"]["1"]["verdict"]["decision"].startswith("go:"))
        self.assertEqual(report["ordinals"]["1"]["non_win_candidates"],
                         [{"call": "airk-A1", "notes": ["N2", "N3"], "slots": [2]}])
        best = score_forks.score(self.rows, self.scenarios, {"airk-A1": {"choice": CHOICE[2]}})
        self.assertEqual(best["ordinals"]["1"]["compact"]["capture"], 1.0)
        self.assertEqual(best["ordinals"]["1"]["verdict"]["decision"], "no-go: no room")

    def test_verdict_bands(self) -> None:
        def decide(leverage, gain, capture=None, compact_mean=None):
            summary = {"leverage": leverage, "mean_oracle_gain": gain}
            return score_forks.verdict(summary, {"capture": capture, "compact_mean_gain": compact_mean})["decision"]
        self.assertEqual(decide(0.09, 2.0, 0.0), "no-go: fix the roster, not the weights")
        self.assertEqual(decide(0.5, 0.19, 0.0), "no-go: fix the roster, not the weights")
        self.assertEqual(decide(0.3, 0.6, 0.7), "no-go: no room")
        self.assertTrue(decide(0.2, 0.5, 0.39).startswith("go:"))
        self.assertTrue(decide(0.3, 0.6, 0.5, -0.1).startswith("go:"))  # Compact worse than queue order
        self.assertTrue(decide(0.3, 0.6, 0.5, 0.1).startswith("between"))
        self.assertTrue(decide(0.15, 0.3, 0.1).startswith("between"))
        self.assertTrue(decide(0.3, 0.6).startswith("undetermined"))

    def test_card_choice_reads_any_action_shape(self) -> None:
        self.assertEqual(score_forks.card_choice({"card": {"action": {"read": {"choice": CHOICE[3]}}}}), CHOICE[3])
        self.assertEqual(score_forks.card_choice({"card": {"action": {"Read": {"choice": CHOICE[4]}}}}), CHOICE[4])
        self.assertIsNone(score_forks.card_choice({"card": {"action": "sufficient"}}))


class PrepareForks(unittest.TestCase):
    def test_framed_digest_matches_the_native_framing(self) -> None:
        domain, field = b"dom", b"data!"
        preimage = (b"AIRK-DIGEST-1\x00" + (3).to_bytes(8, "big") + b"dom" + (5).to_bytes(8, "big") + b"data!")
        self.assertEqual(prepare_forks.framed_digest(domain, [field]), hashlib.sha256(preimage).hexdigest())

    def test_canonical_calls_send_the_change_set_budget_without_an_index(self) -> None:
        calls = prepare_forks.canonical_calls([dict(SCENARIO)], {"budget": prepare_forks.CS1_BUDGET})
        self.assertEqual(len(calls), 1)
        args = calls[0]["arguments"]
        self.assertEqual(args["question"], "Trace a call.")
        self.assertEqual(args["schema"], "airk.arbiter.context.request/2")
        self.assertEqual(args["budget"], {"calls": 12, "rounds": 3, "fanout": 12, "candidates": 32,
                                          "max_bytes": 524288, "input_tokens": 8192, "deadline_ms": 120000})
        self.assertNotIn("index", args)
        self.assertEqual(prepare_forks.clean({"index": "idx", "question": "q", "project": "p"}), {"question": "q"})

    def test_source_check_compares_in_process_files_with_the_image(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp)
            for path, text in {"crates/airk-kernel/src/arbiter/software.rs": b"image bytes\n",
                               "crates/airk-kernel/src/arbiter/joint.rs": b"edited after the image\n",
                               "crates/airk-kernel/src/arbiter/software/tests.rs": b"test only\n",
                               "crates/airk-service/src/x.rs": b"runs in the live service\n",
                               "crates/airk-mcp/src/context_action_capture_tests/software_forks.rs": b"own\n"}.items():
                (repo / path).parent.mkdir(parents=True, exist_ok=True)
                (repo / path).write_bytes(text)
            record = {"label": "cs1x", "git_head": "a" * 40, "changed_files": {
                "crates/airk-kernel/src/arbiter/software.rs": hashlib.sha256(b"image bytes\n").hexdigest(),
                "crates/airk-service/src/x.rs": "0" * 64}}
            porcelain = (" M crates/airk-kernel/src/arbiter/joint.rs\n?? crates/airk-kernel/src/arbiter/software/tests.rs\n"
                         "?? crates/airk-mcp/src/context_action_capture_tests/software_forks.rs\n")
            check = prepare_forks.source_check(record, repo, "a" * 40, porcelain)
            self.assertEqual(check["checked"], 2)
            self.assertEqual(check["mismatches"], [{"path": "crates/airk-kernel/src/arbiter/joint.rs", "image": None,
                                                    "tree": hashlib.sha256(b"edited after the image\n").hexdigest()}])
            self.assertEqual(prepare_forks.source_check(record, repo, "b" * 40, "")["mismatches"],
                             [{"path": "HEAD", "image": "a" * 40, "tree": "b" * 40}])

    def test_collection_name_requires_exactly_one(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "collections-1.4" / ".writer-locks").mkdir(parents=True)
            (root / "collections-1.4" / "answer-eval-airk").mkdir()
            self.assertEqual(prepare_forks.collection_name(root), "answer-eval-airk")
            (root / "collections-1.4" / "second").mkdir()
            with self.assertRaises(Exception):
                prepare_forks.collection_name(root)


if __name__ == "__main__":
    unittest.main()
