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
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location("service_clients", Path(__file__).parents[1] / "check-service-clients.py")
CLIENTS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CLIENTS)


class ServiceClientTests(unittest.TestCase):
    def test_management_arguments_use_exact_saved_registration_while_modern_evidence_stays_scoped(self):
        for modern in (False, True):
            with self.subTest(single_image=modern):
                fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
                fixture.single_image = modern
                fixture.control = Path("owned-control")
                registration = {"binding":"native-project-binding","root":"native-original-source",
                                "data_root":"native-owned-index-data","collection":"native-collection"}
                fixture.projects = {"fixture-alias":registration}
                fixture.scopes = {"fixture-alias":{"context_key":"original-key","namespace":"original-namespace",
                                                 "expected_catalog_revision":4,"grant_id":"original-grant"}}
                fixture.rpc = Mock(return_value={"descriptor":{"namespace":"original-namespace","catalog_revision":5}})
                expected = ["--service-runtime-dir","owned-control","--service-project",registration["binding"],
                            "--root",registration["root"],"--data-dir",registration["data_root"],
                            "--collection",registration["collection"]]
                self.assertEqual(fixture.management_arguments("fixture-alias"), expected)
                fixture.rpc.assert_not_called()
                with self.assertRaises(KeyError):
                    fixture.management_arguments("unregistered-foreign-project")
                evidence = fixture.arguments("fixture-alias")
                if modern:
                    self.assertEqual(evidence[:3], ["--service-runtime-dir","owned-control","--catalog-session"])
                    self.assertEqual(json.loads(evidence[3]), {**fixture.scopes["fixture-alias"],"expected_catalog_revision":5})
                    self.assertNotIn("--service-project", evidence)
                else:
                    self.assertEqual(evidence, expected)
                self.assertEqual(fixture.projects["fixture-alias"], registration)
                self.assertEqual(fixture.scopes["fixture-alias"]["expected_catalog_revision"], 4)

    def test_shared_fixture_callers_leave_reserved_native_state_uncreated(self):
        scripts = ("benchmark-code.py", "benchmark-usage.py", "check-document-tables.py",
                   "check-publication-faults.py", "check-usage-inputs.py", "check-storage-lifecycle.py")
        for script in scripts:
            with self.subTest(script=script), tempfile.TemporaryDirectory() as temporary:
                spec = importlib.util.spec_from_file_location("layout_" + script.replace("-", "_"), Path(__file__).parents[1] / script)
                module = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(module)
                binary = Path(temporary) / "airk"
                binary.write_bytes(b"fixture only")
                def at_boundary(runtime, owner):
                    self.assertFalse((owner / "state").exists(), "Native state must be created privately by the installer")
                    self.assertTrue((owner / "index-data").is_dir())
                    raise RuntimeError("verified roots before native stage")
                arguments = [script, "--binary", str(binary)]
                if script.startswith("benchmark-"):
                    arguments += ["--output", str(Path(temporary) / "report.json")]
                with contextlib.ExitStack() as stack:
                    stack.enter_context(patch("sys.argv", arguments))
                    stack.enter_context(patch.object(module.clients, "owned_client_fixture", tempfile.TemporaryDirectory))
                    target = "UsageFixture" if script == "check-usage-inputs.py" else "FaultFixture" if script == "check-publication-faults.py" else "Fixture"
                    owner = module if target != "Fixture" else module.clients
                    constructor = stack.enter_context(patch.object(owner, target, side_effect=at_boundary))
                    if script == "check-usage-inputs.py":
                        stack.enter_context(patch.object(module, "retained_usage_fixture", tempfile.TemporaryDirectory))
                    with self.assertRaisesRegex(RuntimeError, "verified roots"):
                        if script == "check-publication-faults.py":
                            base = Path(temporary) / "publication"
                            base.mkdir()
                            module.run_campaign(binary, base, 1, None)
                        else:
                            module.main()
                    constructor.assert_called_once()

    def test_publication_fault_signals_only_retained_child_after_native_identity_check(self):
        spec = importlib.util.spec_from_file_location("fault_identity_test", Path(__file__).parents[1] / "check-publication-faults.py")
        faults = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(faults)
        with tempfile.TemporaryDirectory() as temporary:
            owner = Path(temporary).resolve()
            binary = owner / "install" / "runtimes" / "fixture" / "airk"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"sealed fixture")
            fixture = faults.FaultFixture.__new__(faults.FaultFixture)
            fixture.owner, fixture.install, fixture.binary = owner, owner / "install", binary
            fixture.single_image = True
            fixture.managed_health = {"process_id": 123, "instance": "native-instance"}
            fixture.assert_alive = Mock()
            child = Mock(pid=123, returncode=-9)
            child.poll.return_value = None
            fixture.child = child
            proof = fixture.inject_death()
            fixture.assert_alive.assert_called_once()
            child.kill.assert_called_once()
            child.wait.assert_called_once_with(timeout=30)
            self.assertEqual(proof["owner"], fixture.managed_health)
            self.assertTrue(proof["observed_exit"])
            self.assertTrue((owner / "fault-process-receipts.jsonl").is_file())
            for mismatch in ("native", "pid", "outside"):
                child.reset_mock()
                fixture.assert_alive = Mock(side_effect=AssertionError("changed native instance") if mismatch == "native" else None)
                fixture.managed_health = {"process_id": 124 if mismatch == "pid" else 123}
                fixture.binary = owner if mismatch == "outside" else binary
                with self.subTest(mismatch=mismatch), self.assertRaises(AssertionError):
                    fixture.inject_death()
                child.kill.assert_not_called()

    def test_usage_benchmark_uses_installed_image_and_owned_accounting_authority(self):
        spec = importlib.util.spec_from_file_location("usage_command_test", Path(__file__).parents[1] / "benchmark-usage.py")
        usage = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(usage)
        fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
        fixture.binary, fixture.control = Path("installed/airk"), Path("private/control")
        fixture.projects = {"usage-benchmark": {"binding": "owned", "root": "source", "data_root": "index-data/usage", "collection": "usage"}}
        fixture.arguments = Mock(side_effect=AssertionError("Owner accounting must not use evidence grant"))
        for args in (["usage", "report"], ["usage", "clear"], ["usage", "configure", "--mode", "disabled"]):
            command = usage.usage_command(fixture, args)
            self.assertEqual(command[0], str(fixture.binary))
            self.assertEqual(command[1:11], ["--service-runtime-dir", str(fixture.control), "--service-project", "owned",
                                           "--root", "source", "--data-dir", "index-data/usage", "--collection", "usage"])
            self.assertEqual(command[11:], args)
        with self.assertRaises(AssertionError):
            usage.usage_command(fixture, ["search", "fictional"])
        fixture.arguments.assert_not_called()

    def test_fault_transition_observes_initial_exit_without_signaling_a_discovered_pid(self):
        spec = importlib.util.spec_from_file_location("fault_drain_test", Path(__file__).parents[1] / "check-publication-faults.py")
        faults = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(faults)
        fixture = Mock(binary=Path(__file__).resolve(), control=Path("owned/control"))
        fixture.assert_alive = Mock()
        fixture.managed_health = {"process_id": 123, "instance": "original"}
        held = Mock()
        held.observation.return_value = {"path": str(fixture.binary), "user": "owner"}
        held.gone.side_effect = [False, True]
        backend = Mock(user="owner")
        backend.open.return_value = held
        fixture.run.side_effect = [subprocess.CompletedProcess([], 0, '{"status":"stopping"}'), subprocess.CompletedProcess([], 1, '')]
        with patch.object(faults.platform, "system", return_value="Windows"), patch.object(faults.processes, "_backend", return_value=backend):
            receipt = faults.drain_initial_owner(fixture)
        self.assertEqual(receipt, {"instance": "original", "process_id": 123, "observed_exit": True, "signals_sent": 0})
        held.close.assert_called_once()
        fixture.assert_alive.assert_called_once()
        fixture.run.assert_any_call("service", "shutdown", "--runtime-dir", fixture.control)
        fixture.run.reset_mock()
        held.gone.side_effect = None
        held.gone.return_value = False
        held.observation.return_value = {"path": str(fixture.binary), "user": "foreign"}
        with patch.object(faults.platform, "system", return_value="Windows"), patch.object(faults.processes, "_backend", return_value=backend), self.assertRaises(AssertionError):
            faults.drain_initial_owner(fixture)
        fixture.run.assert_not_called()

    def test_explicit_tmpfs_fault_fixture_retains_failed_state_and_cleans_success(self):
        spec = importlib.util.spec_from_file_location("fault_retention_test", Path(__file__).parents[1] / "check-publication-faults.py")
        faults = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(faults)
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(RuntimeError, "unknown stop"):
                with faults.owned_fault_fixture(Path(temporary)) as failed:
                    (Path(failed) / "receipt").write_text("owned unresolved state")
                    raise RuntimeError("unknown stop")
            self.assertEqual((Path(failed) / "receipt").read_text(), "owned unresolved state")
            with faults.owned_fault_fixture(Path(temporary)) as success:
                self.assertTrue(Path(success).is_relative_to(Path(temporary)))
            self.assertFalse(Path(success).exists())

    def test_source_work_completion_uses_exact_management_project_and_watermarks(self):
        project = "owned-project"
        progress = {"status": "progress", "project": project, "pending_job": None, "queued_jobs": [],
                    "observation": None, "queue_truncated": False, "project_paused": False}
        watch = {"status": "watch", "project": project, "observation": {"enabled": True, "configuration_observed": True,
                 "requested_revision": 3, "completed_revision": 3, "failure": None,
                 "automatic_retries_exhausted": False, "automatic_retry_canceled": False}}
        self.assertTrue(CLIENTS.source_work_settled(progress, watch, project))
        for field, value in (("pending_job", {"project": project}), ("queued_jobs", [{"project": project}]),
                             ("queue_truncated", True), ("queue_truncated", None), ("project_paused", True),
                             ("observation", {"worker": {"project": project}, "job": {"project": project, "state": "running"}})):
            with self.subTest(busy=field):
                self.assertFalse(CLIENTS.source_work_settled({**progress, field: value}, watch, project))
        for field, value in (("completed_revision", 2), ("enabled", False), ("configuration_observed", False)):
            with self.subTest(watch=field):
                self.assertFalse(CLIENTS.source_work_settled(progress, {**watch, "observation": {**watch["observation"], field: value}}, project))
        for field, value in (("project", "foreign"), ("pending_job", {"project": "foreign"}),
                             ("queued_jobs", [{"project": "foreign"}]),
                             ("observation", {"worker": {"project": "foreign"}, "job": {"project": project}})):
            with self.subTest(crossed=field), self.assertRaises(AssertionError):
                CLIENTS.source_work_settled({**progress, field: value}, watch, project)
        with self.assertRaises(AssertionError):
            CLIENTS.source_work_settled(progress, {**watch, "project": "foreign"}, project)
        for field, value in (("completed_revision", 4), ("requested_revision", True), ("failure", "watch_failed"),
                             ("automatic_retries_exhausted", True), ("automatic_retry_canceled", True)):
            with self.subTest(invalid_watch=field), self.assertRaises(AssertionError):
                CLIENTS.source_work_settled(progress, {**watch, "observation": {**watch["observation"], field: value}}, project)

    def test_scoped_evidence_does_not_claim_scheduler_visibility(self):
        freshness = {"state": "unavailable", "error": "scoped_freshness_unavailable", "live_source_verified": False,
                     "requested_watermark": None, "completed_watermark": None, "running_watermark": None, "last_success_unix_ms": None}
        CLIENTS.assert_scoped_freshness_private({"freshness": freshness})
        for field, value in (("state", "idle"), ("error", None), ("live_source_verified", True),
                             ("requested_watermark", 3), ("completed_watermark", 3), ("running_watermark", 0), ("last_success_unix_ms", 1)):
            with self.subTest(field=field), self.assertRaises(AssertionError):
                CLIENTS.assert_scoped_freshness_private({"freshness": {**freshness, field: value}})
        with self.assertRaises(AssertionError):
            CLIENTS.assert_scoped_freshness_private({"freshness": {key: value for key, value in freshness.items() if key != "completed_watermark"}})

    def test_fixture_cleanup_failure_preserves_primary_and_retains_separate_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
            fixture.owner = Path(temporary)
            fixture.close = Mock(side_effect=RuntimeError("runtime_upgrade_in_progress"))
            primary = AssertionError("original saved-index adoption failure")
            try:
                raise primary
            except AssertionError as observed:
                self.assertFalse(fixture.__exit__(type(observed), observed, observed.__traceback__))
                self.assertIs(observed, primary)
                self.assertEqual(str(observed), "original saved-index adoption failure")
            fixture.close.assert_called_once_with()
            records = list(fixture.owner.glob("managed-cleanup-error-*.json"))
            self.assertEqual(len(records), 1)
            record = json.loads(records[0].read_text(encoding="utf-8"))
            self.assertEqual(record["primary_error"]["message"], str(primary))
            self.assertEqual(record["cleanup_error"]["message"], "runtime_upgrade_in_progress")
            self.assertTrue(record["fixture_preserved"])
            self.assertIn(str(records[0]), primary.__notes__[0])

    def test_fixture_cleanup_failure_without_primary_remains_fatal(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
            fixture.owner = Path(temporary)
            cleanup = RuntimeError("native owner lease remains held")
            fixture.close = Mock(side_effect=cleanup)
            with self.assertRaises(RuntimeError) as caught:
                fixture.__exit__(None, None, None)
            self.assertIs(caught.exception, cleanup)
            self.assertIsNone(fixture.cleanup_failure["primary_error"])
            fixture.close.assert_called_once_with()

    def test_fixture_cleanup_recording_failure_cannot_replace_primary(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
            fixture.owner = Path(temporary) / "missing-owner"
            fixture.close = Mock(side_effect=RuntimeError("pending accepted operation"))
            primary = AssertionError("first failure")
            self.assertFalse(fixture.__exit__(type(primary), primary, None))
            self.assertIn("could not be written", primary.__notes__[0])
            self.assertFalse(fixture.owner.exists())

    def test_owned_context_uses_explicit_private_base_without_changing_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve()
            marker = base/"preexisting"; marker.write_text("preserved")
            original_mode = base.stat().st_mode
            with patch.dict(CLIENTS.os.environ,{"AIRK_PRIVATE_CENSUS_BASE":str(base)}):
                with CLIENTS.owned_client_fixture() as selected:
                    child = Path(selected)
                    self.assertEqual(child.parent,base)
                    self.assertTrue(child.is_dir())
            self.assertFalse(child.exists())
            self.assertEqual(base.stat().st_mode,original_mode)
            self.assertEqual(marker.read_text(),"preserved")
            with patch.dict(CLIENTS.os.environ,{"AIRK_PRIVATE_CENSUS_BASE":"relative/unknown"}):
                with self.assertRaisesRegex(RuntimeError,"absolute"):
                    with CLIENTS.owned_client_fixture():
                        self.fail("invalid base created a fixture")

    def test_maintenance_submission_uses_explicit_owner_and_never_the_read_selector(self):
        fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
        fixture.control = Path("owned-control")
        fixture.projects = {"a":{"binding":"exact-binding","root":"owned-source","data_root":"owned-data","collection":"a"}}
        fixture.arguments = lambda _: self.fail("scoped read selector used for maintenance")
        with patch.object(fixture,"run",return_value=subprocess.CompletedProcess([],0,'{"job":{"id":"one-job"}}',"")) as run, patch.object(fixture,"wait",side_effect=lambda result,_expected:result):
            fixture.job("a","index")
        self.assertEqual(run.call_count,1)
        self.assertEqual(run.call_args.args,("--service-runtime-dir","owned-control","--service-project","exact-binding","--root","owned-source","--data-dir","owned-data","--collection","a","index"))

    def test_modern_service_identity_failure_does_not_submit_stop_to_another_instance(self):
        fixture = CLIENTS.Fixture.__new__(CLIENTS.Fixture)
        fixture.single_image, fixture.managed_started, fixture.managed_stop = True, True, None
        fixture.managed_health = {"instance":"owned","runtime_sha256":"approved","executable_sha256":"main"}
        fixture.log = Mock()
        with patch.object(fixture,"rpc",return_value={"observation":{"instance":"different","runtime_sha256":"approved","executable_sha256":"main"}}), patch.object(fixture,"run") as run:
            with self.assertRaises(AssertionError):
                fixture.close()
        run.assert_not_called()
        self.assertIsNone(fixture.managed_stop)
        fixture.log.close.assert_called_once()

    def test_catalog_and_retry_inputs_include_query_scope_as_a_native_read(self):
        self.assertIn("airk_query_scope", CLIENTS.TOOLS)
        self.assertIn("airk_query_scope", CLIENTS.RETRYABLE_TOOLS)
        self.assertIn("query_scope", CLIENTS.MCP_TOOLS)
        self.assertEqual((len(CLIENTS.TOOLS), len(CLIENTS.MCP_TOOLS)), (19, 29))

    def test_legacy_query_scope_never_accepts_catalog_expansion_or_false_completeness(self):
        reply = {"schema": "airk.query-scope/1", "mode": "legacy", "status": "configuration_required",
                 "projects": [], "indexes": [], "sources": [], "grant": None, "scope_revision": None,
                 "truncated": False, "continuation": None}
        CLIENTS.assert_legacy_query_scope(reply)
        for field, value in (("projects", [{"id": "unselected"}]), ("indexes", ["unselected"]),
                             ("sources", ["private"]), ("grant", "invented"), ("scope_revision", 1),
                             ("mode", "reviewed"), ("status", "complete"), ("truncated", True), ("continuation", "cursor")):
            with self.subTest(field=field), self.assertRaises(AssertionError):
                CLIENTS.assert_legacy_query_scope({**reply, field: value})

    def test_inference_replies_cannot_hide_a_native_exit_abort(self):
        responses = [{"status": "extracted"},
                     {"status": "opened", "protocol": 1, "version": "1.6.4", "identity": "fixture-model"},
                     {"status": "embedded", "vectors": [[0.25, 0.75]], "execution": {}}]
        output = b""
        for response in responses:
            data = json.dumps(response).encode()
            output += len(data).to_bytes(4, "big") + data
        result = subprocess.CompletedProcess([], 0, output, b"")
        CLIENTS.verify_inference_worker_result(result, "1.6.4", "fixture-model", 2)
        for code in (-6, 1):
            failed = subprocess.CompletedProcess([], code, output, b"native exit failure")
            with self.assertRaisesRegex(AssertionError, "Inference worker exited"):
                CLIENTS.verify_inference_worker_result(failed, "1.6.4", "fixture-model", 2)
        for malformed in (output[:-1], output + b"x", (4 << 20 | 1).to_bytes(4, "big")):
            with self.assertRaises(AssertionError):
                CLIENTS.verify_inference_worker_result(subprocess.CompletedProcess([], 0, malformed, b""), "1.6.4", "fixture-model", 2)

    def test_frozen_query_scope_preserves_complete_native_authority_without_catalog_discovery(self):
        selection = {"grant_id": "grt_selected", "expected_grant_revision": 1,
                     "index_id": "idx_selected", "project_ids": ["prj_selected"]}
        grant = {"id": "grt_selected", "revision": 1}
        observation = {"schema": 1, "qualifier": "selected_subset", "provenance": "user_reviewed_project_grant",
                       "grant": grant, "fingerprint": "a" * 64,
                       "members": [{"index_id": "idx_selected", "memberships": [{"project_id": "prj_selected", "membership_revision": 1}],
                                    "source_root_ids": ["src_selected"], "source_revision": 2, "policy_epoch": 0, "policy_digest": None}],
                       "future_provenance": {"retained": True}}
        reply = {"schema": "airk.query-scope/1", "mode": "frozen_single_index", "status": "configuration_required",
                 "catalog_discovery": "unavailable_on_this_connection", "projects": [], "indexes": [], "sources": [],
                 "scope_revision": None, "continuation": None, "truncated": False, "grant": grant,
                 "observed_selection": {"project_ids": ["prj_selected"], "index_ids": ["idx_selected"]},
                 "scope_observation": observation, "reason": "Separate reviewed discovery required."}
        CLIENTS.assert_frozen_query_scope(reply, selection, observation)
        for field in ("scope_revision", "continuation"):
            with self.subTest(missing=field), self.assertRaises(AssertionError):
                CLIENTS.assert_frozen_query_scope({key: value for key, value in reply.items() if key != field}, selection, observation)
        for field, value in (("mode", "reviewed_grant"), ("status", "complete"), ("catalog_discovery", "available"),
                             ("projects", ["foreign"]), ("indexes", ["foreign"]), ("sources", ["foreign"]),
                             ("scope_revision", 1), ("continuation", "cursor"), ("truncated", True),
                             ("grant", {"id": "foreign", "revision": 1}),
                             ("observed_selection", {"project_ids": ["prj_selected", "foreign"], "index_ids": ["idx_selected"]})):
            with self.subTest(field=field), self.assertRaises(AssertionError):
                CLIENTS.assert_frozen_query_scope({**reply, field: value}, selection, observation)
        for field, value in (("fingerprint", "b" * 64), ("provenance", "imported_legacy_snapshot"),
                             ("grant", {"id": "grt_selected", "revision": 2}), ("future_provenance", {})):
            with self.subTest(observation_field=field), self.assertRaises(AssertionError):
                CLIENTS.assert_frozen_query_scope({**reply, "scope_observation": {**observation, field: value}}, selection, observation)
        for field, value in (("index_id", "foreign"), ("memberships", []), ("source_root_ids", ["foreign"]),
                             ("source_revision", 3), ("policy_epoch", 1), ("policy_digest", "b" * 64)):
            changed = copy.deepcopy(observation)
            changed["members"][0][field] = value
            with self.subTest(member_field=field), self.assertRaises(AssertionError):
                CLIENTS.assert_frozen_query_scope({**reply, "scope_observation": changed}, selection, observation)
        for field, value in (("provenance", "imported_legacy_snapshot"), ("qualifier", "complete"),
                             ("grant", {"id": "foreign", "revision": 1}), ("members", [])):
            changed = {**observation, field: value}
            with self.subTest(crossed_baseline=field), self.assertRaises(AssertionError):
                CLIENTS.assert_frozen_query_scope({**reply, "scope_observation": changed}, selection, changed)

    def test_removal_requires_exact_history_proof_and_never_replays_a_pruned_request(self):
        project, job = "a" * 64, "b" * 64
        for outcome in ("confirmed", "crossed", "expired"):
            with self.subTest(outcome=outcome), tempfile.TemporaryDirectory() as directory:
                fixture = self.fixture()
                fixture.control = Path(directory)
                fixture.projects = {"fixture": {"binding": project}}
                fixture.job_timeout = 0.1
                pending = fixture.control / "client-requests"
                pending.mkdir()
                request = {"project": project, "caller": "original-caller", "key": {"nonce": "original-key"}}
                (pending / "original.json").write_text(json.dumps({"request": {"configure_project": request}}), encoding="utf-8")
                observed = {"status": "request_receipt", "job": None, "expired": True}
                if outcome != "expired":
                    observed["removed_project"] = project if outcome == "confirmed" else "c" * 64
                clock, queries = [0.0], []

                def rpc(method, *, deadline=None):
                    queries.append((method, deadline))
                    clock[0] += 0.06
                    return observed

                receipt = subprocess.CompletedProcess([], 0, json.dumps({"status": "job", "job": {"id": job, "project": project}}), "")
                with patch.object(fixture, "enqueue"), patch.object(fixture, "wait"), \
                        patch.object(fixture, "prepare_configuration", return_value={"prepared": "original"}), \
                        patch.object(fixture, "run", return_value=receipt) as submit, \
                        patch.object(fixture, "rpc", side_effect=rpc), \
                        patch.object(CLIENTS.time, "monotonic", side_effect=lambda: clock[0]), \
                        patch.object(CLIENTS.time, "sleep"):
                    if outcome == "confirmed":
                        query, proof = fixture.remove("fixture")
                        self.assertEqual(proof, observed)
                        self.assertIsNone(proof["job"])
                    else:
                        with self.assertRaises(AssertionError):
                            fixture.remove("fixture")
                    submit.assert_called_once_with("service", "requests", "--runtime-dir", fixture.control, "retry", "original")
                expected = {"request_receipt": {"caller": request["caller"], "key": request["key"], "project": project}}
                self.assertTrue(queries)
                self.assertTrue(all(method == expected and deadline == 0.1 for method, deadline in queries))

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
                   ("enqueue", "configure_project", "register", "reserve_project", "pause", "retry", "cancel",
                    "catalog_preview", "catalog_apply", "catalog_source_change_resolve", "unknown")]
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

    def test_source_review_contention_preserves_selection_key_and_original_deadline(self):
        method = {"catalog_source_mutation_draft": {"selection": {
            "key": "same-review-key", "catalog_revision": 7, "request_epoch": 0,
            "change": {"kind": "add_root", "alias": "specs", "path": "fixture-specs"}}}}
        result = {"status": "catalog_source_mutation_draft", "draft": {"operation": "exact-operation"}}
        clock = [0.0]
        with patch.object(CLIENTS.time, "monotonic", side_effect=lambda: clock[0]), \
                patch.object(CLIENTS.time, "sleep", side_effect=lambda delay: clock.__setitem__(0, clock[0] + delay)):
            transport = unittest.mock.Mock(side_effect=[{"status": "failure", "code": "registry_busy"}, result])
            self.assertEqual(CLIENTS.metadata_rpc(transport, method, deadline=1.0), result)
            self.assertEqual(transport.call_count, 2)
            self.assertEqual(transport.call_args_list[0], transport.call_args_list[1])
            self.assertEqual(transport.call_args.args, (json.dumps(method), 1.0))
        for refused in ({"status": "failure", "code": "invalid_registry_request"},
                        {"status": "failure", "code": "registry_busy", "unexpected": True}):
            with self.subTest(refused=refused):
                transport = unittest.mock.Mock(return_value=refused)
                with self.assertRaises(AssertionError):
                    CLIENTS.metadata_rpc(transport, method)
                transport.assert_called_once()

    def test_source_review_observations_retry_only_exact_native_contention(self):
        for name in ('catalog_source_mutation_draft', 'catalog_host_context', 'catalog_page',
                     'catalog_grant_draft', 'catalog_source_change_observation', 'catalog_receipt',
                     'catalog_index_retirement_evidence'):
            method = {name: {'key': 'original-key', 'index_id': 'original-index'}}
            result = {'status': name, 'complete': {'native': ['all', 'fields']}}
            with self.subTest(name=name), patch.object(CLIENTS.time, 'monotonic', return_value=0), \
                    patch.object(CLIENTS.time, 'sleep'):
                transport = unittest.mock.Mock(side_effect=[{'status':'failure','code':'registry_busy'},result])
                self.assertEqual(CLIENTS.metadata_rpc(transport, method, deadline=1.0), result)
                self.assertEqual(transport.call_count, 2)
                self.assertEqual(transport.call_args_list[0], transport.call_args_list[1])
                self.assertEqual(transport.call_args.args, (json.dumps(method), 1.0))
                for refused in ({'status':'failure','code':'invalid_registry_request'},
                                {'status':'failure','code':'registry_busy','unexpected':True}):
                    transport = unittest.mock.Mock(return_value=refused)
                    with self.assertRaises(AssertionError): CLIENTS.metadata_rpc(transport, method, deadline=1.0)
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
            (root / "airk-runtime.json").write_text('{"schema":1}', encoding="utf-8")
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
        fixture.single_image = False
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
