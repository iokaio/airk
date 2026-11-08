# SPDX-License-Identifier: Apache-2.0
"""Owned producer/credential contracts and real isolated TLS; no AIRK execution."""
import copy
import hashlib
import importlib.util
import importlib.machinery
import json
from pathlib import Path
import tempfile
import shutil
from types import SimpleNamespace
import unittest
from unittest import mock

import test_http_fixture as TLS


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).parents[1] / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SERVICE = load("service_http_benchmark_test", "benchmark-service.py")
INVESTIGATION = load("owned_benchmark_test", "benchmark-mcp-investigation.py")
VERIFICATION = load("verification_benchmark_test", "benchmark-source-verification.py")


class ProducerTests(unittest.TestCase):
    def test_owned_benchmark_preparation_does_not_precreate_native_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checked = []
            def boundary(runtime, owner):
                self.assertFalse((owner / "state").exists())
                self.assertTrue((owner / "index-data").is_dir())
                checked.append(owner)
                raise RuntimeError("verified native state boundary")
            original = importlib.machinery.SourceFileLoader.exec_module
            def load_fixture(loader, module):
                original(loader, module)
                if module.__name__ == "investigation_clients":
                    module.Fixture = boundary
                    module.owned_client_fixture = tempfile.TemporaryDirectory
            with mock.patch.object(importlib.machinery.SourceFileLoader, "exec_module", load_fixture):
                with self.assertRaisesRegex(RuntimeError, "verified native state"):
                    INVESTIGATION.owned(SimpleNamespace(runtime_dir=root, output=root / "output"))
            self.assertEqual(len(checked), 1)

    def test_scoped_source_work_keeps_privacy_and_full_nonclock_management_identity(self):
        project = "owned"
        progress = {"status": "progress", "project": project, "pending_job": None, "queued_jobs": [],
                    "observation": None, "queue_truncated": False, "project_paused": False, "observed_unix_ms": 1}
        watch = {"status": "watch", "project": project, "observation": {"enabled": False, "failure": None,
                 "automatic_retries_exhausted": False, "automatic_retry_canceled": False,
                 "requested_revision": 2, "completed_revision": 2, "observed_unix_ms": 1, "future": {"retained": True}}}
        service = SimpleNamespace(projects={"verification": {"binding": project}}, rpc=mock.Mock(side_effect=[progress, watch]))
        clients = SimpleNamespace(assert_scoped_freshness_private=mock.Mock())
        settled, stable, raw = VERIFICATION.scoped_source_work(clients, service, {"freshness": "opaque"}, 123)
        self.assertTrue(settled)
        clients.assert_scoped_freshness_private.assert_called_once_with({"freshness": "opaque"})
        self.assertEqual(raw, {"progress": progress, "watch": watch})
        changed = copy.deepcopy(watch); changed["observation"]["observed_unix_ms"] = 2
        service.rpc = mock.Mock(side_effect=[{**progress, "observed_unix_ms": 2}, changed])
        self.assertEqual(VERIFICATION.scoped_source_work(clients, service, {}, 123)[1], stable)
        changed["observation"]["future"] = {}
        service.rpc = mock.Mock(side_effect=[progress, changed])
        self.assertNotEqual(VERIFICATION.scoped_source_work(clients, service, {}, 123)[1], stable)
        for field, value in (("pending_job", {"project": project}), ("queue_truncated", True), ("project_paused", True)):
            service.rpc = mock.Mock(side_effect=[{**progress, field: value}, watch])
            self.assertFalse(VERIFICATION.scoped_source_work(clients, service, {}, 123)[0])
        for changed_progress, changed_watch in (({**progress, "project": "foreign"}, watch),
                (progress, {**watch, "project": "foreign"}),
                (progress, {**watch, "observation": {**watch["observation"], "enabled": True}}),
                (progress, {**watch, "observation": {**watch["observation"], "completed_revision": 3}})):
            service.rpc = mock.Mock(side_effect=[changed_progress, changed_watch])
            with self.assertRaises(AssertionError):
                VERIFICATION.scoped_source_work(clients, service, {}, 123)

    def test_source_verification_preparation_keeps_native_state_separate(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary)
            (runtime / "airk-runtime.json").write_text(json.dumps({"schema": 2, "version": "1.8.14"}))
            (runtime / "airk-build.json").write_text(json.dumps({"source": {"fixture": True}}))
            checked = []
            def boundary(selected_runtime, owner):
                self.assertFalse((owner / "state").exists())
                self.assertTrue((owner / "index-data").is_dir())
                checked.append(owner)
                raise RuntimeError("verified_state_boundary")
            clients = SimpleNamespace(runtime_root=lambda _: runtime, owned_client_fixture=tempfile.TemporaryDirectory, Fixture=boundary)
            builder = SimpleNamespace(source_identity=lambda: {"fixture": True}, digest=lambda _: "fixture-digest")
            with mock.patch.object(VERIFICATION, "load", side_effect=[clients, SimpleNamespace(), builder]), \
                    mock.patch.object(VERIFICATION, "verify_payload", return_value={}):
                self.assertFalse(VERIFICATION.run(SimpleNamespace(runtime_dir=runtime, output=runtime / "output", samples=1)))
            self.assertEqual(len(checked), 1)

    def test_only_exact_known_http_admission_refusal_is_counted_without_retry(self):
        body = {"schema": "airk.local-api-error/1", "error": {"code": "query_admission_busy"}, "future": {"retained": True}}
        session = SimpleNamespace(request=mock.Mock(side_effect=SERVICE.http_fixture.HttpRefused(429, body)))
        result = SERVICE.request_attempt(session, "search", {"query": "original"})
        self.assertEqual(result["http_status"], 429)
        self.assertEqual(result["body"], body)
        self.assertGreaterEqual(result["rejected_ms"], 0)
        session.request.assert_called_once()
        for error in [SERVICE.http_fixture.HttpRefused(403, body),
                      SERVICE.http_fixture.HttpRefused(429, {"error": {"code": "query_admission_busy"}}),
                      SERVICE.http_fixture.HttpRefused(429, {**body, "error": {"code": "unknown"}}),
                      ConnectionResetError("uncertain exchange")]:
            session.request = mock.Mock(side_effect=error)
            with self.assertRaises(type(error)):
                SERVICE.request_attempt(session, "search", {})
            session.request.assert_called_once()

    def test_unknown_native_stop_retains_the_exact_owned_install_and_state(self):
        owners = []
        class FailingNativeFixture:
            def __init__(self, runtime, owner, **_):
                self.owner = owner
                owners.append(owner)
            def __enter__(self):
                (self.owner / "unresolved-native-state").write_bytes(b"retain")
                return self
            def __exit__(self, *_):
                raise RuntimeError("native_stop_uncertain")
            def rpc(self, _):
                raise RuntimeError("observation_failed")
        try:
            with mock.patch.object(SERVICE.clients, "Fixture", FailingNativeFixture):
                with self.assertRaisesRegex(RuntimeError, "native_stop_uncertain"):
                    SERVICE.campaign(Path("unused-mocked-runtime"), 0, 0, 1)
            self.assertEqual(len(owners), 1)
            self.assertEqual((owners[0] / "unresolved-native-state").read_bytes(), b"retain")
        finally:
            for owner in owners:
                self.assertEqual(owner.parent.resolve(), Path(tempfile.gettempdir()).resolve())
                self.assertTrue(owner.name.startswith("airk-service-clients-"))
                self.assertFalse(owner.is_symlink())
                shutil.rmtree(owner)

    def test_owned_modern_configuration_has_exact_scope_control_and_no_launch_args(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            binary, control = root / "airk", root / "control"
            binary.write_bytes(b"approved-main"); control.mkdir()
            (control / "http-1").mkdir()
            (root / "airk-runtime.json").write_text(json.dumps({"version": "1.8.14"}))
            selected = {"namespace": "native", "grant_id": "frozen", "expected_catalog_revision": 4}
            service = SimpleNamespace(single_image=True, binary=binary, control=control,
                projects={"owned": {"root": str(root)}}, scopes={"owned": selected},
                enroll_http=mock.Mock(return_value={"native": "exact-enrolled-metadata"}),
                arguments=mock.Mock(side_effect=AssertionError("Modern config must not acquire a stdio launch")))
            result = INVESTIGATION.owned_configuration(service, "owned", root)
            self.assertEqual(result["schema"], 2); self.assertEqual(result["transport"], "https")
            self.assertEqual(result["args"], [])
            self.assertEqual(result["control"], str(control)); self.assertEqual(result["source_root"], str(root))
            self.assertEqual(result["selection"], {"kind": "scoped", "selection": selected})
            service.enroll_http.assert_called_once_with({"kind": "scoped", "selection": selected}, mock.ANY, "unknown")
            roster = json.loads(Path(result["connections_file"]).read_bytes())
            self.assertEqual(roster, {"schema": 1, "client": "unknown", "connections": [{"native": "exact-enrolled-metadata"}]})
            result["selection"]["selection"]["grant_id"] = "changed"
            self.assertEqual(selected["grant_id"], "frozen")
            service.single_image = False
            service.arguments = lambda _: ["--root", str(root)]
            (root / "airk-runtime.json").write_text(json.dumps({"version": "1.8.12"}))
            legacy = INVESTIGATION.owned_configuration(service, "owned", root)
            self.assertEqual(legacy["args"], ["--root", str(root), "mcp"])
            self.assertNotIn("transport", legacy)

    def test_sixty_four_clients_enroll_distinct_credentials_with_exact_immutable_authority(self):
        selected = {"namespace": "native", "grant_id": "same-reviewed-grant", "expected_catalog_revision": 1}
        methods = []
        health = {"instance": "service", "user": "user", "executable_sha256": "execution", "catalog_clock": {"namespace": "native"}}
        def rpc(method):
            self.assertEqual(method, "health", "Console must not perform private HTTP enrollment")
            return {"status": "health", "observation": health}
        def enroll_http(selection, identity, client):
            methods.append((copy.deepcopy(selection), identity, client))
            return {"connection_id": identity, "selection": copy.deepcopy(selection),
                "endpoint": {"service_instance": "service", "user": "user", "namespace": "native", "executable_sha256": "execution"}}
        fixture = SimpleNamespace(scopes={"owned": selected}, rpc=rpc, enroll_http=enroll_http, managed_health=health)
        connections = [SERVICE.enroll(fixture, "owned") for _ in range(64)]
        self.assertEqual(len({info["connection_id"] for info in connections}), 64)
        self.assertEqual(len(methods), 64)
        self.assertTrue(all(selection == {"kind": "scoped", "selection": selected} and client == "unknown"
                            for selection, _, client in methods))
        self.assertEqual(selected["expected_catalog_revision"], 1)

    def test_payload_verification_never_hashes_a_modern_helper_but_requires_legacy_helper(self):
        binary = Path("/owned/airk")
        modern = {"schema": 2, "version": "1.8.14", "os": "linux", "arch": "x86_64", "airk_sha256": "main"}
        builder = SimpleNamespace(digest=mock.Mock(return_value="main"))
        result = VERIFICATION.verify_payload(builder, binary, modern, {"runtime": modern})
        self.assertIsNone(result["helper_sha256"])
        self.assertEqual(result["transport"], "https")
        builder.digest.assert_called_once_with(binary)
        legacy = {**modern, "schema": 1, "version": "1.8.12", "embed_sha256": "helper"}
        builder.digest = mock.Mock(side_effect=["main", "helper"])
        self.assertEqual(VERIFICATION.verify_payload(builder, binary, legacy, {"runtime": legacy})["helper_sha256"], "helper")
        self.assertEqual(builder.digest.call_args_list[-1].args, (binary.with_name("airk-embed"),))
        for malformed in [{**modern, "embed_sha256": None}, {**modern, "schema": 1}]:
            with self.assertRaises(ValueError):
                VERIFICATION.verify_payload(builder, binary, malformed, {"runtime": malformed})


class ActualHttpsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        TLS.ActualHttpsFixtureTests.setUpClass()

    @classmethod
    def tearDownClass(cls):
        TLS.ActualHttpsFixtureTests.tearDownClass()

    def test_initialized_real_tls_session_retains_only_hashed_session_identity_and_deletes(self):
        fixture = TLS.ActualHttpsFixtureTests()
        with fixture.server() as (connection, observed):
            session = SERVICE.HttpSession(connection, fixture.root)
            try:
                self.assertEqual(session.session_sha256, hashlib.sha256(session.network.session.encode()).hexdigest())
                self.assertEqual(len(session.session_sha256), 64)
                self.assertTrue(session.request("ping", {})["fixture"])
            finally:
                session.close()
            session.close()
            self.assertTrue(session.closed)
            self.assertEqual(sum(method == "initialize" for method, _ in observed), 1)
            self.assertEqual(sum(method == "DELETE" for method, _ in observed), 1)


class SemanticLoadContracts(unittest.TestCase):
    pipeline = "ab" * 32

    def result(self):
        return {"execution":{"requested_mode":"vector","served_mode":"vector","incomplete":False},
                "semantic_readiness":{"state":"ready","completed_artifacts":16,"selected_artifacts":16,
                                      "pipeline_digest":self.pipeline}}

    def owner(self, loads=1):
        return {"embedding_status":{"available":True,"process_id":None,"loads":loads,
                "provider":{"schema":1,"kind":"in_process","admission_open":True,
                            "provider":{"service_instance":"owned","factory_generation":"same"}}}}

    def test_semantic_arguments_preserve_full_supported_inputs_and_lexical_default(self):
        lexical = SERVICE.search_arguments(3)
        semantic = SERVICE.search_arguments(3, True)
        self.assertEqual(lexical["mode"], "lexical")
        self.assertEqual(semantic, {**lexical,"mode":"vector"})
        self.assertEqual(semantic["response_mode"], "full")

    def test_no_fallback_or_pipeline_substitution_even_during_partial_semantic_coverage(self):
        self.assertTrue(SERVICE.semantic_result(self.result(), self.pipeline))
        for mutation in ("fallback","pipeline","missing_execution","overreported"):
            value = self.result()
            if mutation == "fallback": value["execution"]["served_mode"] = "lexical"
            elif mutation == "pipeline": value["semantic_readiness"]["pipeline_digest"] = "cd"*32
            elif mutation == "missing_execution": del value["execution"]
            else: value["semantic_readiness"]["completed_artifacts"] = 17
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                SERVICE.semantic_result(value, self.pipeline, require_ready=False)
        partial = self.result()
        partial["execution"]["incomplete"] = True
        partial["semantic_readiness"].update(state="partial",completed_artifacts=8)
        self.assertFalse(SERVICE.semantic_result(partial,self.pipeline,require_ready=False))
        with self.assertRaises(AssertionError):
            SERVICE.semantic_result(partial,self.pipeline)

    def test_shared_owner_load_count_is_not_client_count_or_a_global_process_absence_claim(self):
        baseline = self.owner()
        SERVICE.stable_semantic_owner(baseline, self.owner())
        for changed in (self.owner(64), self.owner(0)):
            with self.assertRaises(AssertionError):
                SERVICE.stable_semantic_owner(baseline, changed)
        changed = self.owner(); changed["embedding_status"]["provider"]["provider"]["factory_generation"] = "replacement"
        with self.assertRaises(AssertionError):
            SERVICE.stable_semantic_owner(baseline, changed)
        changed = self.owner(); changed["embedding_status"]["process_id"] = 123
        with self.assertRaises(AssertionError):
            SERVICE.semantic_provider(changed)
        changed = self.owner(); changed["embedding_status"]["provider"]["admission_open"] = False
        with self.assertRaises(AssertionError):
            SERVICE.semantic_provider(changed)

    def test_semantic_pipeline_comes_from_the_approved_embedded_cpu_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "airk-runtime.json").write_text('{"schema":2}')
            smoke = {"source":"embedded","engine":"onnxruntime-cpu/1.22.1","dimensions":384,
                     "repeat_equal":True,"profile":"minilm-l6-v2-u8-v1","pipeline_digest":self.pipeline,"legacy_identity":"ef"*32}
            (root / "airk-build.json").write_text(json.dumps({"embedded_model_smoke":smoke}))
            self.assertEqual(SERVICE.semantic_pipeline(root),self.pipeline)
            for key, value in (("source","ambient"),("profile","foreign"),("pipeline_digest","invalid"),("repeat_equal",False)):
                (root / "airk-build.json").write_text(json.dumps({"embedded_model_smoke":{**smoke,key:value}}))
                with self.subTest(key=key), self.assertRaises(AssertionError):
                    SERVICE.semantic_pipeline(root)
            (root / "airk-runtime.json").write_text('{"schema":1}')
            with self.assertRaises(AssertionError):
                SERVICE.semantic_pipeline(root)

    def test_setup_submits_each_model_and_vector_owner_job_once_and_waits_its_exact_receipt(self):
        fixture = SimpleNamespace(projects={"first":{"binding":"a"},"second":{"binding":"b"}})
        calls = []
        def job(name, command):
            calls.append((name,command))
            return {"id":name+command,"project":fixture.projects[name]["binding"],"state":"succeeded"}
        fixture.job = job
        fixture.configure = lambda name, draft: job(name, "vectors_enabled")
        fixture.enqueue = lambda name, kind: {"job":job(name,kind)}
        fixture.wait = lambda receipt: receipt["job"]
        fixture.published_status = lambda name, deadline: self.result()
        with mock.patch.object(SERVICE,"snapshot",side_effect=[self.owner(0),self.owner(1)]):
            result = SERVICE.prepare_semantic(fixture,["first","second"],self.pipeline)
        self.assertEqual(calls,[(name,command) for name in ("first","second") for command in ("setup-model","vectors_enabled","vectors")])
        self.assertEqual(result["model_load_delta"],1)
        self.assertEqual(len(result["jobs"]),2)
        for row in result["jobs"]:
            self.assertEqual(row["vectors_accepted"]["job"]["id"],row["vectors_completed"]["id"])
        with mock.patch.object(SERVICE,"snapshot",side_effect=[self.owner(1),self.owner(1)]):
            warmed = SERVICE.prepare_semantic(fixture,["first","second"],self.pipeline)
        self.assertEqual(warmed["model_load_delta"],0,"Already loaded reviewed MiniLM must not be loaded again for measurement")
        with mock.patch.object(SERVICE,"snapshot",side_effect=[self.owner(1),self.owner(2)]):
            with self.assertRaises(AssertionError):
                SERVICE.prepare_semantic(fixture,["first","second"],self.pipeline)


if __name__ == "__main__":
    unittest.main()
