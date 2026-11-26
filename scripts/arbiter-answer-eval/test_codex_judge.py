# SPDX-License-Identifier: Apache-2.0
"""Pure checks for the strict Codex judge adapter; no subprocesses or sign-in."""

import copy
import json
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import judge


class CodexVerdictTests(unittest.TestCase):
    def setUp(self):
        self.model = "gpt-6.2-astra"
        self.scenario = {"notes": [{"id": "N1", "mark": "essential"}]}
        self.verdict = {
            "notes": [{"id": "N1", "verdict": "correct", "reason": "Matches."}],
            "misleading_followed": False,
            "misleading_reason": "None.",
            "rubric_b": {key: {"score": 4, "reason": "Clear."}
                         for key in ("find", "choice", "trace", "useful")},
        }
        self.status = {
            "turn_status": "completed", "thread_id": "thread-a", "turn_id": "turn-b",
            "model_observed": self.model, "effort_observed": "medium",
            "provider_observed": "openai", "auth_route": "chatgpt",
            "configured_mcp": {}, "attempts": [], "instructions_count": 0,
            "usage": {"total": {"inputTokens": 20, "cachedInputTokens": 3,
                                "outputTokens": 10, "reasoningOutputTokens": 4,
                                "totalTokens": 30}},
            "final": json.dumps(self.verdict),
        }

    def validate(self, code=0):
        return judge.validate_codex_judge(code, self.status, self.model, self.scenario)

    def test_translation_closes_objects_and_preserves_historical_schema(self):
        historical = copy.deepcopy(judge.verdict_schema())
        strict = judge.codex_verdict_schema()
        self.assertIs(strict["additionalProperties"], False)
        note = strict["properties"]["notes"]["items"]
        self.assertIs(note["additionalProperties"], False)
        self.assertEqual(note["properties"]["verdict"]["type"], "string")
        self.assertEqual(judge.verdict_schema(), historical)

    def test_accepts_completed_exact_identity_and_usage(self):
        verdict, usage, problems = self.validate()
        self.assertEqual(verdict, self.verdict)
        self.assertEqual(usage["input_tokens"], 20)
        self.assertEqual(usage["reasoning_output_tokens"], 4)
        self.assertEqual(problems, [])

    def test_rejects_nonterminal_timeout_tool_or_model_substitution(self):
        self.assertEqual(self.validate(None)[2], ["judge timed out"])
        self.assertEqual(self.validate(1)[2], ["Codex judge did not complete"])
        for field, value in (
            ("model_observed", "gpt-6.2-sol"),
            ("auth_route", "api_key"),
            ("attempts", [{"tool": "shell"}]),
        ):
            with self.subTest(field=field):
                original = self.status[field]
                self.status[field] = value
                self.assertEqual(self.validate()[2], ["Codex judge identity or tool isolation differs"])
                self.status[field] = original

    def test_rejects_missing_usage_and_nonexact_or_wrong_verdict_shapes(self):
        usage = self.status.pop("usage")
        self.assertEqual(self.validate()[2], ["Codex judge token usage is unavailable"])
        self.status["usage"] = usage
        self.status["final"] = '{"notes":[],"notes":[]}'
        self.assertEqual(self.validate()[2], ["no exact JSON verdict"])
        self.verdict["rubric_b"]["find"]["score"] = True
        self.status["final"] = json.dumps(self.verdict)
        self.assertEqual(self.validate()[2], ["Codex verdict does not match the strict schema"])
        self.verdict["rubric_b"]["find"]["score"] = 4
        self.verdict["notes"].append({"id": "unrequested", "verdict": "missing", "reason": "Absent."})
        self.status["final"] = json.dumps(self.verdict)
        self.assertEqual(self.validate()[2], ["verdict judges notes outside the essential roster"])


if __name__ == "__main__":
    unittest.main()
