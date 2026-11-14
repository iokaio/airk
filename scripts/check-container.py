# SPDX-License-Identifier: Apache-2.0
"""Smoke-test one container, two collections and persistence using only public fixtures."""
import argparse
from copy import deepcopy
import hashlib
import re
import importlib.util
import json
import queue
from pathlib import Path, PurePosixPath
import subprocess
import sys
import threading
import time
import uuid


spec = importlib.util.spec_from_file_location("service_clients", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)
spec = importlib.util.spec_from_file_location("combined_evidence", Path(__file__).with_name("combined_evidence.py"))
combined = importlib.util.module_from_spec(spec)
spec.loader.exec_module(combined)


def identifier(value, prefix="", length=32):
    assert isinstance(value, str) and re.fullmatch(re.escape(prefix) + "[0-9a-f]{" + str(length) + "}", value), value
    return value


def reviewed_fixture(rpc, collection):
    """Review native setup and one full-root Project grant; never replay a mutation."""
    source, data = "/workspaces/" + collection, "/data/indexes/" + collection
    def key():
        epoch = rpc("request_epoch")
        assert type(epoch["epoch"]) is int and epoch["epoch"] > 0, epoch
        return {"namespace": identifier(epoch["namespace"]), "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex}
    context = uuid.uuid4().hex + uuid.uuid4().hex
    descriptor = rpc({"catalog_host_context": {"key": context, "expected_catalog_revision": None}})["descriptor"]
    namespace = identifier(descriptor["namespace"])
    revision = descriptor["catalog_revision"]
    selection = {"key": key(), "expected_catalog_revision": revision, "name": "Container " + collection,
                 "primary_root": source, "data_root": data, "collection": collection,
                 "additional_roots": [], "setup_watch": False}
    assert selection["key"]["namespace"] == namespace
    review = rpc({"catalog_index_setup_draft": {"selection": selection}})
    assert review["status"] == "catalog_index_setup_draft", review
    draft = review["draft"]
    assert draft["schema"] == 1 and draft["request"]["key"] == selection["key"]
    request = draft["request"]
    assert request["expected_catalog_revision"] == revision and request["setup_watch"] is False
    registration = request["registration"]
    assert (registration["root"], registration["data_root"], registration["collection"]) == (source, data, collection)
    identifier(registration["binding"], length=64)
    assert request["sources"] == [{"canonical_path": source, "aliases": [], "owner": True}], request
    assert draft["storage_action"] in ("create_missing_leaf", "claim_existing_empty_storage"), draft
    identifier(draft["review_digest"], length=64)
    setup = rpc({"catalog_index_setup_apply": {"draft": draft}})
    assert setup["status"] == "catalog_index_setup_applied", setup
    receipt = setup["receipt"]
    assert receipt["schema"] == 1 and receipt["stage"] == "ready" and receipt["request"] == request, receipt
    identifier(receipt["request_digest"], length=64)
    index = identifier(receipt["index_id"], "idx_")
    assert len(receipt["source_ids"]) == 1
    source_id = identifier(receipt["source_ids"][0], "src_")
    revision = receipt["catalog_revision"]
    project = "prj_" + uuid.uuid4().hex
    def apply(operation):
        nonlocal revision
        intent = {"caller": uuid.uuid4().hex, "key": key(), "expected_catalog_revision": revision, "operation": operation}
        assert intent["key"]["namespace"] == namespace
        preview = rpc({"catalog_preview": {"request": intent}})
        assert preview["status"] == "catalog_preview", preview
        assert preview["preview"]["operation"] == intent["operation"] and preview["preview"]["catalog_revision"] == revision
        identifier(preview["preview"]["request_digest"], length=64)
        identifier(preview["preview_digest"], length=64)
        response = rpc({"catalog_apply": {"request": intent, "preview_digest": preview["preview_digest"]}})
        assert response["status"] == "catalog_applied", response
        applied = response["receipt"]
        assert (applied["caller"] == intent["caller"] and applied["key"] == intent["key"]
                and applied["request_digest"] == preview["preview"]["request_digest"]), response
        if applied["authority_fence_required"]:
            fenced = response["fenced_receipt"]
            assert fenced["request"] == intent and fenced["metadata"] == applied
            assert fenced["preview_digest"] == preview["preview_digest"]
            identifier(fenced["prepared_digest"], length=64)
            proof = fenced["proof"]
            identifier(proof["plan_digest"], length=64)
            identifier(proof["service"])
            assert type(proof["gate_generation"]) is int and proof["gate_generation"] > 0
            assert isinstance(proof["confirmations"], list)
            # Native admission authenticates acknowledgements/reaping. Preserve
            # that complete proof; a boolean flag alone is not a fenced receipt.
            assert all(item["method"] in ("native_scope_ack", "native_worker_reap") and isinstance(item["target"], dict)
                       for item in proof["confirmations"])
        revision = response["receipt"]["catalog_revision"]
        return response
    apply({"operation": "create_project", "id": project, "name": "Container " + collection + " set"})
    membership = {"project_id": project, "index_id": index, "revision": 1,
                  "scope": [{"source_root_id": source_id, "kind": "subtree", "path": ""}]}
    joined = apply({"operation": "set_membership", "membership": membership,
                   "expected_scope_revision": 1, "expected_membership_revision": None})
    assert joined["receipt"]["authority_fence_required"] is True, joined
    descriptor = rpc({"catalog_host_context": {"key": context, "expected_catalog_revision": revision}})["descriptor"]
    assert descriptor["namespace"] == namespace and descriptor["catalog_revision"] == revision
    grant_review = rpc({"catalog_grant_draft": {"context_key": context, "selection": {"key": key(),
        "expected_catalog_revision": revision, "project_ids": [project], "index_ids": [index]}}})
    assert grant_review["status"] == "catalog_grant_draft", grant_review
    grant = grant_review["draft"]
    intent = grant["request"]
    assert intent["expected_catalog_revision"] == revision and intent["key"]["namespace"] == namespace
    frozen = intent["operation"]["grant"]
    grant_id = identifier(frozen["id"], "grt_")
    assert intent["operation"]["operation"] == "create_grant" and frozen["revision"] == 1 and frozen["revoked"] is False, frozen
    assert frozen["host"] == descriptor["host"]
    assert len(frozen["memberships"]) == 1 and frozen["memberships"][0]["membership"] == membership, frozen
    granted = rpc({"catalog_apply": {"request": intent, "preview_digest": grant["preview_digest"]}})
    assert granted["status"] == "catalog_applied", granted
    applied = granted["receipt"]
    assert (applied["caller"] == intent["caller"] and applied["key"] == intent["key"]
            and applied["request_digest"] == grant["preview"]["request_digest"]), granted
    selected = {"schema": 1, "context_key": context, "namespace": namespace,
                "expected_catalog_revision": granted["receipt"]["catalog_revision"], "grant_id": grant_id,
                "expected_grant_revision": frozen["revision"], "project_ids": [project], "index_ids": [index]}
    return {"collection": collection, "registration": registration, "selection": selected,
            "source_id": source_id, "membership": membership}


def catalog_snapshot(rpc, scope):
    selected = scope["selection"]
    descriptor = rpc({"catalog_host_context": {"key": selected["context_key"], "expected_catalog_revision": None}})["descriptor"]
    assert descriptor["namespace"] == selected["namespace"]
    revision = descriptor["catalog_revision"]
    result = {}
    for kind, identity in (("index", selected["index_ids"][0]), ("project", selected["project_ids"][0]), ("grant", selected["grant_id"])):
        detail = rpc({"catalog_detail": {"entity": {"kind": kind, "id": identity}, "expected_catalog_revision": revision}})["detail"]
        assert detail["namespace"] == selected["namespace"] and detail["revision"] == revision and detail["item"]["kind"] == kind, detail
        result[kind] = detail["item"]["value"]
    assert result["index"]["backing"] == scope["registration"]
    page = rpc({"catalog_page": {"request": {"kind": "memberships", "project_id": selected["project_ids"][0],
        "index_id": selected["index_ids"][0], "after": None, "expected_catalog_revision": revision, "limit": 16, "max_bytes": 48000}}})["page"]
    assert page["namespace"] == selected["namespace"] and page["revision"] == revision and page["continuation"] is None
    assert len(page["items"]) == 1 and page["items"][0]["value"] == scope["membership"], page
    result["memberships"] = page["items"]
    return result


def validate_delivery(value, scope):
    """Authenticate the full raw body before any derived assertion/normalization."""
    delivery = value["delivery_observation"]
    assert delivery["digest_basis"] == "canonical_body_without_delivery_observation", delivery
    receipt = delivery["receipt"]
    body = deepcopy(value)
    del body["delivery_observation"]
    assert receipt["result_digest"] == combined.framed_digest("airk/combined-delivery-result/1", body), receipt
    assert receipt["schema"] == 1 and receipt["namespace"] == scope["selection"]["namespace"]
    assert receipt["catalog_revision"] >= scope["selection"]["expected_catalog_revision"]
    identifier(receipt["service_instance"])
    identifier(receipt["scope_identity"], length=64)
    return receipt


def selected_scope(value, scope):
    selected = value["selection"]
    expected = scope["selection"]
    assert selected["project_ids"] == expected["project_ids"] and selected["index_ids"] == expected["index_ids"], selected
    assert selected["grant_id"] == expected["grant_id"] and selected["grant_revision"] == expected["expected_grant_revision"], selected
    identifier(selected["identity"], length=64)
    return selected


def member_result(value, scope, allow_unpublished=False):
    assert value["schema"] == "airk.combined-read/1", value
    receipt = validate_delivery(value, scope)
    selected = selected_scope(value, scope)
    assert receipt["scope_identity"] == selected["identity"] == value["publication_vector"]["authority"]
    index = scope["selection"]["index_ids"][0]
    assert len(value["members"]) == 1 and len(value["publication_vector"]["members"]) == 1
    member = value["members"][0]
    assert member["index_id"] == index and member["outcome"] == "complete" and member["is_error"] is False, member
    assert value["publication_vector"]["members"][0]["index_id"] == index
    allocation = member["allocation"]
    assert allocation["index_id"] == index and 0 < allocation["limit"] <= 100 and 0 < allocation["max_bytes"] <= 65536
    assert 0 < allocation["deadline_ms"] <= 30000
    result = combined.member_result(value, index)
    assert result["collection"] == scope["collection"]
    if result.get("publication") is not None or not allow_unpublished:
        combined.publication(value, result["publication"], index)
    return result


def query_scope(value, scope, view):
    assert value["schema"] == "airk.query-scope/1" and value["mode"] == "reviewed_grant" and value["view"] == view, value
    selected = selected_scope(value, scope)
    receipt = validate_delivery(value, scope)
    assert receipt["scope_identity"] == selected["identity"] == value["scope_identity"]
    assert value["grant"] == {"id": scope["selection"]["grant_id"], "revision": scope["selection"]["expected_grant_revision"]}
    assert value["truncated"] is False and value["cursor"] is None and value["count"] == len(value["items"]) == 1
    item = value["items"][0]
    if view != "indexes":
        assert item["project_id"] == scope["selection"]["project_ids"][0]
    if view == "projects":
        assert item["index_ids"] == scope["selection"]["index_ids"]
    elif view == "indexes":
        assert item["index_id"] == scope["selection"]["index_ids"][0] and item["membership"]["membership"] == scope["membership"]
    else:
        assert item["index_id"] == scope["selection"]["index_ids"][0] and item["source_root_id"] == scope["source_id"]
        assert item["original_root"] == scope["registration"]["root"] and item["scope"] == scope["membership"]["scope"]
    return value


# These are transport-generated request IDs, dated observations and elapsed
# allowances. No scope, revision, budget count/bytes, retry or evidence field is omitted.
TIMING_FIELDS = {"request_id", "observed_unix_ms", "observed_from_unix_ms", "observed_to_unix_ms", "cache_age_ms", "cached", "elapsed_ms"}

def parity_value(value):
    if isinstance(value, list):
        return [parity_value(item) for item in value]
    if not isinstance(value, dict):
        return value
    result = {key: parity_value(item) for key, item in value.items() if key not in TIMING_FIELDS}
    if isinstance(result.get("allocation"), dict):
        result["allocation"]["deadline_ms"] = 0
    if isinstance(result.get("delivery_observation"), dict):
        result["delivery_observation"]["receipt"].pop("result_digest", None)
    return result


def logical_result(raw, scope, refused=False):
    """A full logical comparison copy; complete original dictionaries/proofs are retained."""
    if raw.get("schema") != "airk.combined-read/1":
        if not refused:
            query_scope(raw, scope, raw["view"])
        elif "delivery_observation" in raw:
            validate_delivery(raw, scope)
        return parity_value(raw)
    if refused:
        validate_delivery(raw, scope); selected_scope(raw, scope)
        assert len(raw["members"]) == 1 and raw["members"][0]["index_id"] == scope["selection"]["index_ids"][0]
        assert raw["members"][0]["is_error"] is True
    else:
        member_result(raw, scope)
    logical = deepcopy(raw)
    for member in logical["members"]:
        member["result"] = combined.member_result(raw, member["index_id"])
    for rank in logical.get("ranking", []):
        rank["origin"] = combined.ranked_origin(raw, rank)
    logical.pop("shared_values", None)
    return parity_value(logical)


def docker(*args, check=True, timeout=60):
    result = subprocess.run(["docker", *args], check=False, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)
    if check and result.returncode:
        print(result.stderr[-4096:], file=sys.stderr, flush=True)
        result.check_returncode()
    return result


def enroll_container_http(name, install, control, selection, identifier, records, *, split=False):
    """One public installer submission using an owned in-container input file."""
    spec = importlib.util.spec_from_file_location("container_native_setup", Path(__file__).with_name("http_fixture.py"))
    helper = importlib.util.module_from_spec(spec); spec.loader.exec_module(helper)
    until = time.monotonic() + 60
    payload = json.dumps(selection, separators=(",", ":"))
    assert len(payload.encode()) <= 49152
    path = str(PurePosixPath(control) / "http-1" / ("fixture-selection-" + uuid.uuid4().hex + ".json"))
    writer = "import os,sys; f=os.open(sys.argv[1],os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600); s=os.fdopen(f,'wb'); s.write(sys.stdin.buffer.read(49153)); s.close()"
    command = ["docker", "exec", "-i", name, "python3", "-c", writer, path]
    record = {"transport":"native_http_setup_input", "command":command, "selection":selection,
              "connection_id":identifier, "client":"unknown", "install_dir":install, "selection_file":path}
    records.append(record)
    result = subprocess.run(command, input=payload, text=True, capture_output=True, timeout=clients.remaining(until,60))
    record.update(exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr)
    result.check_returncode()
    try:
        native = ["airk", *(["__airk_role", "setup-interim"] if split else []),
                  "runtime", "http-connection", "--install-dir", install,
                  "--selection-file", path, "--connection-id", identifier, "--client", "unknown"]
        record["native_command"] = native
        response = docker("exec", name, *native, timeout=clients.remaining(until,60))
        record["enrollment"] = {"exit_code":response.returncode,"stdout":response.stdout,"stderr":response.stderr}
        connection = json.loads(response.stdout)
        # Filesystem/CA validation occurs inside the container in check_https;
        # the host must not pretend Path.resolve authenticates container files.
        return helper.validate_enrollment(connection, control, selection, identifier,
                                          reference_resolver=PurePosixPath)
    finally:
        cleanup = "import os,sys,hashlib; p=sys.argv[1]; f=os.open(p,os.O_RDONLY|os.O_NOFOLLOW); m=os.fstat(f); b=os.read(f,49153); n=os.stat(p,follow_symlinks=False); assert m.st_uid==os.getuid() and m.st_nlink==1 and (m.st_dev,m.st_ino)==(n.st_dev,n.st_ino) and hashlib.sha256(b).hexdigest()==sys.argv[2]; os.unlink(p); os.close(f)"
        docker("exec",name,"python3","-c",cleanup,path,hashlib.sha256(payload.encode()).hexdigest(),timeout=5)


def owned_container(name, token):
    observed = docker("inspect", name, check=False)
    if observed.returncode:
        if any(observed.stderr.strip().endswith(f"No such {kind}: {name}") for kind in ("container", "object")):
            return None
        observed.check_returncode()
    container = json.loads(observed.stdout)[0]
    if (container["Config"].get("Labels") or {}).get("io.airk.fixture") != token:
        raise RuntimeError("Container ownership changed; preserved foreign container")
    return container["Id"]


def remove_owned_container(name, token):
    identity = owned_container(name, token)
    if identity is None:
        return
    result = docker("rm", "--force", identity, check=False)
    if result.returncode and not result.stderr.strip().endswith(f"No such container: {identity}"):
        result.check_returncode()


def check_stdio(name, install, scope, cli_tool, catalog, records):
    selector = json.dumps(scope["selection"])
    process = subprocess.Popen(["docker", "exec", "-i", name, "airk", "--service-install-dir", install,
                                "--combined-session", selector, "mcp"], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    responses = queue.Queue()
    threading.Thread(target=lambda: [responses.put(line) for line in process.stdout], daemon=True).start()
    serial = 0
    def exchange(method, params, timeout=60):
        nonlocal serial
        serial += 1
        message = {"jsonrpc": "2.0", "id": serial, "method": method, "params": params}
        record = {"transport": "stdio", "input": message}; records.append(record)
        process.stdin.write(json.dumps(message) + "\n"); process.stdin.flush()
        deadline = time.monotonic() + timeout
        for _ in range(10):
            reply = json.loads(responses.get(timeout=clients.remaining(deadline, timeout)))
            if reply.get("id") == serial:
                record["output"] = reply
                assert "error" not in reply, reply
                return reply["result"]
        raise AssertionError("No matching MCP response")
    try:
        def notify(message):
            process.stdin.write(json.dumps(message) + "\n"); process.stdin.flush()
        check_mcp(exchange, notify, scope, cli_tool, catalog, records)
    finally:
        process.stdin.close()
        try: process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill(); process.wait(timeout=10)


def check_mcp(exchange, notify, scope, cli_tool, catalog, records):
    initialized = exchange("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
        "clientInfo": {"name": "airk-container-check", "version": "1"}})
    assert initialized["serverInfo"]["name"] == "airk"
    notify({"jsonrpc": "2.0", "method": "notifications/initialized"})
    tools = exchange("tools/list", {})["tools"]
    names = [tool["name"] for tool in tools]
    assert len(names) == len(clients.MCP_TOOLS) and set(names) == clients.MCP_TOOLS, names
    native = {tool["name"].removeprefix("airk_"): tool for tool in catalog}
    assert len(catalog) == len(clients.TOOLS), "Native catalog contains duplicate or extra entries"
    assert set(native) == {name.removeprefix("airk_") for name in clients.TOOLS}
    for tool in tools:
        if tool["name"] in native:
            assert tool["inputSchema"] == native[tool["name"]]["inputSchema"], tool["name"]
    for tool_name, arguments in evidence_requests(scope):
        expected = cli_tool(tool_name, arguments)
        def read(until):
            return exchange("tools/call", {"name": tool_name.removeprefix("airk_"), "arguments": arguments}, clients.remaining(until, 60))
        envelope, _ = clients.read_envelope(read, time.monotonic() + 60)
        assert envelope.get("isError") is not True, envelope
        actual = clients.tool_value(envelope)
        assert logical_result(actual, scope) == logical_result(expected, scope), tool_name
    # Deliberately refused exact input: no retry, even if an envelope looks transient.
    denied_args = {"path": "../project-b/README.md", "start_line": 1, "end_line": 2, "response_mode": "full"}
    expected_denial = cli_tool("airk_read_file", denied_args)
    denied = exchange("tools/call", {"name": "read_file", "arguments": denied_args})
    assert denied.get("isError") is True, denied
    def no_evidence(value):
        if isinstance(value, dict):
            assert not any(value.get(key) for key in ("hits", "items", "text", "lines")), value
            for entry in value.values(): no_evidence(entry)
        elif isinstance(value, list):
            for entry in value: no_evidence(entry)
    actual_denial = clients.tool_value(denied)
    no_evidence(actual_denial); no_evidence(expected_denial)
    for value in (actual_denial, expected_denial):
        if "delivery_observation" in value: validate_delivery(value, scope)
    assert logical_result(actual_denial, scope, refused=True) == logical_result(expected_denial, scope, refused=True), "CLI/MCP refusal data differs"


def check_https(name, control, connection, scope, cli_tool, catalog, records):
    """Run the actual test client in the service's loopback namespace.

    Python exists only in Docker's explicit smoke stage. No AIRK frontend,
    launcher, bridge or additional installed runtime executable is involved.
    """
    script = Path(__file__).with_name("http_fixture.py").read_text(encoding="utf-8") + '''
import sys
configuration = json.loads(sys.stdin.readline())
client = McpClient(configuration["connection"], configuration["control"])
try:
    for line in sys.stdin:
        value = json.loads(line)
        if value.get("fixture_api") == "catalog":
            reply = client.api_catalog()
        elif value.get("fixture_api") == "read":
            reply = client.api_read(value["tool"], value["arguments"])
        else:
            client.send(value)
            if "id" not in value:
                continue
            reply = client.receive(value["id"])
        print(json.dumps(reply), flush=True)
finally:
    client.close()
'''
    process = subprocess.Popen(["docker", "exec", "-i", name, "python3", "-u", "-c", script],
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               text=True, encoding="utf-8")
    responses = queue.Queue()
    def receive_lines():
        try:
            for line in process.stdout:
                responses.put(line)
        finally:
            responses.put(None)
    threading.Thread(target=receive_lines, daemon=True).start()
    serial = 0
    def notify(message):
        process.stdin.write(json.dumps(message) + "\n"); process.stdin.flush()
    def raw(message, timeout=60):
        record = {"transport": "https", "input": message}; records.append(record)
        notify(message)
        line = responses.get(timeout=timeout)
        assert line is not None, "Owned container HTTPS test client exited before its reply"
        reply = json.loads(line)
        record["output"] = reply
        return reply
    def exchange(method, params, timeout=60):
        nonlocal serial
        serial += 1
        reply = raw({"jsonrpc": "2.0", "id": serial, "method": method, "params": params}, timeout)
        assert reply.get("id") == serial and "error" not in reply
        return reply["result"]
    try:
        notify({"connection": connection, "control": control})
        check_mcp(exchange, notify, scope, cli_tool, catalog, records)
        api_catalog = raw({"fixture_api": "catalog"})
        assert api_catalog["schema"] == "airk.local-api/1" and api_catalog["contract_version"] == 1
        native = {tool["name"].removeprefix("airk_"): tool for tool in catalog}
        actual = {tool["name"].removeprefix("airk_"): tool for tool in api_catalog["tools"]}
        assert len(actual) == len(api_catalog["tools"]) and set(native).issubset(actual)
        for tool, expected in native.items():
            assert actual[tool]["inputSchema"] == expected["inputSchema"], tool
        for tool, arguments in evidence_requests(scope):
            expected = cli_tool(tool, arguments)
            envelope = raw({"fixture_api": "read", "tool": tool.removeprefix("airk_"), "arguments": arguments})
            assert envelope.get("isError") is not True
            actual = clients.tool_value(envelope)
            assert logical_result(actual, scope) == logical_result(expected, scope), "CLI/JSON API full result differs: " + tool
    finally:
        process.stdin.close()
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill(); process.wait(timeout=10)
            raise AssertionError("Owned container HTTPS test client failed to exit") from None
        errors = process.stderr.read(65_537)
        records.append({"transport": "https-client-exit", "exit_code": process.returncode, "stderr": errors})
        assert len(errors) <= 65_536 and process.returncode == 0, "Owned container HTTPS test client failed"


def evidence_requests(scope):
    full = {"response_mode": "full", "verification": "off"}
    yield "airk_symbols", {"name": "authenticate", "limit": 5, "max_bytes": 16384, **full}
    yield "airk_search", {"query": "authenticate", "mode": "lexical", "limit": 5, "max_bytes": 16384, **full}
    yield "airk_query_scope", {"view": "projects"}
    yield "airk_query_scope", {"view": "indexes", "project_id": scope["selection"]["project_ids"][0]}
    yield "airk_query_scope", {"view": "sources", "project_id": scope["selection"]["project_ids"][0], "index_id": scope["selection"]["index_ids"][0]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default="airk:dev")
    parser.add_argument("--report", type=Path, help="Complete raw native fixture observations (default: ignored artifacts directory)")
    args = parser.parse_args()
    token = uuid.uuid4().hex
    name = "airk-service-check-" + token[:12]
    volume = name + "-data"
    fixtures = Path(__file__).resolve().parent.parent / "examples/workspaces"
    volume_created = False
    scopes = {}
    split = False
    report = args.report or Path("artifacts/container-check-" + token + ".json")
    evidence = {"schema": "airk-container-check/2", "image": args.image, "records": [], "scopes": scopes, "status": "running"}
    records = evidence["records"]
    original_hashes = {str(file.relative_to(fixtures)): hashlib.sha256(file.read_bytes()).hexdigest()
                       for file in fixtures.rglob("*") if file.is_file()}
    evidence["source_hashes_before"] = original_hashes

    def start():
        docker("run", "--detach", "--name", name, "--label", f"io.airk.fixture={token}", "--network", "none",
               "--read-only", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
               "--tmpfs", "/tmp", "--mount", f"type=bind,source={fixtures},target=/workspaces,readonly",
               "--mount", f"type=volume,source={volume},target=/data", args.image)

    def command(collection, *command_args, check=True, timeout=60):
        selected = scopes[collection]["selection"]
        selected_client = ("airkctl", "--service-runtime-dir", control) if split else ("airk", "--service-install-dir", install)
        exact = ("exec", name, *selected_client, "--combined-session", json.dumps(selected), *command_args)
        record = {"transport": "cli", "input": exact}; records.append(record)
        response = docker(*exact, check=check, timeout=timeout)
        record.update({"stdout": response.stdout, "stderr": response.stderr, "exit_code": response.returncode})
        return response

    def rpc(method, deadline=None):
        def invoke(exact, until):
            arguments = ["docker", "exec", "-i", name, "airkctl" if split else "airk", "service", "rpc", "--runtime-dir", control, "--stdin"]
            record = {"transport": "metadata", "command": arguments, "input": exact}; records.append(record)
            response = subprocess.run(arguments, input=exact, text=True, capture_output=True,
                                      check=False, timeout=clients.remaining(until, 60))
            record.update({"stdout": response.stdout, "stderr": response.stderr, "exit_code": response.returncode})
            response.check_returncode()
            return json.loads(response.stdout)
        return clients.metadata_rpc(invoke, method, deadline)

    def ready():
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            location = docker("exec", name, "airk", *(["__airk_role", "setup-interim"] if split else []), "runtime", "location", check=False)
            if location.returncode == 0:
                control = json.loads(location.stdout)["user_paths"]["state"] + "/control"
                health = docker("exec", name, "airkctl" if split else "airk", "service", "health", "--runtime-dir", control, check=False)
                if health.returncode == 0:
                    return control
            time.sleep(0.2)
        raise AssertionError("Container service did not become healthy")

    def wait_job(control, accepted):
        job_id = accepted["job"]["id"]
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            job = rpc({"job": {"id": job_id}}, deadline)["job"]
            assert job["id"] == job_id
            if job["state"] == "succeeded":
                return job
            assert job["state"] not in ("failed", "canceled"), job
            time.sleep(0.2)
        raise AssertionError("Native container job did not finish")

    def tool(collection, name, arguments=None, deadline=None, allow_error=False):
        deadline = min(deadline if deadline is not None else float("inf"), time.monotonic() + 60)
        exact = ("tools", "call", name, "--args", json.dumps(arguments or {"response_mode": "full"}))
        def read(until):
            result = command(collection, *exact, check=False, timeout=clients.remaining(until, 60))
            assert result.returncode in (0, 1), f"Container read process failed: exit {result.returncode}"
            envelope = json.loads(result.stdout)
            assert result.returncode == 0 or allow_error and envelope.get("isError") is True or clients.read_retry_delay(envelope) is not None, "Container read failed without typed rejection"
            return envelope
        if allow_error:
            envelope = read(deadline)  # Deliberate negative probe executes exactly once.
            assert envelope.get("isError") is True, envelope
        else:
            envelope, _ = clients.read_envelope(read, deadline)
            assert envelope.get("isError") is not True, envelope
        return clients.tool_value(envelope)

    def published_status(collection):
        return clients.observe_publication(lambda until: member_result(tool(collection, "airk_diagnostics", deadline=until), scopes[collection], allow_unpublished=True),
                                           collection, scopes[collection]["registration"]["binding"], time.monotonic() + 60)

    try:
        docker("volume", "create", "--label", f"io.airk.fixture={token}", volume)
        assert json.loads(docker("volume", "inspect", volume).stdout)[0]["Labels"].get("io.airk.fixture") == token
        volume_created = True
        start()
        image_runtime = json.loads(docker("exec", name, "cat", "/usr/lib/airk/airk-runtime.json").stdout)
        assert image_runtime["schema"] in (1, 2, 3), "Unknown container runtime format"
        split = image_runtime["schema"] == 3
        control = ready()
        print("Native container service is healthy.", flush=True)
        docker("exec", name, "mkdir", "-p", "/data/indexes")
        install = json.loads(docker("exec", name, "airk", *(["__airk_role", "setup-interim"] if split else []), "runtime", "location").stdout)["install_dir"]
        modern = image_runtime["schema"] in (2, 3)
        for collection in ("project-a", "project-b"):
            scopes[collection] = reviewed_fixture(rpc, collection)
            binding = scopes[collection]["registration"]["binding"]
            accepted_command = ("exec", name, "airkctl" if split else "airk", "--service-runtime-dir", control, "--service-project", binding, "index")
            indexed = docker(*accepted_command)
            records.append({"transport": "administrative_cli", "input": accepted_command, "stdout": indexed.stdout, "stderr": indexed.stderr})
            accepted = json.loads(indexed.stdout)
            wait_job(control, accepted)
            print(f"Indexed isolated collection {collection}.", flush=True)
        snapshots = {collection: catalog_snapshot(rpc, scope) for collection, scope in scopes.items()}
        statuses = {collection: published_status(collection) for collection in scopes}
        publications = {collection: status["publication"] for collection, status in statuses.items()}
        evidence["catalog_before_restart"] = snapshots
        evidence["publications_before_restart"] = publications
        def assertions(collection):
            scope = scopes[collection]
            for tool_name, arguments in evidence_requests(scope):
                raw = tool(collection, tool_name, arguments)
                if tool_name == "airk_query_scope":
                    query_scope(raw, scope, arguments["view"])
                else:
                    result = member_result(raw, scope)
                    allocation = raw["members"][0]["allocation"]
                    assert allocation["limit"] <= arguments["limit"] and allocation["max_bytes"] <= arguments["max_bytes"]
                    if tool_name == "airk_symbols":
                        assert bool(result["items"]) is (collection == "project-a")
                    else:
                        assert bool(result["hits"]) is (collection == "project-a"), result
                        assert len(raw["ranking"]) == len(result["hits"]), raw
                        for rank in raw["ranking"]:
                            origin = combined.ranked_origin(raw, rank)
                            assert origin["index_id"] == scope["selection"]["index_ids"][0]
                            assert origin["project_ids"] == scope["selection"]["project_ids"] and origin["source_root_id"] == scope["source_id"]
                            assert origin["source_relative_path"] in {hit["citation"]["path"] for hit in result["hits"]}
                        for hit in result["hits"]:
                            citation = hit["citation"]
                            relative = Path(citation["path"])
                            assert not relative.is_absolute() and ".." not in relative.parts
                            original = fixtures / collection / relative
                            assert citation["content_hash"] == hashlib.sha256(original.read_bytes()).hexdigest(), citation
            catalog = json.loads(command(collection, "tools", "list", "--json").stdout)["tools"]
            cli_read = lambda name, args: tool(collection, name, args, allow_error=name == "airk_read_file")
            if modern:
                connection = enroll_container_http(name, install, control,
                    {"kind":"combined","selection":scope["selection"]}, uuid.uuid4().hex, records, split=split)
                assert connection["selection"] == {"kind": "combined", "selection": scope["selection"]}, "HTTP scope changed during enrollment"
                helper_spec = importlib.util.spec_from_file_location("container_http_identity", Path(__file__).with_name("http_fixture.py"))
                helper = importlib.util.module_from_spec(helper_spec); helper_spec.loader.exec_module(helper)
                helper.validate_native_identity(connection, rpc("health")["observation"])
                check_https(name, control, connection, scope, cli_read, catalog, records)
            else:
                check_stdio(name, install, scope, cli_read, catalog, records)
        assertions("project-a"); assertions("project-b")
        pin = json.loads(docker("exec", name, "airk", *(["__airk_role", "setup-interim"] if split else []), "runtime", "inspect", "--install-dir", install).stdout)
        assert pin["managed_policy"]["lifecycle"] == "external"
        if modern:
            assert docker("exec", name, "airk", *(["__airk_role", "setup-interim"] if split else []), "--version").stdout.strip() == "airk " + pin["approval"]["version"]
            if split:
                assert docker("exec", name, "airkctl", "--version").stdout.strip() == "airkctl " + pin["approval"]["version"]
            for obsolete in ("/usr/lib/airk/airk-embed", install + "/airk-launcher"):
                assert docker("exec", name, "test", "!", "-e", obsolete, check=False).returncode == 0, "Modern container resurrected a retired executable"
        else:
            client = json.loads(docker("exec", name, "airk", "runtime", "prepare-client", "--install-dir", install).stdout)
            assert client["launcher"] == install + "/airk-launcher" and client["service_lifecycle_changed"] is False
            assert pin["launcher"] == client["launcher"]
            assert docker("exec", name, client["launcher"], "--version").stdout.strip() == "airk " + pin["approval"]["version"]
        print("Read-only MCP and collection isolation passed.", flush=True)
        docker("exec", name, "test", "-f", "/usr/lib/airk/notices/inventory.json")
        if not modern:
            docker("exec", name, "test", "-x", "/usr/lib/airk/airk-embed")
        observed = json.loads(docker("inspect", name).stdout)[0]
        evidence["container_properties"] = observed
        assert observed["HostConfig"]["NetworkMode"] == "none" and observed["HostConfig"]["ReadonlyRootfs"] is True
        assert observed["Config"]["User"].split(":")[0] not in ("", "0", "root")
        assert "ALL" in observed["HostConfig"]["CapDrop"] and "no-new-privileges" in observed["HostConfig"]["SecurityOpt"]
        mount = next(item for item in observed["Mounts"] if item["Destination"] == "/workspaces")
        assert mount["RW"] is False
        identity = owned_container(name, token)
        assert identity is not None, "Owned container disappeared before restart"
        docker("stop", identity)
        remove_owned_container(name, token)
        start()
        assert ready() == control
        assert {collection: catalog_snapshot(rpc, scope) for collection, scope in scopes.items()} == snapshots
        assert {collection: published_status(collection)["publication"] for collection in scopes} == publications
        assertions("project-a"); assertions("project-b")
        assert published_status("project-a")["files"] == 2
        # Retired direct-store deletion cannot remove either service-owned collection.
        assert command("project-a", "collections", "remove", "project-b", check=False).returncode != 0
        assert (fixtures / "project-b/README.md").is_file()
        assert published_status("project-b")["files"] > 0
        assert published_status("project-a")["files"] == 2
    except BaseException as error:
        evidence["failure"] = {"type": type(error).__name__, "message": str(error)}
        raise
    finally:
        try:
            evidence["source_hashes_after"] = {str(file.relative_to(fixtures)): hashlib.sha256(file.read_bytes()).hexdigest()
                                               for file in fixtures.rglob("*") if file.is_file()}
            assert evidence["source_hashes_after"] == original_hashes, "Container fixture sources changed"
            if volume_created:
                # Remove only label-verified identities and wait for their mounts.
                remove_owned_container(name, token)
                assert json.loads(docker("volume", "inspect", volume).stdout)[0]["Labels"].get("io.airk.fixture") == token
                docker("volume", "rm", volume)
        except BaseException as error:
            evidence["cleanup_failure"] = {"type": type(error).__name__, "message": str(error)}
            raise
        finally:
            evidence["status"] = "failed" if evidence.get("failure") or evidence.get("cleanup_failure") else "passed"
            report.parent.mkdir(parents=True, exist_ok=True)
            report.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    print("Container smoke passed: exact native/MCP evidence, two isolated collections, offline/read-only runtime, persistent restart, versioned executable inventory, notices and refusal of retired direct writers.")


if __name__ == "__main__":
    main()
