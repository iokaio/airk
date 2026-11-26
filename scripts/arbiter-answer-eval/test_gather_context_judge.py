# SPDX-License-Identifier: Apache-2.0
"""Judge cohort scheduling and receipt custody without subprocess or filesystem writes."""

from contextlib import redirect_stderr
import io
import json
from pathlib import Path
import threading
import unittest
from unittest import mock

import common
import gather_context_judge as diagnostic


def trials(count):
    return [
        diagnostic.Trial(Path(f"masked-{index}"), "j1", {"id": index}, {"result": f"Result-{index}"}, {})
        for index in range(count)
    ]


def observed(index, *, error=None):
    return diagnostic.Observation(
        2 if error else 0, f"stream {index}".encode(), b"",
        {} if error else {"process_exit": 0, "id": index}, 20, error,
    )


class JudgeCohortTests(unittest.TestCase):
    def test_four_workers_join_before_any_shared_home_audit_or_result_use(self):
        roster = trials(4)
        phases = []
        joined = set()
        running = set()
        peak = 0
        cohort = threading.Barrier(len(roster), timeout=5)
        lock = threading.Lock()

        def audit(action, path):
            self.assertFalse(running)
            if action == "After":
                self.assertEqual(joined, {trial.directory for trial in roster})
            phases.append((action, path))

        def run(trial):
            nonlocal peak
            with lock:
                self.assertEqual(len(phases), 2 * len(roster))
                running.add(trial.directory)
                peak = max(peak, len(running))
            try:
                cohort.wait()
                return observed(trial.scenario["id"])
            finally:
                with lock:
                    running.remove(trial.directory)
                    joined.add(trial.directory)

        results = diagnostic.collect_cohort(roster, audit=audit, observe=run)
        expected = [(action, trial.directory) for trial in roster for action in ("Prepare", "Before")]
        expected += [("After", trial.directory) for trial in roster]
        self.assertEqual(phases, expected)
        self.assertEqual(peak, 4)
        self.assertEqual([result.status["id"] for result in results], [0, 1, 2, 3])

    def test_failed_preparation_starts_no_children(self):
        roster = trials(2)
        run = mock.Mock()

        def audit(action, path):
            if action == "Before" and path == roster[-1].directory:
                raise common.HarnessError("unverified parent")

        with self.assertRaisesRegex(common.HarnessError, "unverified parent"):
            diagnostic.collect_cohort(roster, audit=audit, observe=run)
        run.assert_not_called()

    def test_failed_after_checks_siblings_before_refusing_to_return_results(self):
        roster = trials(3)
        after = []

        def audit(action, path):
            if action == "After":
                after.append(path)
                if path == roster[0].directory:
                    raise common.HarnessError("foreign output")

        with self.assertRaisesRegex(common.HarnessError, "Judge output custody failed for masked-0"):
            diagnostic.collect_cohort(roster, audit=audit, observe=lambda _: observed(0))
        self.assertEqual(after, [trial.directory for trial in roster])

    def test_serial_default_runs_in_callers_thread(self):
        ident = threading.get_ident()
        result = diagnostic.collect_cohort(
            trials(1), audit=mock.Mock(), observe=lambda _: threading.get_ident(),
        )
        self.assertEqual(result, [ident])

    def test_roster_over_limit_refuses_before_any_side_effect(self):
        audit = mock.Mock()
        with self.assertRaisesRegex(common.HarnessError, "invalid judge cohort width"):
            diagnostic.collect_cohort(trials(5), audit=audit, observe=mock.Mock())
        audit.assert_not_called()

    def test_live_driver_parameters_keep_blind_prompt_and_fixed_budget(self):
        trial = trials(1)[0]
        config = object()
        schema = {"type": "object"}
        with (
            mock.patch.object(diagnostic.run_session, "codex_command", return_value=["codex"]) as command,
            mock.patch.object(diagnostic.run_session, "codex_child_env", return_value={"key": "value"}),
            mock.patch.object(diagnostic.codex_isolation_probe, "run_app_server",
                              return_value=(0, b"stream", b"", {"process_exit": 0})) as run,
        ):
            result = diagnostic.observe_trial(trial, config, schema)
        command.assert_called_once_with(config, trial.directory)
        run.assert_called_once_with(
            ["codex"], {"key": "value"}, trial.directory / "work", model="gpt-6.2-astra", effort="medium",
            instruction=diagnostic.judge.packet_prompt(trial.packet), tool_catalog=[], clean_home=True,
            output_schema=schema, base_instructions=diagnostic.judge.JUDGE_SYSTEM, timeout=900, require_chatgpt=True,
        )
        self.assertEqual((result.code, result.stream, result.error), (0, b"stream", None))


class JudgeReceiptTests(unittest.TestCase):
    def test_invalid_judge_preserves_all_audited_sibling_receipts(self):
        roster = trials(3)
        results = [observed(0), observed(1, error="OSError"), observed(2)]
        writes = {}
        audited = []

        def writer(path, data):
            self.assertNotIn(path, writes)
            writes[path] = data

        def audit(action, path):
            self.assertEqual(action, "After")
            self.assertIn(path / "judge.json", writes)
            audited.append(path)

        verdict = {"notes": []}

        def validate(code, status, model, scenario):
            return (verdict, {}, []) if code == 0 else (None, None, ["Codex judge did not complete"])

        with (
            mock.patch.object(diagnostic.judge, "validate_codex_judge", side_effect=validate),
            mock.patch.object(diagnostic.grade, "grade_one") as grade,
            self.assertRaisesRegex(common.HarnessError, "Result-1/j1 did not settle"),
        ):
            diagnostic.finish_cohort(roster, results, audit=audit, writer=writer, frozen_sha="frozen", provenance={})
        grade.assert_not_called()
        self.assertEqual(audited, [trial.directory for trial in roster])
        receipt = json.loads(writes[roster[1].directory / "judge.json"])
        self.assertIn("judge driver raised OSError", receipt["problems"])
        self.assertEqual(writes[roster[2].directory / "stream.jsonl"], b"stream 2")

    def test_valid_grade_requires_observed_app_server_exit(self):
        roster = trials(1)
        result = diagnostic.Observation(0, b"stream", b"", {}, 10, None)
        writes = {}
        with (
            mock.patch.object(diagnostic.judge, "validate_codex_judge", return_value=({}, {}, [])),
            self.assertRaisesRegex(common.HarnessError, "did not settle"),
        ):
            diagnostic.finish_cohort(roster, [result], audit=mock.Mock(),
                                     writer=lambda path, raw: writes.update({path: raw}),
                                     frozen_sha="frozen", provenance={})
        receipt = json.loads(writes[roster[0].directory / "judge.json"])
        self.assertEqual(receipt["problems"], ["judge app-server process exit was not observed"])

    def test_cli_keeps_serial_default_and_accepts_four(self):
        arguments = ["judge.py", "--campaign-root", "campaign", "--actor-root", "actors",
                     "--run-root", "run", "--pwsh", "pwsh", "--actor-spec-sha256", "a",
                     "--disposition-sha256", "b", "--expected-sid", "sid"]
        for extra, width in (([], 1), (["--jobs", "4"], 4)):
            with self.subTest(width=width), mock.patch("sys.argv", arguments + extra):
                with mock.patch.object(diagnostic, "score") as score:
                    self.assertEqual(diagnostic.main(), 0)
                self.assertEqual(score.call_args.args[0].jobs, width)
        with mock.patch("sys.argv", arguments + ["--jobs", "5"]), redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as error:
                diagnostic.main()
        self.assertEqual(error.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
