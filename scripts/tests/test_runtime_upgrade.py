# SPDX-License-Identifier: Apache-2.0
"""Upgrade fixture retention and exact cleanup ordering; no native runtime is launched."""
import importlib.util
from contextlib import closing
from pathlib import Path
import tempfile
import json
import hashlib
import sqlite3
import time
import subprocess
import signal
import copy
from types import SimpleNamespace
from unittest import mock
import unittest

SPEC = importlib.util.spec_from_file_location("runtime_upgrade", Path(__file__).parents[1] / "check-runtime-upgrade.py")
UPGRADE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(UPGRADE)

class UpgradeFixtureOwnershipTests(unittest.TestCase):
    def test_pre_candidate_cleanup_requires_exact_old_owner_and_retains_stop_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); binary = root/"old.exe"; binary.write_bytes(b"inert exact old image")
            install = root/"install"; install.mkdir()
            execution = UPGRADE.digest(binary)
            health = {"status":"health","observation":{"runtime_sha256":"a"*64,"executable_sha256":execution,"instance":"b"*32,"process_id":123}}
            command = mock.Mock(side_effect=[SimpleNamespace(returncode=0,stdout=json.dumps(health)),SimpleNamespace(returncode=0,stdout=json.dumps({"phase":"complete"}))])
            result = UPGRADE.stop_predecessor_before_staging(command,binary,install,root/"control","a"*64,execution)
            self.assertEqual(result["native_stop"],{"phase":"complete"})
            self.assertFalse(result["process_exit_confirmed"])
            self.assertEqual(command.call_args_list[1].args,(binary,"runtime","stop","--install-dir",install))
            for field,value in (("runtime_sha256","c"*64),("executable_sha256","c"*64),("instance","invalid"),("process_id",False)):
                crossed = copy.deepcopy(health); crossed["observation"][field] = value
                command = mock.Mock(return_value=SimpleNamespace(returncode=0,stdout=json.dumps(crossed)))
                with self.subTest(field=field), self.assertRaises(RuntimeError):
                    UPGRADE.stop_predecessor_before_staging(command,binary,install,root/"control","a"*64,execution)
                self.assertEqual(command.call_count,1)
            (install/"activation.json").write_text("pending exact owner")
            command = mock.Mock()
            with self.assertRaises(RuntimeError):
                UPGRADE.stop_predecessor_before_staging(command,binary,install,root/"control","a"*64,execution)
            command.assert_not_called()

    def test_publication_summary_and_expansion_require_the_same_validated_identity(self):
        token = "airk-r1:"+"1"*32+":4:"+"2"*64
        summary = {"token":token}
        expanded = {"token":token,"incarnation":"1"*32,"generation":4,"revision":"2"*64,"collection":"upgrade"}
        self.assertEqual(UPGRADE.publication_identity(summary,4,"upgrade"),UPGRADE.publication_identity(expanded,4,"upgrade"))
        for field,value in (("incarnation","3"*32),("revision","3"*64),("generation",5),("generation",True),("collection","foreign"),("token","arbitrary")):
            with self.subTest(field=field,value=value), self.assertRaises(RuntimeError):
                UPGRADE.publication_identity({**expanded,field:value},4,"upgrade")
        with self.assertRaises(RuntimeError):
            UPGRADE.publication_identity(summary,5,"upgrade")

    def test_scoped_semantic_identity_requires_both_actual_publication_bound_queries(self):
        status = {"publication":{"token":"airk-r1:"+"1"*32+":4:"+"2"*64},"generation":4,"indexed_vectors":1,
                  "semantic_readiness":{"state":"ready","selected_artifacts":1,"completed_artifacts":1}}
        original_status = copy.deepcopy(status)
        model = {"reference":{"pipeline_digest":"a"*64,"legacy_identity":"b"*64},"profile":"retained-model"}
        queries = {mode:{"publication":status["publication"],"generation":4,
                    "semantic_readiness":{**status["semantic_readiness"],"pipeline_digest":"a"*64,"selection_digest":"c"*64},
                    "execution":{"requested_mode":mode,"served_mode":mode,"incomplete":False}}
                   for mode in ("vector","hybrid")}
        queries["vector"]["publication"] = {**status["publication"],"generation":4,"incarnation":"1"*32,"revision":"2"*64,"collection":"upgrade"}
        identity = UPGRADE.semantic_identity(status,model,queries)
        self.assertEqual(identity["pipeline_digest"],"a"*64)
        self.assertEqual(identity["selection_digest"],"c"*64)
        self.assertEqual(identity["identity_provenance"],"actual_vector_and_hybrid_publication_reads")
        self.assertEqual(status,original_status,"scoped status fields must remain unchanged")
        with self.assertRaises(RuntimeError):
            UPGRADE.semantic_identity(status,model,{"vector":queries["vector"]})
        changes = [("publication",{"token":"other"}),("generation",5),
                   ("semantic_readiness",{**queries["hybrid"]["semantic_readiness"],"pipeline_digest":"d"*64}),
                   ("semantic_readiness",{**queries["hybrid"]["semantic_readiness"],"selection_digest":"d"*64}),
                   ("semantic_readiness",{**queries["hybrid"]["semantic_readiness"],"completed_artifacts":0}),
                   ("execution",{"requested_mode":"hybrid","served_mode":"lexical","incomplete":False})]
        for field,value in changes:
            altered = copy.deepcopy(queries)
            altered["hybrid"][field] = value
            with self.subTest(field=field,value=value), self.assertRaises(RuntimeError):
                UPGRADE.semantic_identity(status,model,altered)
        with self.assertRaises(RuntimeError):
            UPGRADE.semantic_identity({**status,"semantic_readiness":{**status["semantic_readiness"],"pipeline_digest":"d"*64}},model,queries)

    def test_reviewed_https_reconnect_uses_public_native_enrollment_once(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            control = root / "control"
            private = control / "http-1"
            private.mkdir(parents=True, mode=0o700)
            spec = importlib.util.spec_from_file_location("upgrade_enrollment_contract", Path(__file__).parents[1] / "http_fixture.py")
            helper = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(helper)
            selected = {"namespace":"a" * 32,"expected_catalog_revision":4,"index_id":"idx_original"}
            submissions = []
            def invoke(_binary, *args, input=None, timeout=180):
                if args[:2] == ("runtime", "http-connection"):
                    self.assertGreater(timeout, 0); self.assertLessEqual(timeout, 60)
                    source = Path(args[args.index("--selection-file") + 1])
                    self.assertEqual(json.loads(source.read_text()), {"kind":"scoped","selection":selected})
                    self.assertEqual(args[args.index("--client") + 1], "unknown")
                    identifier = args[args.index("--connection-id") + 1]
                    submissions.append(identifier)
                    token, ca = private / ("credential-" + identifier + ".token"), private / "ca.pem"
                    token.write_text("owned fixture reference"); ca.write_text("owned fixture CA reference")
                    return {"schema":1,"connection_id":identifier,"selection":{"kind":"scoped","selection":selected},
                            "credential_file":str(token),"ca_certificate_file":str(ca)}
                self.assertEqual(args[:2], ("service", "rpc"))
                self.assertEqual(json.loads(input), "health", "Console must not perform private HTTP enrollment")
                return {"observation":{"instance":"same owned service"}}
            transport = SimpleNamespace(enroll_native=helper.enroll_native,
                validate_native_identity=mock.Mock(side_effect=RuntimeError("stop after native enrollment boundary")))
            loader = SimpleNamespace(loader=SimpleNamespace(exec_module=lambda module:None))
            with mock.patch.object(UPGRADE.importlib.util,"spec_from_file_location",return_value=loader), \
                    mock.patch.object(UPGRADE.importlib.util,"module_from_spec",return_value=transport):
                with self.assertRaisesRegex(RuntimeError,"native enrollment boundary"):
                    UPGRADE.verify_https_reconnect(invoke, root / "airk", root / "install", control, root,
                        {"binding":"original"}, selected, "original-config-hash", b"saved source")
            self.assertEqual(len(submissions), 1)
            self.assertEqual(list(private.glob("fixture-selection-*.json")), [])

    def test_darwin_owned_exit_waits_for_exact_native_image_retirement_not_signal_causality(self):
        with tempfile.TemporaryDirectory() as temporary:
            launcher = Path(temporary).resolve() / "airk-launcher"
            launcher.write_bytes(b"fictional predecessor executable bytes")
            image = UPGRADE.capture_client_image(launcher)
            self.assertEqual(image["sha256"], hashlib.sha256(launcher.read_bytes()).hexdigest())
            row = {"path":image["path"], "sha256":image["sha256"]}
            cleanup = {"launchers":{"schema":1, "legacy_launchers_retired":True,
                "launcher":None, "universal_launcher":None, "processes":{"scope":"verified_launcher_images", "remaining":0,
                "image_roster":[row], "observations":[{**row,"observation":{"complete":True,"failure":None,
                    "matching_executable_pids":[],"unobserved":0}}]}}}
            for status in (0, -UPGRADE.DARWIN_SIGKILL):
                pending = UPGRADE.darwin_client_exit(123, status, image)
                self.assertFalse(pending["qualified"])
                self.assertFalse(pending["signal_causality_proven"])
                # Native cleanup deleted the exact original executable; retain
                # its captured full digest rather than treating absence as proof.
                if launcher.exists(): launcher.unlink()
                observed = UPGRADE.darwin_client_exit(123, status, image, cleanup)
                self.assertTrue(observed["qualified"])
                self.assertFalse(observed["held_kernel_census"])
                self.assertFalse(observed["signal_causality_proven"])
            for invalid in (None, True, 1, -signal.SIGTERM):
                with self.subTest(status=invalid), self.assertRaisesRegex(RuntimeError,"accepted exact owned exit"):
                    UPGRADE.darwin_client_exit(123, invalid, image, cleanup)
            for fault in ("path", "hash", "duplicate", "absent", "incomplete", "remaining", "unobserved", "failure"):
                bad = copy.deepcopy(cleanup); processes = bad["launchers"]["processes"]
                if fault == "path": processes["image_roster"][0]["path"] = str(Path(image["path"]).with_name("foreign"))
                elif fault == "hash": processes["observations"][0]["sha256"] = "0"*64
                elif fault == "duplicate": processes["image_roster"] *= 2
                elif fault == "absent": processes["image_roster"] = []
                elif fault == "incomplete": processes["observations"][0]["observation"]["complete"] = False
                elif fault == "remaining": processes["observations"][0]["observation"]["matching_executable_pids"] = [123]
                elif fault == "unobserved": processes["observations"][0]["observation"]["unobserved"] = 1
                elif fault == "failure": processes["observations"][0]["observation"]["failure"] = "denied"
                with self.subTest(fault=fault), self.assertRaisesRegex(RuntimeError,"Darwin predecessor"):
                    UPGRADE.darwin_client_exit(123, -UPGRADE.DARWIN_SIGKILL, image, bad)

    def test_native_cleanup_cannot_pass_with_remaining_payload_or_incomplete_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            install = Path(temporary)
            active, old = "a" * 64, "b" * 64
            (install / "runtimes" / active).mkdir(parents=True)
            process_fields = {"remaining": 0, "observations": [], "image_roster": [], "image_exclusions": []}
            launcher = {"schema": 1, "active": active, "legacy_launchers_retired": True,
                        "launcher": None, "universal_launcher": None,
                        "processes": {**process_fields, "scope": "verified_launcher_images"}}
            retired = {"active": active, "complete": True, "selected_service_restarted": False,
                       "retired_runtimes": [old],
                       "processes": {**process_fields, "scope": "verified_superseded_runtime_images"}}
            result = UPGRADE.retire_obsolete_runtime(mock.Mock(side_effect=[launcher, retired]),
                                                    "airk", install, active, old)
            self.assertEqual(result["runtimes"], retired)
            leftover = install / "runtimes" / old
            leftover.mkdir()
            with self.assertRaisesRegex(RuntimeError, "Obsolete runtime payloads remain"):
                UPGRADE.retire_obsolete_runtime(mock.Mock(side_effect=[launcher, retired]),
                                               "airk", install, active, old)
            leftover.rmdir()
            for field, value in (("complete", False), ("selected_service_restarted", True),
                                 ("retired_runtimes", []), ("retired_runtimes", old),
                                 ("retired_runtimes", [old, old]), ("retired_runtimes", [old, active]),
                                 ("processes", {**retired["processes"], "remaining": False}),
                                 ("processes", {"remaining": 1})):
                with self.subTest(field=field), self.assertRaisesRegex(RuntimeError, "retirement is incomplete"):
                    UPGRADE.retire_obsolete_runtime(mock.Mock(side_effect=[launcher, {**retired, field: value}]),
                                                   "airk", install, active, old)

    def test_registry_migration_distinguishes_modern_inference_contract_and_rejects_downgrade(self):
        for version in (12, 13):
            UPGRADE.verify_registry_version(version, 12, 2)
        UPGRADE.verify_registry_version(12, 11, 1)
        for version, previous, schema in ((13, 12, 1), (12, 13, 2), (14, 12, 2),
                                          (11, 11, 2), (12, 12, 3)):
            with self.subTest(version=version, previous=previous, schema=schema):
                with self.assertRaisesRegex(RuntimeError, "Upgraded registry schema"):
                    UPGRADE.verify_registry_version(version, previous, schema)

    def test_darwin_runtime_resolution_keeps_main_in_its_bundle(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            main = root / "AIRK.app/Contents/MacOS/airk"
            main.parent.mkdir(parents=True); main.write_bytes(b"inert signed-layout fixture")
            (root/"airk").write_bytes(b"must not select an unsealed main copy")
            self.assertEqual(UPGRADE.runtime_binary(root,"darwin"),main)
            main.unlink()
            with self.assertRaisesRegex(RuntimeError,"sealed runtime main"):
                UPGRADE.runtime_binary(root,"darwin")

    def test_semantic_queries_refuse_lexical_fallback_and_changed_pipeline_or_source(self):
        original = b"original retained source\n"
        publication = {"token":"airk-r1:"+"1"*32+":4:"+"2"*64}
        body = {"execution":{"requested_mode":"vector","served_mode":"vector","incomplete":False},"publication":publication,
                "semantic_readiness":{"pipeline_digest":"a"*64},"hits":[{"citation":{"path":"README.md","content_hash":hashlib.sha256(original).hexdigest()},"text":original.decode()}]}
        UPGRADE.assert_semantic_query(body,"vector",publication,"a"*64,original)
        for field,value in (("execution",{"requested_mode":"vector","served_mode":"lexical","incomplete":False}),
                            ("execution",{"requested_mode":"vector","served_mode":"vector","incomplete":True}),
                            ("semantic_readiness",{"pipeline_digest":"b"*64}), ("publication",{"token":"new"}), ("hits",[])):
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                UPGRADE.assert_semantic_query({**body,field:value},"vector",publication,"a"*64,original)

    def test_old_semantic_jobs_only_observe_the_original_accepted_identity(self):
        job = {"id":"a"*64,"project":"b"*64,"kind":"vectors","state":"pending"}
        rpc = mock.Mock(side_effect=[{"job":{**job,"state":"running"}},{"job":{**job,"state":"succeeded"}}])
        with mock.patch.object(UPGRADE.time,"sleep"):
            observed = UPGRADE.wait_semantic_job(rpc,{"status":"job","job":job},job["project"],"vectors")
        self.assertEqual(observed["completed"]["id"],job["id"])
        self.assertEqual(rpc.call_args_list,[mock.call({"job":{"id":job["id"]}})]*2)
        for changed in ({"id":"c"*64,"state":"succeeded"},{"state":"failed"}):
            rpc = mock.Mock(return_value={"job":{**job,**changed}})
            with self.assertRaises(RuntimeError):
                UPGRADE.wait_semantic_job(rpc,{"status":"job","job":job},job["project"],"vectors")
            self.assertEqual(rpc.call_count,1)

    def test_semantic_job_snapshot_detects_new_vector_work_while_retaining_full_outcomes(self):
        with tempfile.TemporaryDirectory() as temporary:
            database = Path(temporary).resolve() / "jobs.sqlite3"
            with closing(sqlite3.connect(database)) as sql, sql:
                sql.execute("CREATE TABLE service_jobs(seq INTEGER,id TEXT,project TEXT,kind TEXT,state TEXT,epoch INTEGER,attempts INTEGER,outcome BLOB)")
                sql.execute("INSERT INTO service_jobs VALUES (1,'saved','owned','vectors','succeeded',7,1,?)",(json.dumps({"future":{"retained":True}}).encode(),))
            before = UPGRADE.semantic_jobs(database,"owned")
            self.assertEqual(before[0]["outcome"],{"future":{"retained":True}})
            with closing(sqlite3.connect(database)) as sql, sql:
                sql.execute("INSERT INTO service_jobs VALUES (2,'unrequested','owned','index','pending',8,0,NULL)")
            self.assertNotEqual(UPGRADE.semantic_jobs(database,"owned"),before)
            self.assertEqual(UPGRADE.semantic_jobs(database,"other"),[])

    def test_model_reference_cannot_claim_minilm_using_different_actual_assets(self):
        catalog = json.loads((Path(__file__).parents[2] / "crates/airk-retrieval/src/model_catalog.json").read_bytes())
        with tempfile.TemporaryDirectory() as temporary:
            control = Path(temporary)
            root = control / "model-files-1.4"; model = root / "sets" / ("a"*64)
            model.mkdir(parents=True)
            selected = {"shared":{"set":"a"*64},"legacy_identity":"b"*64,"pipeline_digest":"c"*64,"assets":{}}
            contents = {"model.json":json.dumps(catalog["profiles"][0]["manifest"]).encode(),"model.onnx":b"not the approved weights","tokenizer.json":b"{}","LICENSE":b"fictional test license"}
            for name,content in contents.items():
                (model/name).write_bytes(content)
                selected["assets"][name] = {"bytes":len(content),"sha256":hashlib.sha256(content).hexdigest()}
            (root / "default-fixture.json").write_text(json.dumps(selected))
            with self.assertRaisesRegex(RuntimeError,"assets do not match"):
                UPGRADE.retained_model(control)

    def test_owned_git_rules_preserve_local_corpus_and_refuse_changed_config(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            local = root / ".gitignore"
            local.write_text("private-fixture.txt\n")
            original = dict(UPGRADE.os.environ)
            environment = UPGRADE.isolated_git_environment(root)
            self.assertEqual(dict(UPGRADE.os.environ),original)
            self.assertTrue(Path(environment["GIT_CONFIG_GLOBAL"]).is_relative_to(root))
            self.assertEqual(environment["GIT_CONFIG_NOSYSTEM"],"1")
            self.assertEqual(local.read_text(),"private-fixture.txt\n")
            self.assertEqual(UPGRADE.isolated_git_environment(root),environment)
            config = Path(environment["GIT_CONFIG_GLOBAL"])
            config.write_text("unexpected preexisting rules")
            with self.assertRaisesRegex(RuntimeError,"changed"):
                UPGRADE.isolated_git_environment(root)
            self.assertEqual(config.read_text(),"unexpected preexisting rules")

    def test_https_reconnect_retries_only_typed_read_refusal_and_retains_every_reply(self):
        def envelope(value, failed=False):
            return {"isError":failed,"content":[{"type":"text","text":json.dumps(value)}]}
        retry = envelope({"status":"retry","retryable":True,"action":"repeat_same_request","code":"publication_busy","retry_after_ms":1},True)
        success = envelope({"hits":[],"future_field":{"retained":True}})
        retained = []
        read = mock.Mock(side_effect=[retry,success])
        with mock.patch.object(UPGRADE.time,"sleep"):
            self.assertEqual(UPGRADE.typed_https_read(read,retained.append,time.monotonic()+5),success)
        self.assertEqual(retained,[retry,success])
        self.assertEqual(read.call_count,2)
        self.assertEqual(read.call_args_list[0].args,read.call_args_list[1].args)
        submitted = envelope({"error":{"code":"direct_query_admission","reason":"busy","query_submitted":True,"retryable":True,"retry_after_ms":1}},True)
        retained = []
        read = mock.Mock(return_value=submitted)
        with self.assertRaises(AssertionError):
            UPGRADE.typed_https_read(read,retained.append,time.monotonic()+5)
        self.assertEqual(read.call_count,1)
        self.assertEqual(retained,[submitted])

    def test_https_comparison_preserves_all_envelope_fields_and_only_normalizes_known_timing(self):
        specification = importlib.util.spec_from_file_location("upgrade_http_parity", Path(__file__).parents[1] / "http_fixture.py")
        transport = importlib.util.module_from_spec(specification); specification.loader.exec_module(transport)
        left = {"content":[{"type":"text","text":json.dumps({"observed_unix_ms":1,"hits":[]})}],"structuredContent":{"request_id":"one","future":{"retained":True}},"future_top":{"schema":9}}
        right = json.loads(json.dumps(left)); right["content"][0]["text"] = json.dumps({"observed_unix_ms":2,"hits":[]}); right["structuredContent"]["request_id"] = "two"
        self.assertEqual(transport.parity_envelope(left),transport.parity_envelope(right))
        for key in ("structuredContent","future_top"):
            changed = json.loads(json.dumps(right)); changed[key] = {}
            self.assertNotEqual(transport.parity_envelope(left),transport.parity_envelope(changed))
        self.assertIsInstance(left["content"][0]["text"],str)

    def test_reviewed_fixture_uses_only_explicit_native_setup_and_one_source_grant(self):
        with tempfile.TemporaryDirectory() as parent:
            source, data = Path(parent) / "source", Path(parent) / "data"
            source.mkdir(); data.mkdir()
            calls = []
            registration = {"binding": "fixture", "root": str(source), "data_root": str(data), "collection": "upgrade"}
            revision = 1
            def rpc(method):
                nonlocal revision
                calls.append(method)
                if method == "request_epoch": return {"namespace": "native", "epoch": 7}
                name, body = next(iter(method.items()))
                if name == "catalog_host_context": return {"descriptor": {"namespace": "native", "catalog_revision": revision}}
                if name == "catalog_index_setup_draft": return {"draft": {"native": "unchanged"}}
                if name == "catalog_index_setup_apply":
                    self.assertEqual(body["draft"], {"native": "unchanged"}); revision += 1
                    return {"status": "catalog_index_setup_applied", "receipt": {"stage": "ready", "catalog_revision": revision, "index_id": "idx_fixture", "source_ids": ["src_fixture"]}}
                if name == "catalog_preview": return {"preview_digest": "native-preview"}
                if name == "catalog_apply":
                    self.assertEqual(body["preview_digest"], "native-preview"); revision += 1
                    return {"status": "catalog_applied", "receipt": {"catalog_revision": revision}}
                if name == "catalog_grant_draft": return {"draft": {"request": {"operation": {"grant": {"id": "grt_fixture"}}}, "preview_digest": "native-preview"}}
                if name == "catalog_detail": return {"detail": {"item": {"value": {"backing": registration}}}}
                self.fail(f"Unexpected fixture mutation: {method}")
            actual, selected = UPGRADE.reviewed_fixture(rpc, source, data)
            self.assertEqual(actual, registration)
            setup = next(row["catalog_index_setup_draft"]["selection"] for row in calls if isinstance(row, dict) and "catalog_index_setup_draft" in row)
            self.assertEqual((setup["primary_root"], setup["data_root"], setup["additional_roots"], setup["setup_watch"]), (str(source), str(data), [], False))
            member = next(row["catalog_preview"]["request"]["operation"]["membership"] for row in calls if isinstance(row, dict) and "catalog_preview" in row and row["catalog_preview"]["request"]["operation"]["operation"] == "set_membership")
            self.assertEqual(member["scope"], [{"source_root_id": "src_fixture", "kind": "subtree", "path": ""}])
            grant = next(row["catalog_grant_draft"]["selection"] for row in calls if isinstance(row, dict) and "catalog_grant_draft" in row)
            self.assertEqual(grant["index_ids"], ["idx_fixture"])
            self.assertEqual(grant["project_ids"], selected["project_ids"])
            self.assertEqual(len(selected["project_ids"]), 1)

    def test_reviewed_snapshot_preserves_full_values_and_refuses_incomplete_membership(self):
        selected = {"context_key": "context", "namespace": "native", "index_id": "index", "project_ids": ["project"], "grant_id": "grant"}
        continuation = None
        def rpc(method):
            name, body = next(iter(method.items()))
            if name == "catalog_host_context": return {"descriptor": {"namespace": "native", "catalog_revision": 9}, "capabilities": {"policy_contract": 3}}
            if name == "catalog_detail": return {"detail": {"namespace": "native", "revision": 9, "item": {"kind": body["entity"]["kind"], "value": {"future": [1, 2], "id": body["entity"]["id"]}}}}
            if name == "catalog_page": return {"page": {"namespace": "native", "revision": 9, "continuation": continuation, "items": [{"future_member": {"retained": True}}]}}
            self.fail(str(method))
        actual, capabilities = UPGRADE.reviewed_snapshot(rpc, selected)
        self.assertEqual(actual["grant"], {"future": [1, 2], "id": "grant"})
        self.assertEqual(actual["memberships"], [{"future_member": {"retained": True}}])
        self.assertEqual(capabilities, {"policy_contract": 3})
        continuation = {"next": "unread"}
        with self.assertRaisesRegex(RuntimeError, "incomplete"):
            UPGRADE.reviewed_snapshot(rpc, selected)

    def test_failure_cleanup_never_masks_original_refusal_or_deletes_fixture(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent))
            root = Path(temporary.name)
            (root / "install").mkdir()
            sentinel = root / "diagnostic.json"
            sentinel.write_text('{"native":"original-refusal"}')
            def failed_stop():
                raise RuntimeError("secondary stop failure")
            with self.assertRaisesRegex(RuntimeError, "original ConfigurationUnavailable"):
                try:
                    raise RuntimeError("original ConfigurationUnavailable")
                finally:
                    observed = UPGRADE.finish_fixture(temporary, root, False,
                        lambda: self.fail("failed fixture must not uninstall"), observe_stop=failed_stop)
            self.assertTrue(observed["fixture_retained"])
            self.assertEqual(observed["errors"][0]["operation"], "observe_owned_candidate_stop")
            self.assertEqual(sentinel.read_text(), '{"native":"original-refusal"}')

    def test_pending_committed_activation_never_auto_uninstalls_or_aborts(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            install = root / "install"; install.mkdir()
            (install / "activation.json").write_text('{"phase":"probe"}')
            observed = UPGRADE.finish_fixture(temporary, root, True,
                lambda: self.fail("pending activation must not uninstall"))
            self.assertTrue(observed["activation_pending"])
            self.assertTrue(observed["fixture_retained"])
            self.assertFalse(observed["teardown_complete"])
            self.assertTrue((install / "activation.json").is_file())

    def test_uninstall_failure_retains_previously_completed_assertion_evidence(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            (root / "install").mkdir()
            def refuse_uninstall(): raise RuntimeError("teardown refused")
            observed = UPGRADE.finish_fixture(temporary, root, True, refuse_uninstall)
            self.assertTrue(root.is_dir())
            self.assertFalse(observed["teardown_complete"])
            self.assertEqual(observed["errors"][0]["operation"], "uninstall")

    def test_success_removes_only_owned_fixture_after_acknowledged_uninstall(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            (root / "install").mkdir(); sibling = Path(parent) / "unrelated"; sibling.mkdir()
            calls = []
            observed = UPGRADE.finish_fixture(temporary, root, True, lambda: (calls.append("uninstall") or {"uninstalled": True}))
            self.assertEqual(calls, ["uninstall"])
            self.assertTrue(observed["teardown_complete"])
            self.assertFalse(root.exists())
            self.assertTrue(sibling.is_dir())

    def test_missing_install_never_counts_as_acknowledged_uninstall(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            observed = UPGRADE.finish_fixture(temporary, root, True,
                lambda: self.fail("missing install cannot be uninstalled"))
            self.assertFalse(observed["uninstall_acknowledged"])
            self.assertFalse(observed["teardown_complete"])
            self.assertTrue(root.is_dir())
            self.assertIn("missing", observed["errors"][0]["error"])

    def test_deferred_uninstall_preserves_fixture_and_its_unknown_state(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            (root / "install").mkdir()
            observed = UPGRADE.finish_fixture(temporary, root, True,
                lambda: {"uninstalled": False, "deferred_runtimes": ["owned"]})
            self.assertFalse(observed["uninstall_acknowledged"])
            self.assertTrue(root.is_dir())

    def test_native_timeout_retains_exact_partial_streams_and_original_exception(self):
        with tempfile.TemporaryDirectory() as parent:
            root = Path(parent); records = []
            error = subprocess.TimeoutExpired(["owned.exe", "runtime", "activate"], 1,
                output=b"partial native stdout", stderr=b"original activation diagnostics")
            with mock.patch.object(UPGRADE.subprocess, "run", side_effect=error):
                with self.assertRaises(subprocess.TimeoutExpired) as observed:
                    UPGRADE.record_native_command(root, records, "owned.exe", ["runtime", "activate"], timeout=1)
            self.assertIs(observed.exception, error)
            self.assertEqual((root / "command-001.stdout").read_bytes(), b"partial native stdout")
            self.assertEqual((root / "command-001.stderr").read_bytes(), b"original activation diagnostics")
            self.assertEqual(records[0]["failure_type"], "TimeoutExpired")
            self.assertIsNone(records[0]["exit_code"])

if __name__ == "__main__": unittest.main()
