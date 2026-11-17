# SPDX-License-Identifier: Apache-2.0
"""Pure harness-state regressions; these do not qualify a native audit."""
import ast
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("usage_stable_driver", Path(__file__).resolve().parents[1] / "check-usage-inputs.py")
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


class Fixture:
    def __init__(self, states):
        self.states = iter(states)
        self.calls = []
        self.projects = {"p": {"binding": "actual-fixture-binding"}}

    def observe(self, *_):
        state = next(self.states)
        self.calls.append(("observe", state))
        return state

    def rpc(self, method):
        self.calls.append(("pause", method["pause"]["paused"]))

    def enqueue(self, project, kind):
        self.calls.append(("enqueue", project, kind))
        return {"job": {"id": "saved-job"}}

    def wait(self, receipt):
        self.calls.append(("wait", receipt))

    @driver.contextmanager
    def hold_publication(self, project):
        self.calls.append(("hold", project))
        try:
            yield
        finally:
            self.calls.append(("release", project))

    def query(self, *_args, **_kwargs):
        raise AssertionError("Preflight must not submit a measured query")


def state(paused=False, active=0, veto=False):
    return {"paused": paused, "active_jobs": active, "audit_veto": veto}


class StablePublicationTests(unittest.TestCase):
    def enter(self, fixture):
        return patch.object(driver, "usage_quiescence", fixture.observe)

    def test_unresolved_false_clean_never_enters_writer_hold(self):
        f = Fixture([state(), state(veto=True)])
        with self.enter(f), self.assertRaisesRegex(AssertionError, "Unresolved audit"):
            with driver.stable_usage_publication(f, "p"):
                self.fail("Veto was waived")
        self.assertNotIn(("hold", "p"), f.calls)
        self.assertEqual(f.calls[-1], ("pause", False))

    def test_paused_running_verify_drains_before_hold_and_restores_pause(self):
        f = Fixture([state(True, 1, True), state(False, 1, True), state(), state(True)])
        with self.enter(f), patch.object(driver.time, "sleep"):
            with driver.stable_usage_publication(f, "p"):
                self.assertEqual(f.calls[-1], ("hold", "p"))
        self.assertEqual(f.calls[1:3], [("pause", False), ("enqueue", "p", "verify")])
        self.assertEqual(f.calls[-2:], [("release", "p"), ("pause", True)])

    def test_claim_between_drain_and_pause_is_resumed_and_drained(self):
        f = Fixture([state(), state(), state(True, 1, True), state(False, 1, True), state(), state(True)])
        with self.enter(f), patch.object(driver.time, "sleep"):
            with driver.stable_usage_publication(f, "p"):
                pass
        self.assertEqual(sum(call[0] == "enqueue" for call in f.calls), 1)
        self.assertEqual([call for call in f.calls if call[0] == "pause"],
                         [("pause", True), ("pause", False), ("pause", True), ("pause", False)])
        self.assertEqual(sum(call[0] == "hold" for call in f.calls), 1)

    def test_repeated_race_fails_without_hold_and_restores_prior(self):
        f = Fixture([state(), *[value for _ in range(4) for value in (state(), state(True, 1))]])
        with self.enter(f), self.assertRaisesRegex(AssertionError, "raced repeatedly"):
            with driver.stable_usage_publication(f, "p"):
                self.fail("Repeated race admitted measurement")
        self.assertNotIn(("hold", "p"), f.calls)
        self.assertEqual(f.calls[-1], ("pause", False))

    def test_measurement_failure_releases_hold_before_pause_restore(self):
        f = Fixture([state(), state(), state(True)])
        original_rpc = f.rpc
        def restore_fails(method):
            original_rpc(method)
            if method["pause"]["paused"] is False:
                raise RuntimeError("restore failure")
        f.rpc = restore_fails
        with self.enter(f), self.assertRaisesRegex(RuntimeError, "original failure") as captured:
            with driver.stable_usage_publication(f, "p"):
                raise RuntimeError("original failure")
        self.assertEqual(f.calls[-2:], [("release", "p"), ("pause", False)])
        self.assertTrue(any("restore failure" in note for note in captured.exception.__notes__))

    def test_atomic_metadata_detects_running_job_and_actual_veto(self):
        with tempfile.TemporaryDirectory() as temporary:
            f = Fixture([])
            f.owner = Path(temporary).resolve()
            f.control = f.owner / "control"
            f.control.mkdir()
            path = f.control / "service-1.4.sqlite3"
            with driver.sqlite3.connect(path) as sql:
                sql.executescript("CREATE TABLE projects(binding TEXT,paused INTEGER);"
                    "CREATE TABLE service_jobs(project TEXT,state TEXT);"
                    "CREATE TABLE service_recovery_budget(scope TEXT);")
                sql.execute("INSERT INTO projects VALUES(?,1)", (f.projects["p"]["binding"],))
                sql.execute("INSERT INTO service_jobs VALUES(?,'running')", (f.projects["p"]["binding"],))
                sql.execute("INSERT INTO service_recovery_budget VALUES(?)", ("integrity:" + f.projects["p"]["binding"],))
            before = path.read_bytes()
            self.assertEqual(driver.usage_quiescence(f, "p"), state(True, 1, True))
            self.assertEqual(path.read_bytes(), before)

    def test_original_measured_body_and_exact_count_assertions_are_preserved(self):
        source = Path(driver.__file__).read_text(encoding="utf-8")
        tree = ast.parse(source)
        holds = [node for node in ast.walk(tree) if isinstance(node, ast.With)
                 and any(isinstance(item.context_expr, ast.Call)
                         and isinstance(item.context_expr.func, ast.Name)
                         and item.context_expr.func.id == "stable_usage_publication" for item in node.items)]
        self.assertEqual(len(holds), 1)
        segment = ast.get_source_segment(source, holds[0])
        for exact in ["max_workers=4", "range(40)", '== 40', '== 41', 'native_calls += 40']:
            self.assertIn(exact, segment)
        self.assertIn('retry=False, admission_only=True', source)
        self.assertEqual(sum(isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                             and node.func.id == "run" and node.args
                             and isinstance(node.args[0], ast.Constant) and node.args[0].value == "search"
                             for node in ast.walk(holds[0])), 2)


if __name__ == "__main__":
    unittest.main()
