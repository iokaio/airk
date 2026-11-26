# SPDX-License-Identifier: Apache-2.0
"""In-memory controls for the complete-evidence ceiling and baseline gate."""

import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import common
import complete_evidence_pass as ceiling
import prompts_freeze
import run_pass


def roster(counts):
    return [
        {"scenario_id": f"{repo}-{slot}", "repo": repo, "status": "eligible",
         "correct_finish": ordinal < count}
        for repo, count in zip(common.REPOS, counts, strict=True)
        for ordinal, slot in enumerate(common.SLOTS)
    ]


class CeilingDisposition(unittest.TestCase):
    def test_requires_total_and_both_repository_thresholds(self):
        for counts, expected in (((6, 5), "below-threshold"), ((8, 4), "below-threshold"),
                                 ((7, 5), "passed"), ((6, 6), "passed")):
            with self.subTest(counts=counts):
                self.assertEqual(ceiling.summarize(roster(counts))["status"], expected)

    def test_missing_invalid_unresolved_and_reused_threads_block_passing(self):
        for status in ("missing", "invalid", "unresolved"):
            cases = roster((10, 10))
            cases[0].update(status=status, correct_finish=False)
            result = ceiling.summarize(cases)
            self.assertEqual(result["status"], "incomplete")
            self.assertEqual(result["cases"][0]["status"], status)
        cases = roster((10, 10))
        cases[0]["threads"] = ["shared-thread"]
        cases[1]["threads"] = ["shared-thread"]
        self.assertEqual(ceiling.summarize(cases)["status"], "incomplete")
        with self.assertRaisesRegex(common.HarnessError, "invalid ceiling case roster"):
            ceiling.summarize(cases[:-1])

    def test_authenticated_source_passages_are_scored_separately(self):
        case = {"id": "synthetic"}
        run = object.__new__(run_pass.Pass)
        supplied = SimpleNamespace(load=lambda scenario: "typed")
        run.ceiling_inputs = {case["id"]: supplied}
        raw = b"signed-dispatch"
        receipt = {"result_sha256": common.sha256_bytes(raw), "text_bytes": 12}
        session = {"complete_evidence_receipt": receipt}
        sources = [{"citation": {"path": "a.rs", "start_line": 1, "end_line": 5}, "text": "provided"}]
        native = [{"citation": {"path": "b.rs", "start_line": 1, "end_line": 5}, "text": "ordinary reply"}]
        with (patch.object(run_pass.run_session, "verify_complete_evidence", return_value=receipt),
              patch.object(Path, "read_bytes", return_value=raw),
              patch.object(common, "raw_body", return_value={"source_passages": sources, "hits": native}),
              patch.object(run_pass.coverage, "first_reply", return_value={"essential_missing": ["N2"]}) as check):
            with self.assertRaisesRegex(common.HarnessError, "source passages miss an essential"):
                run.require_ceiling_treatment(case, Path("actor"), session)
            check.assert_called_once_with(case, sources, 12)

    def test_baseline_has_no_manual_pass_boolean(self):
        with self.assertRaisesRegex(common.HarnessError, "needs a ceiling disposition pin"):
            ceiling.require_passing_disposition({"passed": True}, {})

    def test_baseline_recomputes_disposition_and_checks_matched_freeze(self):
        pin = {"path": "disposition.json", "sha256": "a" * 64}
        saved = {"schema": ceiling.SCHEMA, "status": "passed", "r_root": "synthetic", "spec": {
            "path": "spec.json", "sha256": "b" * 64}}
        spec = {"purpose": prompts_freeze.CEILING_PURPOSE, "prompts_freeze": "manifest.json"}
        manifest = {"scenario_freeze": {"sha256": "c" * 64}, "runner": "pinned"}
        values = {"disposition.json": saved, "spec.json": spec, "manifest.json": manifest}
        pass_instance = SimpleNamespace(ceiling_publications={"bookbuilder": "token-b", "munarium": "token-m"})
        with (patch.object(prompts_freeze, "pinned_input", side_effect=lambda path, sha256: {"path": path}),
              patch.object(common, "read_json", side_effect=lambda path: values[Path(path).name]),
              patch.object(run_pass, "Pass", return_value=pass_instance) as opened,
              patch.object(ceiling, "disposition", return_value=saved) as reconcile):
            self.assertEqual(ceiling.require_passing_disposition(pin, manifest), pass_instance.ceiling_publications)
            self.assertTrue(opened.call_args.kwargs["read_only"])
            with self.assertRaisesRegex(common.HarnessError, "differs from the authenticated ceiling"):
                ceiling.require_passing_disposition(pin, {**manifest, "runner": "other"})
            reconcile.return_value = {**saved, "status": "below-threshold"}
            with self.assertRaisesRegex(common.HarnessError, "differs from its retained receipts"):
                ceiling.require_passing_disposition(pin, manifest)


if __name__ == "__main__":
    unittest.main()