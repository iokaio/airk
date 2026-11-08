# SPDX-License-Identifier: Apache-2.0
"""Current reviewed container fixture contracts, without running Docker."""
import contextlib
from copy import deepcopy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import MagicMock, patch

spec = importlib.util.spec_from_file_location("container_check", Path(__file__).parents[1] / "check-container.py")
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class NativeCatalog:
    """Closed current wire fixture; mutations must receive their exact preview."""
    def __init__(self):
        self.revision = 0
        self.namespace = "11" * 16
        self.calls = []
        self.scopes = {}
        self.preview = None
        self.fail = None

    def __call__(self, method):
        self.calls.append(deepcopy(method))
        if method == "request_epoch":
            return {"namespace": self.namespace, "epoch": 1}
        name, fields = next(iter(method.items()))
        if name == self.fail:
            return {"status": "outcome_unknown", "original": deepcopy(fields)}
        if name == "catalog_host_context":
            assert fields["expected_catalog_revision"] in (None, self.revision)
            return {"status": name, "descriptor": {"namespace": self.namespace, "catalog_revision": self.revision, "host": "12" * 32}}
        if name == "catalog_index_setup_draft":
            selected = fields["selection"]
            collection = selected["collection"]
            assert selected["primary_root"] == "/workspaces/" + collection
            assert selected["data_root"] == "/data/indexes/" + collection
            assert selected["additional_roots"] == [] and selected["setup_watch"] is False
            registration = {"root": selected["primary_root"], "data_root": selected["data_root"], "collection": collection,
                            "binding": hashlib.sha256(collection.encode()).hexdigest()}
            request = {"caller": "22" * 16, "key": selected["key"], "expected_catalog_revision": self.revision,
                       "name": selected["name"], "registration": registration,
                       "sources": [{"canonical_path": selected["primary_root"], "aliases": [], "owner": True}], "setup_watch": False}
            self.draft = {"schema": 1, "request": request, "storage_action": "create_missing_leaf", "review_digest": "33" * 32}
            return {"status": name, "draft": deepcopy(self.draft)}
        if name == "catalog_index_setup_apply":
            assert fields["draft"] == self.draft
            request = fields["draft"]["request"]
            collection = request["registration"]["collection"]
            self.revision += 1
            digit = "a" if collection == "project-a" else "b"
            scope = {"registration": request["registration"], "index": "idx_" + digit * 32,
                     "source": "src_" + digit * 32, "project": None, "membership": None, "grant": None}
            self.scopes[collection] = scope
            return {"status": "catalog_index_setup_applied", "receipt": {"schema": 1, "request": request,
                "request_digest": "44" * 32, "stage": "ready", "catalog_revision": self.revision,
                "index_id": scope["index"], "source_ids": [scope["source"]]}}
        if name == "catalog_preview":
            request = fields["request"]
            assert request["expected_catalog_revision"] == self.revision
            self.preview = {"request_digest": hashlib.sha256(json.dumps(request).encode()).hexdigest(),
                            "catalog_revision": self.revision, "operation": deepcopy(request["operation"])}
            return {"status": name, "preview": deepcopy(self.preview), "preview_digest": "55" * 32}
        if name == "catalog_apply":
            request = fields["request"]
            assert fields["preview_digest"] == "55" * 32
            assert request["operation"] == self.preview["operation"] and request["expected_catalog_revision"] == self.revision
            operation = request["operation"]
            scope = list(self.scopes.values())[-1]
            kind = operation["operation"]
            if kind == "create_project":
                scope["project"] = operation["id"]
            elif kind == "set_membership":
                assert operation["expected_scope_revision"] == 1 and operation["expected_membership_revision"] is None
                assert operation["membership"]["scope"] == [{"source_root_id": scope["source"], "kind": "subtree", "path": ""}]
                scope["membership"] = deepcopy(operation["membership"])
            elif kind == "create_grant":
                scope["grant"] = deepcopy(operation["grant"])
            else:
                raise AssertionError(kind)
            self.revision += 1
            response = {"status": "catalog_applied", "receipt": {"caller": request["caller"], "key": request["key"],
                    "request_digest": self.preview["request_digest"], "catalog_revision": self.revision,
                    "authority_fence_required": kind == "set_membership"}}
            if kind == "set_membership":
                response["fenced_receipt"] = {"request": deepcopy(request), "metadata": deepcopy(response["receipt"]),
                    "preview_digest": "55" * 32, "prepared_digest": "56" * 32,
                    "proof": {"plan_digest": "57" * 32, "service": "58" * 16, "gate_generation": 1, "confirmations": []}}
            return response
        if name == "catalog_grant_draft":
            selection = fields["selection"]
            scope = list(self.scopes.values())[-1]
            assert selection["project_ids"] == [scope["project"]] and selection["index_ids"] == [scope["index"]]
            grant = {"id": "grt_" + scope["index"][4:], "host": "12" * 32, "revision": 1, "revoked": False,
                     "memberships": [{"membership": scope["membership"], "source_revision": 1, "policy_epoch": 0, "policy_digest": None}]}
            request = {"caller": "66" * 16, "key": selection["key"], "expected_catalog_revision": self.revision,
                       "operation": {"operation": "create_grant", "grant": grant}}
            self.preview = {"request_digest": "77" * 32, "catalog_revision": self.revision, "operation": request["operation"]}
            return {"status": name, "draft": {"request": request, "preview": self.preview, "preview_digest": "55" * 32}}
        if name == "catalog_detail":
            entity = fields["entity"]
            kind, identity = entity["kind"], entity["id"]
            scope = next(s for s in self.scopes.values() if identity in (s["index"], s["project"], s["grant"]["id"]))
            value = {"index": {"id": scope["index"], "backing": scope["registration"]},
                     "project": {"id": scope["project"]}, "grant": scope["grant"]}[kind]
            return {"status": name, "detail": {"namespace": self.namespace, "revision": self.revision, "item": {"kind": kind, "value": deepcopy(value)}}}
        if name == "catalog_page":
            request = fields["request"]
            scope = next(s for s in self.scopes.values() if s["index"] == request["index_id"])
            return {"status": name, "page": {"namespace": self.namespace, "revision": self.revision, "continuation": None,
                    "items": [{"kind": "membership", "value": deepcopy(scope["membership"])}]}}
        if name == "job":
            return {"status": "job", "job": {"id": fields["id"], "state": "succeeded"}}
        raise AssertionError(method)


