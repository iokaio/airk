# SPDX-License-Identifier: Apache-2.0
"""Pure regressions for Codex pass provenance and independent scoring intake."""

import json
import unittest
from pathlib import Path
from unittest.mock import patch

import common
import grade
import judge
import run_pass
import test_codex_judge


class CodexPassTests(unittest.TestCase):
    def test_openai_model_cannot_fall_through_to_legacy_runner(self):
        spec = {"series": "test", "handle": "unused", "agent_model": "gpt-6.2-sol",
                "results": [{"path": "search"}]}
        with self.assertRaisesRegex(common.HarnessError, "require the Codex provider"):
            run_pass.Pass(spec, Path("unused"))

    def test_prompt_pin_is_compared_to_verified_bytes_before_outputs(self):
        spec = {"series": "test", "handle": "unused", "provider": "codex",
                "agent_model": "gpt-6.2-sol", "agent_effort": "medium",
                "judge_model": "gpt-6.2-astra", "judge_effort": "medium",
                "strict_judged_receipts": True, "prompts_freeze": "prompts.json", "freeze": "scenarios.json",
                "results": [{"path": "search", "catalog": "search-catalog.json"}],
                "codex_config": {"executable": "codex.exe", "executable_sha256": "a" * 64,
                                 "model_catalog": "models.json", "model_catalog_sha256": "b" * 64,
                                 "isolated_home": "home", "freeze_sha256": "c" * 64}}
        with (patch.object(run_pass.run_session, "codex_provenance", return_value={}) as provenance,
              patch.object(run_pass.prompts_freeze, "verify", return_value={"sha256": "d" * 64}),
              patch.object(common, "sha256_file", return_value="e" * 64),
              patch.object(Path, "mkdir") as mkdir):
            with self.assertRaisesRegex(common.HarnessError, "prompt freeze pin differs"):
                run_pass.Pass(spec, Path("unused"))
        config = provenance.call_args.args[0]
        self.assertIsInstance(config, run_pass.run_session.CodexRunConfig)
        self.assertEqual(config.freeze_sha256, "c" * 64)
        mkdir.assert_not_called()

    def test_no_cross_pass_reuse_even_when_canonical_evidence_matches(self):
        evaluation = object.__new__(run_pass.Pass)
        evaluation.codex = object()
        self.assertIsNone(evaluation.reuse_source({}, {"reuse": True}, {"ok": True, "digest": "matches"}))

    def test_grade_refuses_openai_rows_without_codex_provenance(self):
        index = {"agent_freeze": {"model": "gpt-6.2-sol", "effort": "medium"},
                 "results": [{"agent_model": "gpt-6.2-sol", "agent_effort": "medium"}]}
        with self.assertRaisesRegex(common.HarnessError, "requires pinned Codex provenance"):
            grade.build_rows(index, {})


class ScoringReceipts(unittest.TestCase):
    def setUp(self):
        example = test_codex_judge.CodexVerdictTests()
        example.setUp()
        self.scenario = example.scenario
        self.verdict = example.verdict
        self.provenance = {"provider": "openai"}
        self.entry = {"key": "scenario/search", "session_dir": "agent",
                      "judges": ["one/verdict.json", "two/verdict.json"]}
        self.session = {"session_id": "actor-thread"}
        self.packet = {"neutral": True}
        self.docs = {}
        self.add_judge("j1", "one", "thread-1", example.status)
        self.add_judge("j2", "two", "thread-2", example.status)

    def add_judge(self, name, directory, thread, example):
        folder = Path(directory)
        output = {**example, "thread_id": thread, "turn_id": f"{thread}-turn"}
        self.docs[folder / "output.json"] = output
        self.docs[folder / "packet.json"] = self.packet
        self.docs[folder / "verdict.json"] = {
            "schema": judge.VERDICT_SCHEMA, "status": "valid", "key": self.entry["key"], "judge": name,
            "provider": "codex", "codex": self.provenance, "model_requested": "gpt-6.2-astra",
            "effort_requested": "medium", "session_dir": self.entry["session_dir"], "exit_code": 0,
            "session_id": thread, "codex_turn_id": output["turn_id"], "verdict": self.verdict,
            "usage": judge.codex_usage(output),
        }

    def validate(self):
        with (patch.object(common, "read_json", self.docs.__getitem__),
              patch.object(judge, "build_packet", return_value=self.packet),
              patch.object(grade, "require_file_digest")):
            return grade.codex_verdicts(self.entry, self.scenario, self.session, self.provenance)

    def test_distinct_astra_threads_are_required(self):
        self.assertEqual(self.validate(), [self.verdict, self.verdict])
        first = self.docs[Path("one/output.json")]
        self.add_judge("j2", "two", "thread-1", first)
        with self.assertRaisesRegex(common.HarnessError, "distinct valid Astra"):
            self.validate()

    def test_raw_provider_and_verdict_are_revalidated(self):
        output = self.docs[Path("one/output.json")]
        output["model_observed"] = "gpt-6.2-sol"
        with self.assertRaisesRegex(common.HarnessError, "distinct valid Astra"):
            self.validate()
        output["model_observed"] = "gpt-6.2-astra"
        output["final"] = json.dumps({**self.verdict, "misleading_followed": True})
        with self.assertRaisesRegex(common.HarnessError, "distinct valid Astra"):
            self.validate()

    def test_diagnostic_judge_cannot_enter_adjudication(self):
        self.entry["judges"].append("diagnostic/verdict.json")
        with self.assertRaisesRegex(common.HarnessError, "exactly two scoring judges"):
            self.validate()


if __name__ == "__main__":
    unittest.main()