# SPDX-License-Identifier: Apache-2.0
"""Only explicit native read-rejection receipts permit qualification retry."""
import importlib.util
import contextlib
import copy
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("service_clients", Path(__file__).parents[1] / "check-service-clients.py")
CLIENTS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CLIENTS)


class ServiceClientTests(unittest.TestCase):
    def metadata_fixture(self):
        fixture = self.fixture()
        fixture.control = Path("fictional-control")
        return fixture

    def test_registry_contention_repeats_only_the_exact_read_with_remaining_budget(self):
        fixture, clock = self.metadata_fixture(), [0.0]
        method = {"job": {"id": "accepted-job"}}
        replies = [{"status": "failure", "code": "registry_busy"},
                   {"status": "job", "job": {"id": "accepted-job", "state": "succeeded"}}]
        def run(*args, **kwargs):
            clock[0] += 0.1
            return subprocess.CompletedProcess(args, 0, json.dumps(replies.pop(0)), "")
        with patch.object(fixture, "run", side_effect=run) as transport, \
                patch.object(CLIENTS.time, "monotonic", side_effect=lambda: clock[0]), \
                patch.object(CLIENTS.time, "sleep", side_effect=lambda delay: clock.__setitem__(0, clock[0] + delay)):
            self.assertEqual(fixture.rpc(method, deadline=1.0)["job"]["id"], "accepted-job")
        self.assertEqual(transport.call_count, 2)
        first, second = transport.call_args_list
        self.assertEqual(first.args, second.args)
        self.assertEqual(first.kwargs["input"], json.dumps(method))
        self.assertEqual(first.kwargs["input"], second.kwargs["input"])
        self.assertAlmostEqual(first.kwargs["timeout"], 1.0)
        self.assertAlmostEqual(second.kwargs["timeout"], 0.875)

    def test_metadata_mutations_unknown_failures_and_transport_errors_never_replay(self):
        fixture = self.metadata_fixture()
        busy = {"status": "failure", "code": "registry_busy"}
        methods = ["request_epoch"] + [{name: {"id": "exact"}} for name in
                   ("enqueue", "configure_project", "register", "reserve_project", "pause", "retry", "cancel", "unknown")]
        for method, reply in [(method, busy) for method in methods] + [
                ({"job": {"id": "exact"}}, {"status": "failure", "code": "invalid_registry_request"}),
                ({"job": {"id": "exact"}}, {"status": "failure", "code": None}),
                ({"job": {"id": "exact"}}, {**busy, "unexpected": True})]:
            with self.subTest(method=method, reply=reply), patch.object(fixture, "run", return_value=
                    subprocess.CompletedProcess([], 0, json.dumps(reply), "")) as transport:
                with self.assertRaises(AssertionError):
                    fixture.rpc(method)
                transport.assert_called_once()
        for error in (subprocess.TimeoutExpired("fictional-native", 1), subprocess.CalledProcessError(1, "fictional-native")):
            with patch.object(fixture, "run", side_effect=error) as transport:
                with self.assertRaises(type(error)):
                    fixture.rpc({"job": {"id": "exact"}})
                transport.assert_called_once()

    def test_registry_read_contention_has_finite_attempt_and_wall_clock_budgets(self):
        fixture, clock = self.metadata_fixture(), [0.0]
        busy = subprocess.CompletedProcess([], 0, json.dumps({"status": "failure", "code": "registry_busy"}), "")
        with patch.object(fixture, "run", return_value=busy) as transport, \
                patch.object(CLIENTS.time, "monotonic", return_value=0), patch.object(CLIENTS.time, "sleep"):
            with self.assertRaisesRegex(AssertionError, "registry_busy"):
                fixture.rpc({"job": {"id": "exact"}}, deadline=1.0)
            self.assertEqual(transport.call_count, 41)
        def run(*args, **kwargs):
            clock[0] += 0.1
            return busy
        with patch.object(fixture, "run", side_effect=run) as transport, \
                patch.object(CLIENTS.time, "monotonic", side_effect=lambda: clock[0]):
            with self.assertRaises(CLIENTS.ObservationTimeout):
                fixture.rpc({"job": {"id": "exact"}}, deadline=0.1)
            transport.assert_called_once()

    def test_expected_native_refusal_is_observed_once_without_permitting_other_failures(self):
        method = {"stop_embedding": {"worker": "foreign", "force": True}}
        refused = {"status": "failure", "code": "embedding_worker_changed"}
        with patch.object(CLIENTS.time, "sleep") as sleep:
            transport = unittest.mock.Mock(return_value=refused)
            self.assertEqual(CLIENTS.metadata_rpc(transport, method, expected_failure="embedding_worker_changed"), refused)
            transport.assert_called_once()
            self.assertEqual(json.loads(transport.call_args.args[0]), method)
            transport.reset_mock(return_value=True)
            transport.return_value = {"status": "failure", "code": "registry_response_unknown_retry_with_key"}
            with self.assertRaisesRegex(AssertionError, "registry_response_unknown"):
                CLIENTS.metadata_rpc(transport, method, expected_failure="embedding_worker_changed")
            transport.assert_called_once()
            sleep.assert_not_called()

    def test_job_polling_keeps_one_deadline_and_the_original_accepted_receipt(self):
        fixture, clock, deadlines = self.metadata_fixture(), [0.0], []
        fixture.job_timeout = 0.5
        def rpc(method, *, deadline=None):
            self.assertEqual(method, {"job": {"id": "accepted-job"}})
            deadlines.append(deadline)
            return {"job": {"id": "accepted-job", "state": "running" if len(deadlines) == 1 else "succeeded"}}
        with patch.object(fixture, "rpc", side_effect=rpc), \
                patch.object(CLIENTS.time, "monotonic", side_effect=lambda: clock[0]), \
                patch.object(CLIENTS.time, "sleep", side_effect=lambda delay: clock.__setitem__(0, clock[0] + delay)):
            self.assertEqual(fixture.wait({"job": {"id": "accepted-job"}})["state"], "succeeded")
        self.assertEqual(deadlines, [0.5, 0.5])

    def test_runtime_receipts_stay_outside_the_mac_bundle(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            nested = root / "AIRK.app/Contents/MacOS"
            nested.mkdir(parents=True)
            (nested / "airk").write_bytes(b"fictional native payload")
            for selected in (root, nested):
                fixture = CLIENTS.Fixture(selected, root)
                try:
                    self.assertEqual(fixture.runtime, root)
                    self.assertEqual(fixture.binary, nested / "airk")
                finally:
                    fixture.log.close()
            unknown = root / "Other.app/Contents/MacOS"
            unknown.mkdir(parents=True)
            self.assertEqual(CLIENTS.runtime_root(unknown), unknown)

    def envelope(self, value, failed=False):
        return {"isError": failed, "content": [{"type": "text", "text": json.dumps(value)}]}

    def test_unknown_or_submitted_results_never_replay(self):
        for code, reason, submitted in (("direct_query_admission", "busy", True),
                                         ("worker_lost", "busy", False),
                                         ("direct_query_admission", "unknown", False)):
            fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
            fixture.arguments = lambda _: []
            failure = self.envelope({"error": {"code": code, "reason": reason, "query_submitted": submitted}}, True)
            with patch.object(fixture, "run", return_value=subprocess.CompletedProcess([], 1, json.dumps(failure), "failed")) as run:
                with self.assertRaises(AssertionError):
                    fixture.query("fixture", "search", "marker")
                run.assert_called_once()

    def test_only_an_explicit_unsubmitted_receipt_retries(self):
        fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
        fixture.arguments = lambda _: []
        busy = self.envelope({"error": {"code": "direct_query_admission", "reason": "busy", "query_submitted": False, "retryable": True, "fresh_brokerage_required": False, "retry_after_ms": 25}}, True)
        accepted = self.envelope({"request_id": "accepted", "hits": []})
        replies = [subprocess.CompletedProcess([], 1, json.dumps(busy), "busy"),
                   subprocess.CompletedProcess([], 0, json.dumps(accepted), "")]
        with patch.object(fixture, "run", side_effect=replies) as run, patch.object(CLIENTS.time, "sleep") as sleep:
            self.assertEqual(fixture.query("fixture", "search", "marker")["request_id"], "accepted")
            self.assertEqual(run.call_count, 2)
            sleep.assert_called_once_with(0.025)

    def test_publication_retry_preserves_exact_revision_and_cursor(self):
        fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
        fixture.arguments = lambda _: []
        args = ("tools", "call", "airk_outline", "--args", '{"revision":"saved","cursor":"exact"}')
        hint = {"status": "retry", "retryable": True, "action": "repeat_same_request",
                "code": "publication_busy", "retry_after_ms": 25}
        replies = [subprocess.CompletedProcess([], 1, json.dumps(self.envelope(hint, True)), ""),
                   subprocess.CompletedProcess([], 0, json.dumps(self.envelope({"items": []})), "")]
        with patch.object(fixture, "run", side_effect=replies) as run, patch.object(CLIENTS.time, "sleep"):
            self.assertEqual(fixture.query("fixture", *args), {"items": []})
            self.assertEqual(run.call_args_list[0].args, run.call_args_list[1].args)
            self.assertLessEqual(run.call_args_list[1].kwargs["timeout"], run.call_args_list[0].kwargs["timeout"])
        for key, value in (("code", "unknown"), ("retryable", False), ("action", "refresh_revision"),
                           ("retry_after_ms", 0), ("retry_after_ms", True), ("retry_after_ms", 1001)):
            self.assertIsNone(CLIENTS.read_retry_delay(self.envelope({**hint, key: value}, True)))

    def test_setup_rejection_is_unsubmitted_and_honors_native_backoff(self):
        error = {"code": "direct_query_setup", "reason": "busy", "query_submitted": False,
                 "retryable": True, "fresh_brokerage_required": True, "retry_after_ms": 100}
        self.assertEqual(CLIENTS.read_retry_delay(self.envelope({"error": error}, True)), 0.1)
        self.assertIsNone(CLIENTS.read_retry_delay(self.envelope({"error": {**error, "query_submitted": True}}, True)))


    def available(self):
        return {"index_state": "available", "collection": "fixture", "files": 1,
                "publication": {"collection": "fixture", "token": "unchanged-token"},
                "semantic_readiness": {"state": "ready", "completed_artifacts": 1}}

    def unavailable(self, code="index_worker_unavailable"):
        return {"index_state": "unavailable", "project": "project", "collection": "fixture",
                "publication": None, "files": None, "chunks": None,
                "index_error": {"code": code, "message": "private-source-message-must-not-be-logged"},
                "runtime": {"service_error": None, "service": {"responsive": True, "accepting": True, "instance": "owner"},
                            "index_worker": {"process": None}},
                "work": {"basis": "instance_bound_service_metadata", "serving_worker_unavailable": True,
                         "failed_observations": [], "worker": {"identity": None, "automatic_start_blocked": False}}}

    def fixture(self):
        fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
        fixture.arguments = lambda _: ["--service-project", "project"]
        fixture.projects = {"fixture": {"collection": "fixture", "binding": "project"}}
        fixture.command_timeout = 120
        fixture.binary, fixture.runtime, fixture.environment = Path("fixture-native"), Path.cwd(), {}
        return fixture

    def test_unavailable_observation_requires_scoped_snapshot_and_propagates_remaining_transport_budget(self):
        for code in ("index_status_unavailable", "index_worker_unavailable"):
            with self.subTest(code=code):
                fixture, clock, calls = self.fixture(), [0.0], []
                values = [self.unavailable(code), self.available()]
                def run(command, **kwargs):
                    calls.append((command, kwargs["timeout"]))
                    clock[0] += 0.3
                    return subprocess.CompletedProcess(command, 0, json.dumps(self.envelope(values.pop(0))), "")
                def sleep(seconds):
                    clock[0] += seconds
                diagnostics = io.StringIO()
                with patch.object(CLIENTS.subprocess, "run", side_effect=run), patch.object(CLIENTS.time, "monotonic", side_effect=lambda: clock[0]), \
                        patch.object(CLIENTS.time, "sleep", side_effect=sleep), contextlib.redirect_stderr(diagnostics):
                    result = fixture.published_status("fixture", deadline=1.0)
                self.assertEqual(result["publication"]["token"], "unchanged-token")
                self.assertEqual(calls[0][0], calls[1][0])
                self.assertAlmostEqual(calls[0][1], 1.0)
                self.assertAlmostEqual(calls[1][1], 0.675)
                self.assertNotIn("private-source", diagnostics.getvalue())

    def test_opaque_unavailable_expires_without_claiming_transience_or_leaking_messages(self):
        fixture, clock, calls = self.fixture(), [0.0], []
        def run(command, **kwargs):
            calls.append(kwargs["timeout"])
            clock[0] += kwargs["timeout"]
            return subprocess.CompletedProcess(command, 0, json.dumps(self.envelope(self.unavailable("index_status_unavailable"))), "")
        with patch.object(CLIENTS.subprocess, "run", side_effect=run), patch.object(CLIENTS.time, "monotonic", side_effect=lambda: clock[0]):
            with self.assertRaisesRegex(AssertionError, "deadline") as error:
                fixture.published_status("fixture", deadline=0.5)
        self.assertEqual(calls, [0.5])
        self.assertNotIn("private-source", str(error.exception))

    def test_available_observation_after_deadline_is_rejected(self):
        clock = [0.0]
        def late(_):
            clock[0] = 1.0
            return self.available()
        with patch.object(CLIENTS.time, "monotonic", side_effect=lambda: clock[0]):
            with self.assertRaisesRegex(CLIENTS.ObservationTimeout, "deadline"):
                CLIENTS.observe_publication(late, "fixture", "project", 1.0)

    def test_malformed_foreign_or_explicitly_unhealthy_status_never_becomes_success(self):
        for code in ("index_status_unavailable", "index_worker_unavailable"):
            original = self.unavailable(code)
            changes = [("collection", "foreign") if code == "index_status_unavailable" else ("project", "foreign"),
                       ("runtime", []), ("work", "malformed"), ("files", 0),
                       ("index_error", {"code": "unknown", "message": "private-source"})]
            values = [{**copy.deepcopy(original), key: value} for key, value in changes]
            for path, changed in ((["runtime", "service"], None), (["runtime", "service", "responsive"], False),
                                  (["work", "worker", "automatic_start_blocked"], True),
                                  (["work", "failed_observations"], ["unknown"]), (["work", "worker", "identity"], "bad"),
                                  (["work", "worker", "identity"], {"project": "foreign", "service": "owner"})):
                value = copy.deepcopy(original)
                parent = value
                for key in path[:-1]:
                    parent = parent[key]
                parent[path[-1]] = changed
                values.append(value)
            for value in values:
                with self.subTest(code=code, value_type=type(value)), self.assertRaises(AssertionError) as error:
                    CLIENTS.publication_pending(value, "fixture", "project")
                self.assertNotIn("private-source", str(error.exception))
        for changed in ({"token": ""}, {"collection": "foreign"}):
            value = self.available()
            value["publication"].update(changed)
            with self.assertRaises(AssertionError):
                CLIENTS.publication_pending(value, "fixture", "project")
        self.assertFalse(CLIENTS.status_summary({})["service_observation_available"])

    def test_raw_status_is_not_replaced_with_a_publication_and_nonordinary_exit_never_replays(self):
        fixture = self.fixture()
        unavailable = self.unavailable()
        with patch.object(fixture, "run", return_value=subprocess.CompletedProcess([], 0, json.dumps(self.envelope(unavailable)), "")) as run:
            self.assertEqual(fixture.tool("fixture", "airk_status"), unavailable)
            run.assert_called_once()
        hint = self.envelope({"status": "retry", "code": "publication_busy", "retryable": True,
                              "action": "repeat_same_request", "retry_after_ms": 25}, True)
        for code in (-9, 7):
            with patch.object(fixture, "run", return_value=subprocess.CompletedProcess([], code, json.dumps(hint), "")) as run:
                with self.assertRaises(AssertionError):
                    fixture.query("fixture", "search", "exact")
                run.assert_called_once()
        with patch.object(fixture, "run") as run:
            with self.assertRaises(AssertionError):
                fixture.query("fixture", "index")
            run.assert_not_called()

    def test_admission_contract_rejects_missing_fields_and_single_attempt_measurements_do_not_replay(self):
        error = {"code": "direct_query_admission", "reason": "busy", "query_submitted": False,
                 "retryable": True, "fresh_brokerage_required": False, "retry_after_ms": 25}
        for key in error:
            changed = {name: value for name, value in error.items() if name != key}
            self.assertIsNone(CLIENTS.read_retry_delay(self.envelope({"error": changed}, True)))
        for delay in (True, 0, 1001, "25"):
            self.assertIsNone(CLIENTS.read_retry_delay(self.envelope({"error": {**error, "retry_after_ms": delay}}, True)))
        fixture = self.fixture()
        with patch.object(fixture, "run", return_value=subprocess.CompletedProcess([], 1, json.dumps(self.envelope({"error": error}, True)), "")) as run:
            with self.assertRaises(AssertionError):
                fixture.query("fixture", "search", "exact", retry=False)
            run.assert_called_once()

    def test_accounting_waits_only_before_query_submission(self):
        fixture = self.fixture()
        admission = self.envelope({"error": {"code": "direct_query_admission", "reason": "busy", "query_submitted": False,
            "retryable": True, "fresh_brokerage_required": False, "retry_after_ms": 25}}, True)
        accepted = self.envelope({"hits": [1]})
        replies = [subprocess.CompletedProcess([], 1, json.dumps(admission), ""), subprocess.CompletedProcess([], 0, json.dumps(accepted), "")]
        with patch.object(fixture, "run", side_effect=replies) as run, patch.object(CLIENTS.time, "sleep"):
            self.assertEqual(fixture.query("fixture", "search", "exact", retry=False, admission_only=True), {"hits": [1]})
            self.assertEqual(run.call_count, 2)
        publication = self.envelope({"status": "retry", "code": "publication_busy", "retryable": True,
                                    "action": "repeat_same_request", "retry_after_ms": 25}, True)
        with patch.object(fixture, "run", return_value=subprocess.CompletedProcess([], 1, json.dumps(publication), "")) as run:
            with self.assertRaisesRegex(AssertionError, "publication_busy"):
                fixture.query("fixture", "search", "exact", retry=False, admission_only=True)
            run.assert_called_once()

    def test_semantic_observations_reject_missing_or_unknown_fields(self):
        for value in ({}, {"semantic_readiness": {"state": "unknown", "completed_artifacts": 1}},
                      {"semantic_readiness": {"state": "ready", "completed_artifacts": True}}):
            with self.assertRaises(AssertionError):
                CLIENTS.semantic_readiness(value)
        self.assertEqual(CLIENTS.semantic_readiness({"semantic_readiness": {"state": "failed", "completed_artifacts": 0}})["state"], "failed")

    def test_measurement_guard_owns_existing_advisory_lock_without_changing_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            owner = Path(directory).resolve()
            data = owner / "data"
            lock = data / "collections-1.4/.writer-locks/fixture"
            lock.parent.mkdir(parents=True)
            lock.write_bytes(b"")
            fixture = self.fixture()
            fixture.owner = owner
            fixture.projects["fixture"]["data_root"] = str(data)
            with fixture.hold_publication("fixture"):
                with patch.object(CLIENTS.time, "monotonic", side_effect=[0, 6]), self.assertRaises(OSError):
                    with fixture.hold_publication("fixture"):
                        self.fail("Second writer acquired the held native advisory range")
            with fixture.hold_publication("fixture"):
                pass
            self.assertEqual(lock.read_bytes(), b"")
            self.assertFalse((data / "collections-1.4/fixture/writer.lock").exists())


if __name__ == "__main__":
    unittest.main()