def fixture():
    native = NativeCatalog()
    scope = smoke.reviewed_fixture(native, "project-a")
    return native, scope


def delivered(body, scope):
    body = deepcopy(body)
    body["delivery_observation"] = {"digest_basis": "canonical_body_without_delivery_observation", "receipt": {
        "schema": 1, "namespace": scope["selection"]["namespace"], "catalog_revision": 99,
        "service_instance": "88" * 16, "scope_identity": "99" * 32,
        "result_digest": smoke.combined.framed_digest("airk/combined-delivery-result/1", body)}}
    return body


def native_body(scope, tool, arguments):
    selected = scope["selection"]
    selection = {"project_ids": selected["project_ids"], "index_ids": selected["index_ids"], "grant_id": selected["grant_id"],
                 "grant_revision": 1, "identity": "99" * 32, "member_availability": "report"}
    if tool == "airk_query_scope":
        view = arguments["view"]
        item = {"projects": {"project_id": selected["project_ids"][0], "index_ids": selected["index_ids"]},
                "indexes": {"index_id": selected["index_ids"][0], "membership": {"membership": scope["membership"], "source_revision": 1, "policy_epoch": 0, "policy_digest": None}},
                "sources": {"project_id": selected["project_ids"][0], "index_id": selected["index_ids"][0], "source_root_id": scope["source_id"],
                            "original_root": scope["registration"]["root"], "scope": scope["membership"]["scope"]}}[view]
        return delivered({"schema": "airk.query-scope/1", "mode": "reviewed_grant", "view": view, "selection": selection,
            "scope_identity": "99" * 32, "grant": {"id": selected["grant_id"], "revision": 1}, "items": [item],
            "count": 1, "truncated": False, "cursor": None}, scope)
    publication = {"token": "publication-" + scope["collection"], "collection": scope["collection"]}
    result = {"collection": scope["collection"], "publication": {"member": 0}}
    matches = scope["collection"] == "project-a"
    if tool == "airk_symbols":
        result["items"] = [{"name": "authenticate"}] if matches else []
    elif tool == "airk_search":
        path = Path(__file__).parents[2] / "examples/workspaces" / scope["collection"] / "README.md"
        result["hits"] = [{"citation": {"path": "README.md", "content_hash": hashlib.sha256(path.read_bytes()).hexdigest()}, "text": "authenticate"}] if matches else []
    else:
        result.update({"index_state": "available", "files": 2})
    ranking = [{"index_id": selected["index_ids"][0], "origin": {"index_id": selected["index_ids"][0], "source_root_id": scope["source_id"],
        "project_ids": selected["project_ids"], "source_relative_path": "README.md", "publication_ref": {"member": 0}}}] if tool == "airk_search" and matches else []
    return delivered({"schema": "airk.combined-read/1", "selection": selection, "members": [{"index_id": selected["index_ids"][0],
        "outcome": "complete", "is_error": False, "result": result,
        "allocation": {"index_id": selected["index_ids"][0], "limit": arguments.get("limit", 16), "max_bytes": arguments.get("max_bytes", 16384), "deadline_ms": 30000}}],
        "publication_vector": {"authority": "99" * 32, "members": [{"index_id": selected["index_ids"][0], "publication": publication}]}, "ranking": ranking}, scope)


