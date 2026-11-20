# SPDX-License-Identifier: Apache-2.0
"""Model-free refusal checks for the sealed R7 independent reference input."""
import copy
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_inputs as reader


class InputAdmission(unittest.TestCase):
    def test_historical_receipt_or_inexact_pin_is_refused(self):
        for index, pin in enumerate((
                {"path": str(reader.CAMPAIGN/"r5-chronological-native-first56-r1/projection-receipt.json"),
                 "sha256": reader.PROJECTION[1]},
                {"path": str(reader.PROJECTION[0]), "sha256": "a"*64})):
            with self.subTest(index=index), self.assertRaisesRegex(ValueError, "different sealed R7"):
                reader.canonical(pin, reader.PROJECTION)

    def test_extent_mismatch_is_refused(self):
        path, sha = reader.PROJECTION
        actual = {"path": str(path), "sha256": sha, "bytes": 17}
        with patch.object(reader, "reference", return_value=actual):
            self.assertEqual(reader.canonical(reader.core(actual), reader.PROJECTION), actual)
            with self.assertRaisesRegex(ValueError, "extent differs"):
                reader.canonical(actual | {"bytes": 16}, reader.PROJECTION)

    def test_out_of_range_and_unadmitted_terminal_refuse_before_io(self):
        for args in ((54, 3), (-1, 1), (0, 9), (56, 1), (False, 1)):
            with self.subTest(args=args), self.assertRaisesRegex(ValueError, "consecutive rows"):
                reader.selected({}, {}, {}, *args)
        full = {"path": str(reader.CAMPAIGN/"r7-chronological-full100-r1/projection-receipt.json")}
        with self.assertRaisesRegex(ValueError, "terminal-capture admission"):
            reader.selected(full, {}, {}, 0, 1)


def settled_job(pid, image, *, default_owner=False):
    assignment = {"pid": pid, "image": image, "execution_user_sid": reader.SID}
    if default_owner:
        assignment["default_owner_sid"] = reader.SID
    return {
        "exit_code": 0, "timed_out": False, "log_oversized": False,
        "owned_descendants_settled": True, "owned_job_forced_termination": False,
        "owned_job_final_active_processes": 0, "owned_job_final_member_pids": [],
        "parent_assigned_to_job": False, "child_assigned_before_resume": True,
        "owned_total_processes": 1, "unexpected_third_process": False,
        "whole_process_deadline_not_refreshed": True,
        "retained_member_exits": [{"exit_observed": True, "exit_code": 0}],
        "handle_close_proof": {"forced": False, "settled": True, "final_active": 0,
                               "members_before": [], "final_member_pids": []},
        "owned_leader_pid": pid, "suspended_assignment": assignment,
    }


class NativeCustody(unittest.TestCase):
    def setUp(self):
        self.projection = {"path": str(reader.PROJECTION[0]), "sha256": reader.PROJECTION[1], "bytes": 12}
        self.compiler_image = {"path": "compiler.exe", "sha256": reader.IMAGE_SHA}
        self.graph_image = {"path": "staged.exe", "sha256": reader.IMAGE_SHA}
        self.summary = {"compact": "all56", "large": "all56"}
        self.native = {
            "image": self.compiler_image, "passed": True, "NN_invocations": 0, "projection": self.projection,
            "launch_audit_sha256": reader.AUDIT, "summary": self.summary,
            "job": settled_job(1, self.compiler_image), "build": {"sha256": "build"}, "build_log": {"sha256": "log"},
            "command": [self.compiler_image["path"], "--exact", reader.SELECTOR, "--ignored", "--nocapture", "--test-threads=1"],
        }
        self.graph = {
            "schema": "airk.private.compact-cutlass-owned/1", "complete": True, "mode": "graph",
            "build": self.native["build"], "build_log": self.native["build_log"],
            "image": self.graph_image | {"bytes": 10}, "linked_image": self.compiler_image | {"bytes": 10},
            "owned": settled_job(2, self.graph_image, default_owner=True),
            "runtime_manifest": {"sha256": reader.IDENTITY["runtime_manifest_sha256"]},
            "result": reader.IDENTITY | {"placement": {"complete": True}, "graph": {"complete": True},
                                         "score": None, "timing": None},
            "sources": {"selector": "original"},
            "backend_sources": {"math": "original"},
            "typed_graph_validation": {"cuda_fp32_matmuls": 196, "cuda_mha_fp32": 28},
        }

    def test_compiler_and_observed_graph_bind_same_image(self):
        reader.verify_jobs(self.native, self.graph, self.projection, self.summary)
        for field, change in (("image", {"sha256": "historical"}), ("build", {"sha256": "other-build"}),
                              ("linked_image", {"path": "substituted.exe"})):
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "same-image typed graph"):
                reader.verify_jobs(self.native, self.graph | {field: self.graph[field] | change},
                                   self.projection, self.summary)

    def test_old_audit_or_partial_compilation_does_not_enter_r7(self):
        for change in ({"launch_audit_sha256": "legacy"}, {"summary": {"compact": "all56"}},
                       {"NN_invocations": 1}, {"projection": {"sha256": "legacy"}}):
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, "recompile"):
                reader.verify_jobs(self.native | change, self.graph, self.projection, self.summary)

    def test_forced_graph_job_or_changed_owner_fails(self):
        for mutation in ("forced", "default_owner"):
            job = copy.deepcopy(self.graph["owned"])
            if mutation == "forced":
                job["owned_job_forced_termination"] = True
            else:
                job["suspended_assignment"]["default_owner_sid"] = "unauthorized"
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, "same-image typed graph"):
                reader.verify_jobs(self.native, self.graph | {"owned": job}, self.projection, self.summary)


if __name__ == "__main__":
    unittest.main()
