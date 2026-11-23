# SPDX-License-Identifier: Apache-2.0
"""Synthetic tests for lg_bases.py; no service, index or private input is used."""

import json
import tempfile
import unittest
from pathlib import Path

import common
import lg_bases


class Verdicts(unittest.TestCase):
    def test_each_condition_reads_the_lifetimes_own_handle(self):
        ready = {"state": "ready", "queryable": True}
        semantic = dict(ready, semantic_ready_utc="2026-11-23T01:00:00+00:00")
        closed = {"state": "closed", "restartable": True}
        self.assertEqual(lg_bases.wait_verdict({"state": "indexing"}, False, "ready"), ("waiting", "indexing"))
        self.assertEqual(lg_bases.wait_verdict(ready, False, "ready")[0], "done")
        self.assertEqual(lg_bases.wait_verdict(ready, False, "semantic")[0], "waiting")
        self.assertEqual(lg_bases.wait_verdict(semantic, False, "semantic")[0], "done")
        self.assertEqual(lg_bases.wait_verdict(semantic, False, "closed")[0], "waiting")
        self.assertEqual(lg_bases.wait_verdict(closed, True, "closed")[0], "done")
        self.assertEqual(lg_bases.wait_verdict(None, False, "ready"), ("waiting", None))
        with self.assertRaises(common.HarnessError):
            lg_bases.wait_verdict(ready, False, "settled")

    def test_an_error_a_stop_or_an_ended_start_is_a_failure_not_a_wait(self):
        failed = {"state": "indexing", "error": "error: ValueError: x", "stop_reason": "error: ValueError: x"}
        for condition in lg_bases.CONDITIONS:
            self.assertEqual(lg_bases.wait_verdict(failed, True, condition)[0], "failed")
        # A lifetime that stopped on its stop file before it was semantically ready is not what was waited for.
        stopped = {"state": "ready", "stop_reason": "stop file"}
        self.assertEqual(lg_bases.wait_verdict(stopped, False, "semantic")[0], "failed")
        self.assertEqual(lg_bases.wait_verdict(stopped, False, "closed")[0], "waiting")
        self.assertEqual(lg_bases.wait_verdict({"state": "indexing"}, True, "ready"),
                         ("failed", "the start process ended in state 'indexing'"))
        # Closed but not restartable is never a base.
        self.assertEqual(lg_bases.wait_verdict({"state": "closed", "restartable": False}, True, "closed")[0], "failed")


class Summary(unittest.TestCase):
    def test_a_row_names_the_restart_origin_and_each_indexs_facts(self):
        with tempfile.TemporaryDirectory() as folder:
            paths = lg_bases.base_paths(Path(folder), "06-gate-lg", "C")
            self.assertEqual(paths["root"].name, "fixture-06-gate-lg-C")
            self.assertEqual(paths["handle"].parent.name, "06-gate-lg-C-base")
            # A base continued by a second lifetime is read under that lifetime's name, in the same root.
            again = lg_bases.base_paths(Path(folder), "06-gate-lg", "L", "06-gate-lg-L-base2")
            self.assertEqual((again["root"].name, again["lifetime"], again["handle"].parent.name),
                             ("fixture-06-gate-lg-L", "06-gate-lg-L-base2", "06-gate-lg-L-base2"))
            self.assertEqual(lg_bases.named_bases(["L=06-gate-lg-L-base2"]), {"L": "06-gate-lg-L-base2"})
            for bad in (["L"], ["L=a", "L=b"], ["=x"], ["L="]):
                with self.assertRaises(lg_bases.common.HarnessError):
                    lg_bases.named_bases(bad)
            handle = {"state": "closed", "restartable": True, "service_exit_code": 0,
                      "helpers": {"fixture.py": "ab" * 32},
                      "image": {"label": "img", "service": {"sha256": "cd" * 32}, "git_head": "h"},
                      "indexes": {"bookbuilder-v2": {"index_id": "idx_b", "publication_token": "t-handle",
                                                     "files": 8116, "pipeline_digest": "p-handle",
                                                     "denies": [["exact", "a"]], "served_mode": "hybrid"},
                                  "munarium": {"index_id": "idx_m", "denies": []}}}
            states = {"bookbuilder-v2": {"indexed": {"seconds": 1620.5, "job_seconds": 1618.2,
                                                     "queued_by": "the MiniLM install"},
                                         "trims": [{"kind": "exact", "path": "a", "outcome": "applied",
                                                    "before_index": True, "idle_wait_seconds": 0.05,
                                                    "busy_refusals": [], "removed_artifacts": 0}]},
                      "munarium": {"indexed": {"seconds": 300.0, "queued_by": "the launcher"}, "trims": []}}
            probe = {"at": "now", "passed": True, "settled": {"ok": True},
                     "status": {"munarium": {"files": 1023, "chunks": 9000, "indexed_vectors": 9100,
                                             "publication": {"token": "t-status"},
                                             "semantic_readiness": {"state": "ready", "completed_artifacts": 1023,
                                                                    "selected_artifacts": 1023,
                                                                    "pipeline_digest": "p-status"}}}}
            row = lg_bases.summary_row(paths, handle, states, probe)
            self.assertTrue(row["restart_origin"])
            self.assertFalse(row["holds_airk_index"])
            self.assertEqual(row["fixture_py_sha256"], "ab" * 32)
            book, muna = row["indexes"]["bookbuilder-v2"], row["indexes"]["munarium"]
            self.assertEqual((book["indexing_seconds"], book["index_job_seconds"], book["index_pass_queued_by"]),
                             (1620.5, 1618.2, "the MiniLM install"))
            self.assertEqual(book["trims"][0]["busy_refusals"], 0)
            # Without a probe status the handle's own record stands; with one, the probe's figures do.
            self.assertEqual((book["files"], book["publication_token"], book["pipeline_digest"], book["chunks"]),
                             (8116, "t-handle", "p-handle", None))
            self.assertEqual((muna["files"], muna["chunks"], muna["indexed_vectors"], muna["publication_token"],
                              muna["pipeline_digest"], muna["semantic_state"]),
                             (1023, 9000, 9100, "t-status", "p-status", "ready"))
            self.assertEqual(row["probe"]["passed"], True)
            json.dumps(row)
            for change in ({"state": "ready"}, {"restartable": False}, {"error": "error: x"}):
                self.assertFalse(lg_bases.summary_row(paths, dict(handle, **change), states, None)["restart_origin"])
            airk = dict(handle, indexes=dict(handle["indexes"], airk={"index_id": "idx_a"}))
            self.assertTrue(lg_bases.summary_row(paths, airk, states, None)["holds_airk_index"])
            self.assertEqual(lg_bases.summary_row(paths, None, {}, None)["indexes"], {})


if __name__ == "__main__":
    unittest.main()