def envelope(value):
    return {"content": [{"type": "text", "text": json.dumps(value)}]}


def tool_catalog(names):
    return [{"name": name, "inputSchema": {"type": "object", "properties": {"full": {"type": "boolean"}}}} for name in sorted(names)]


class ContainerCheckTests(unittest.TestCase):
    def test_reviewed_setup_preserves_exact_review_membership_and_grant(self):
        native, scope = fixture()
        assert smoke.catalog_snapshot(native, scope)["memberships"][0]["value"] == scope["membership"]
        names = [next(iter(call)) if isinstance(call, dict) else call for call in native.calls]
        self.assertEqual(names.count("catalog_index_setup_apply"), 1)
        self.assertEqual(names.count("catalog_apply"), 3)
        self.assertNotIn("register", names)
        self.assertEqual(scope["selection"]["index_ids"], ["idx_" + "a" * 32])
        self.assertEqual(scope["registration"]["data_root"], "/data/indexes/project-a")

    def test_uncertain_mutation_is_never_replayed_or_rekeyed(self):
        for stage in ("catalog_index_setup_apply", "catalog_apply"):
            native = NativeCatalog(); native.fail = stage
            with self.subTest(stage=stage), self.assertRaises((AssertionError, KeyError)):
                smoke.reviewed_fixture(native, "project-a")
            self.assertEqual(sum(isinstance(call, dict) and stage in call for call in native.calls), 1)

    def test_reviewed_fixture_rejects_crossed_setup_and_missing_fence_before_next_mutation(self):
        for mutation in ("setup_root", "setup_key", "fence", "receipt_key"):
            native = NativeCatalog()
            def crossed(method):
                value = native(method)
                if isinstance(method, dict) and "catalog_index_setup_draft" in method:
                    if mutation == "setup_root": value["draft"]["request"]["registration"]["root"] = "/foreign"
                    if mutation == "setup_key": value["draft"]["request"]["key"]["nonce"] = "ff" * 16
                if isinstance(method, dict) and "catalog_apply" in method and value["receipt"]["authority_fence_required"]:
                    if mutation == "fence": del value["fenced_receipt"]
                    if mutation == "receipt_key": value["receipt"]["key"] = {"namespace":"ff"*16,"epoch":1,"nonce":"ff"*16}
                return value
            with self.subTest(mutation=mutation), self.assertRaises((AssertionError, KeyError)):
                smoke.reviewed_fixture(crossed, "project-a")
            self.assertFalse(any(isinstance(method,dict) and "catalog_grant_draft" in method for method in native.calls))

    def test_raw_delivery_scope_allocation_and_full_parity_fail_closed(self):
        _, scope = fixture()
        raw = native_body(scope, "airk_symbols", {})
        original = deepcopy(raw)
        self.assertTrue(smoke.member_result(raw, scope)["items"])
        self.assertEqual(raw, original)
        changed = deepcopy(raw); changed["members"][0]["result"]["new_complete_field"] = {"original": 42}
        with self.assertRaises(AssertionError): smoke.member_result(changed, scope)
        changed = delivered({k:v for k,v in changed.items() if k != "delivery_observation"}, scope)
        self.assertNotEqual(smoke.logical_result(changed, scope), smoke.logical_result(raw, scope))
        for mutation in ("scope", "allocation", "publication", "member"):
            changed = deepcopy(raw); del changed["delivery_observation"]
            if mutation == "scope": changed["selection"]["index_ids"] = ["idx_" + "f" * 32]
            if mutation == "allocation": changed["members"][0]["allocation"]["deadline_ms"] = 30001
            if mutation == "publication": changed["publication_vector"]["members"][0]["index_id"] = "idx_" + "f" * 32
            if mutation == "member": changed["members"][0]["is_error"] = True
            with self.subTest(mutation=mutation), self.assertRaises((AssertionError, ValueError)):
                smoke.member_result(delivered(changed, scope), scope)

    def test_stdio_requires_exact_catalog_inputs_and_full_result_parity(self):
        _, scope = fixture()
        native = tool_catalog(smoke.clients.TOOLS)
        mcp = tool_catalog(smoke.clients.MCP_TOOLS)
        for mutation in (None, "missing", "duplicate", "retired", "inputs", "result", "denial"):
            tools = deepcopy(mcp)
            if mutation == "missing": tools.pop()
            if mutation == "duplicate": tools[-1] = tools[0]
            if mutation == "retired": tools.append({"name": "airk_history"})
            if mutation == "inputs": next(t for t in tools if t["name"] == "symbols")["inputSchema"]["new_input"] = True
            replies = [{"id":1,"result":{"serverInfo":{"name":"airk"}}},{"id":2,"result":{"tools":tools}}]
            for i,(name,args) in enumerate(smoke.evidence_requests(scope),3):
                value = native_body(scope,name,args)
                if mutation == "result" and i == 3:
                    del value["delivery_observation"]; value["members"][0]["result"]["omitted_by_cli"] = True; value = delivered(value,scope)
                replies.append({"id":i,"result":envelope(value)})
            denied = envelope({"error":{"code":"scope_read_refused"}}); denied["isError"] = True
            if mutation == "denial": denied = envelope({"items":[{"private":"leak"}]})
            replies.append({"id":8,"result":denied})
            process=MagicMock(); process.stdin=io.StringIO(); process.stdout=io.StringIO("\n".join(json.dumps(r) for r in replies))
            with self.subTest(mutation=mutation), patch.object(smoke.subprocess,"Popen",return_value=process):
                args=("fixture","/data/install",scope,lambda name,args:{"error":{"code":"scope_read_refused"}} if name == "airk_read_file" else native_body(scope,name,args),native,[])
                if mutation is None: smoke.check_stdio(*args)
                else:
                    with self.assertRaises(AssertionError): smoke.check_stdio(*args)
            self.assertTrue(process.stdin.closed); process.wait.assert_called_once_with(timeout=10)

    def test_whole_smoke_keeps_reviewed_scope_publications_and_releases_before_volume_cleanup(self):
        self.whole_smoke(False)

    def test_modern_whole_smoke_enrolls_http_without_recreating_a_launcher_or_helper(self):
        self.whole_smoke(True)

    def whole_smoke(self, modern):
        native = NativeCatalog()
        running = False
        volume_removed = False
        starts = 0
        token = None
        job_inputs = {}
        observed_mcp = []
        selection_inputs = {}
        def docker(*args, check=True, timeout=60):
            nonlocal running, volume_removed, starts, token
            self.assertGreater(timeout, 0); self.assertLessEqual(timeout, 60)
            output = ""
            if args[:2] == ("volume", "create"):
                token = args[args.index("--label")+1].split("=",1)[1]
            elif args[:2] == ("volume", "inspect"):
                output = json.dumps([{"Labels":{"io.airk.fixture":token}}])
            elif args[:2] == ("volume", "rm"):
                self.assertFalse(running, "Container must release the volume first"); volume_removed = True
            elif args[0] == "run":
                self.assertFalse(running); running = True; starts += 1
                self.assertIn("--read-only", args); self.assertIn("none", args); self.assertIn("ALL", args)
            elif args[0] == "stop":
                self.assertEqual(args[1], "owned-id")
            elif args[0] == "rm":
                self.assertEqual(args[-1], "owned-id"); running = False
            elif args[0] == "inspect":
                output = json.dumps([{"Id":"owned-id","Config":{"Labels":{"io.airk.fixture":token},"User":"10001:10001"},
                    "HostConfig":{"NetworkMode":"none","ReadonlyRootfs":True,"CapDrop":["ALL"],"SecurityOpt":["no-new-privileges"]},
                    "Mounts":[{"Destination":"/workspaces","RW":False}]}])
            elif args[0] == "exec" and args[2] in ("mkdir", "test"):
                pass
            elif args[0] == "exec" and args[2:] == ("cat", "/usr/lib/airk/airk-runtime.json"):
                output = json.dumps({"schema": 2 if modern else 1})
            elif args[0] == "exec" and args[2] == "/data/install/airk-launcher":
                output = "airk 1.8.12\n"
            elif args[0] == "exec" and args[2] == "python3":
                path, expected = args[-2:]
                self.assertEqual(hashlib.sha256(json.dumps(selection_inputs[path],separators=(",",":")).encode()).hexdigest(), expected)
                del selection_inputs[path]
            elif args[0] == "exec" and args[2] == "airk":
                command = args[3:]
                if command == ("runtime", "location"):
                    output = json.dumps({"install_dir":"/data/install","user_paths":{"state":"/data/state"}})
                elif command[:2] == ("runtime", "http-connection"):
                    self.assertTrue(modern)
                    path = command[command.index("--selection-file")+1]
                    selection = selection_inputs[path]
                    identifier = command[command.index("--connection-id")+1]
                    self.assertEqual(command[command.index("--client")+1], "unknown")
                    self.assertEqual(selection["kind"], "combined")
                    private = path.rsplit("/",1)[0]
                    output = json.dumps({"schema":1,"connection_id":identifier,"selection":selection,
                        "credential_file":private+"/credential-"+identifier+".token","ca_certificate_file":private+"/ca.pem",
                        "endpoint":{"service_instance":"44"*16,"user":"10001","executable_sha256":"55"*32,"namespace":native.namespace}})
                elif command[:2] == ("service", "health"):
                    output = "{}"
                elif command[:2] == ("runtime", "prepare-client"):
                    output = json.dumps({"launcher":"/data/install/airk-launcher","service_lifecycle_changed":False})
                elif command[:2] == ("runtime", "inspect"):
                    output = json.dumps({"launcher":"/data/install/airk-launcher","managed_policy":{"lifecycle":"external"},"approval":{"version":"1.8.12"}})
                elif command == ("--version",) and modern:
                    output = "airk 1.8.12\n"
                elif "--service-project" in command:
                    binding = command[command.index("--service-project")+1]
                    self.assertIn(binding, [scope["registration"]["binding"] for scope in native.scopes.values()])
                    self.assertEqual(command[-1], "index")
                    output = json.dumps({"job":{"id":binding}})
                elif "--combined-session" in command:
                    selected = json.loads(command[command.index("--combined-session")+1])
                    record = next(scope for scope in native.scopes.values() if scope["index"] == selected["index_ids"][0])
                    scope = {"selection":selected,"collection":record["registration"]["collection"],"registration":record["registration"],
                             "membership":record["membership"],"source_id":record["source"]}
                    tail = command[command.index("--combined-session")+2:]
                    if tail == ("tools","list","--json"):
                        output = json.dumps({"tools":tool_catalog(smoke.clients.TOOLS)})
                    elif tail[:2] == ("tools","call"):
                        arguments = json.loads(tail[tail.index("--args")+1]); output = json.dumps(envelope(native_body(scope,tail[2],arguments)))
                    elif tail == ("collections","remove","project-b"):
                        self.assertFalse(check); return subprocess.CompletedProcess(args,2,"","retired writer")
                    else: self.fail(tail)
                else: self.fail(command)
            else: self.fail(args)
            return subprocess.CompletedProcess(args,0,output,"")
        def metadata(args, **kwargs):
            self.assertEqual(args[:3],["docker","exec","-i"])
            method = json.loads(kwargs["input"])
            if args[4] == "python3":
                self.assertIn("os.O_EXCL", args[-2])
                self.assertIn("0o600", args[-2])
                self.assertNotIn(args[-1], selection_inputs)
                selection_inputs[args[-1]] = method
                return subprocess.CompletedProcess(args, 0, "", "")
            self.assertFalse(isinstance(method,dict) and "http_enroll" in method)
            if method == "health":
                self.assertTrue(modern)
                return subprocess.CompletedProcess(args, 0, json.dumps({"status": "health", "observation": {
                    "instance": "44" * 16, "user": "10001", "executable_sha256": "55" * 32,
                    "catalog_clock": {"namespace": native.namespace}}}), "")
            if isinstance(method,dict) and "job" in method:
                job = method["job"]["id"]
                inputs = job_inputs.setdefault(job,[]); inputs.append(kwargs["input"])
                if len(inputs) == 1: return subprocess.CompletedProcess(args,0,json.dumps({"status":"failure","code":"registry_busy"}),"")
                self.assertEqual(inputs[0],kwargs["input"])
            return subprocess.CompletedProcess(args,0,json.dumps(native(method)),"")
        def mcp(name,install,scope,cli_tool,catalog,records):
            observed_mcp.append(deepcopy(scope["selection"]))
            self.assertTrue(smoke.member_result(cli_tool("airk_symbols",{"response_mode":"full","verification":"off"}),scope))
        def https(name, control, connection, scope, cli_tool, catalog, records):
            self.assertTrue(modern)
            self.assertEqual(connection["selection"], {"kind": "combined", "selection": scope["selection"]})
            mcp(name, control, scope, cli_tool, catalog, records)
        with tempfile.TemporaryDirectory() as folder:
            report = Path(folder)/"report.json"
            with patch.object(smoke,"docker",side_effect=docker), patch.object(smoke.subprocess,"run",side_effect=metadata), \
                 patch.object(smoke,"check_stdio",side_effect=mcp) as stdio, patch.object(smoke,"check_https",side_effect=https) as http, patch("sys.argv",["check-container.py","--report",str(report)]), \
                 contextlib.redirect_stdout(io.StringIO()):
                smoke.main()
                if modern:
                    stdio.assert_not_called(); self.assertEqual(http.call_count, 4)
                else:
                    http.assert_not_called(); self.assertEqual(stdio.call_count, 4)
            result = json.loads(report.read_text())
        self.assertEqual(result["status"],"passed")
        self.assertEqual(result["source_hashes_before"],result["source_hashes_after"])
        self.assertEqual(starts,2); self.assertTrue(volume_removed); self.assertFalse(running)
        self.assertEqual(observed_mcp[:2],observed_mcp[2:])
        self.assertEqual(sum(isinstance(m,dict) and "catalog_index_setup_apply" in m for m in native.calls),2)
        self.assertTrue(all(len(attempts)==2 for attempts in job_inputs.values()))
        self.assertTrue(result["records"])

    def test_cleanup_accepts_only_missing_owned_container(self):
        name = "airk-scaffold-check-fixture"
        for error, absent in (
                (f"Error response from daemon: No such container: {name}\n", True),
                ("Error response from daemon: No such container: another\n", False),
                (f"Error response from daemon: No such container: {name}-other\n", False),
                ("Cannot connect to the Docker daemon", False),
                ("permission denied", False)):
            with self.subTest(error=error):
                result = subprocess.CompletedProcess(["docker", "rm", name], 1, "", error)
                with patch.object(smoke, "docker", return_value=result):
                    if absent:
                        smoke.remove_owned_container(name, "fixture-token")
                    else:
                        with self.assertRaises(subprocess.CalledProcessError) as caught:
                            smoke.remove_owned_container(name, "fixture-token")
                        self.assertEqual(caught.exception.stderr, error)



if __name__ == "__main__":
    unittest.main()
