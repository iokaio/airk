# SPDX-License-Identifier: Apache-2.0
"""Qualify the actual standard-runtime JSON examples against one sealed runtime.

This is a T2 campaign. It starts only a private native fixture and performs no
OS trust enrollment. MCP GET/SSE coverage remains in native transport tests;
this receipt qualifies the two raw API routes and explicitly selected reads.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time
import uuid
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


clients, transport = load("check-service-clients"), load("http_fixture")
# Same complete raw read catalog as Fixture.http_mcp; assistance mutations are
# intentionally absent from the API's documented read-only catalog.
READ_NAMES = {tool.removeprefix("airk_") for tool in clients.TOOLS} | {
    "projects", "list_templates", "get_template", "validate_draft", "proposal_status", "get_handoff"}
NONREAD_MCP_NAMES = {"draft_search", "draft_strategy", "draft_plan", "suggest_view"}
CASES = (
    ("query_scope", {"view": "projects", "response_mode": "full"}, False),
    ("list_files", {"response_mode": "full"}, False),
    ("search", {"query": "rawexamplemarker", "mode": "lexical", "response_mode": "full"}, False),
    ("read_file", {"path": "fixture.py", "start_line": 1, "end_line": 2, "response_mode": "full"}, False),
    ("read_file", {"path": "../other/secret.py", "response_mode": "full"}, True),
)


def commands():
    if os.name != "nt":
        raise RuntimeError("Three-client qualification requires Windows PowerShell protected-file support")
    node, pwsh = shutil.which("node"), shutil.which("pwsh")
    if not node or not pwsh:
        raise RuntimeError("All three actual clients are required; Node or PowerShell is missing")
    return {
        "python": ([sys.executable, str(ROOT / "examples/local_https_client.py")], ("--connection", "--tool", "--arguments")),
        "node": ([node, str(ROOT / "examples/local_https_client.cjs")], ("--connection", "--tool", "--arguments")),
        "powershell": ([pwsh, "-NoProfile", "-NonInteractive", "-File", str(ROOT / "examples/local_https_client.ps1")], ("-Connection", "-Tool", "-Arguments")),
    }


def invoke(command, flags, connection_file, tool, arguments, token, environment):
    argv = [*command, flags[0], str(connection_file)]
    if tool is not None:
        argv += [flags[1], tool, flags[2], json.dumps(arguments, separators=(",", ":"))]
    started = time.monotonic()
    result = subprocess.run(argv, capture_output=True, text=True, encoding="utf-8", timeout=135,
                            env=environment, creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    # Never retain a bearer, including an accidental client diagnostic echo.
    if token in result.stdout or token in result.stderr:
        raise AssertionError("Example disclosed a credential; raw output withheld")
    if len(result.stdout.encode("utf-8")) > 2 << 20 or len(result.stderr.encode("utf-8")) > 65536:
        raise AssertionError("Example output exceeded evidence budget")
    body = json.loads(result.stdout)
    if result.returncode not in (0, 1):
        raise AssertionError("Example process failed outside its documented result contract")
    return {"exit_code": result.returncode, "elapsed_seconds": time.monotonic() - started,
            "body": body, "stderr": result.stderr, "dispatches": 1, "retries": 0}


def verify_result(observed, baseline, negative=False):
    expected_exit = 1 if negative else 0
    if observed["exit_code"] != expected_exit:
        raise AssertionError("Example exit status disagrees with the native result")
    if transport.parity_envelope(observed["body"]) != transport.parity_envelope(baseline):
        raise AssertionError("Complete example/native result parity differs")


def catalog_tools(catalog):
    if catalog.get("schema") != "airk.local-api/1" or catalog.get("contract_version") != 1:
        raise AssertionError("Raw API schema/contract differs")
    tools = catalog["tools"]
    if len({tool["name"] for tool in tools}) != len(tools):
        raise AssertionError("Duplicate raw API tool schema")
    return {tool["name"]: tool for tool in tools}


def verify_catalog(catalog, mcp_catalog):
    schemas = catalog_tools(catalog)
    raw = {name.removeprefix("airk_"): schema for name, schema in schemas.items()}
    mcp = {name.removeprefix("airk_"): schema for name, schema in mcp_catalog.items()
           if name.removeprefix("airk_") in READ_NAMES}
    mcp_read_count = sum(name.removeprefix("airk_") in READ_NAMES for name in mcp_catalog)
    mcp_extra = {name.removeprefix("airk_") for name in mcp_catalog} - READ_NAMES
    if (len(schemas) != len(READ_NAMES) or set(raw) != READ_NAMES or set(mcp) != READ_NAMES
            or mcp_read_count != len(READ_NAMES) or not mcp_extra <= NONREAD_MCP_NAMES):
        raise AssertionError("Complete raw/MCP read catalog is incomplete or duplicated")
    def values(rows):
        return {name: {key: value for key, value in schema.items() if key != "name"}
                for name, schema in rows.items()}
    if values(raw) != values(mcp):
        raise AssertionError("Complete raw/MCP tool schemas differ")
    return raw


def verify_relocated(previous, current, previous_token_digest, current_token_digest):
    before, after = previous["endpoint"], current["endpoint"]
    if (type(before.get("generation")) is not int or type(after.get("generation")) is not int
            or before["generation"] <= 0 or after["generation"] <= before["generation"]):
        raise AssertionError("Endpoint generation did not advance across restart")
    if (before["url"] == after["url"] or before["service_instance"] == after["service_instance"]
            or previous_token_digest != current_token_digest
            or previous["connection_id"] != current["connection_id"]):
        raise AssertionError("Endpoint relocation lost credential continuity or did not restart/relocate")
    for field in ("installation", "user", "namespace", "ca_sha256", "executable_sha256"):
        if before[field] != after[field]:
            raise AssertionError("Endpoint relocation changed the enrolled native authority")
    for field in ("schema", "ca_certificate_file", "credential_file"):
        if previous[field] != current[field]:
            raise AssertionError("Endpoint relocation changed protected connection references")
    if not same_selection(previous["selection"], current["selection"]):
        raise AssertionError("Endpoint relocation changed the complete selected scope")


def same_selection(before, after):
    """Only a forward native catalog clock may differ in the full authority."""
    if isinstance(before, dict):
        if not isinstance(after, dict) or before.keys() != after.keys():
            return False
        return all((type(before[key]) is int and type(after[key]) is int and after[key] >= before[key])
                   if key == "expected_catalog_revision" else same_selection(before[key], after[key]) for key in before)
    if isinstance(before, list):
        return isinstance(after, list) and len(before) == len(after) and all(same_selection(a, b) for a, b in zip(before, after))
    return type(before) is type(after) and before == after


def credential_observation(connection, control):
    """Fixture-only generation check; retain neither verifier nor private ledger."""
    ledger = json.loads(transport.private_bytes(Path(control) / "http-1/credentials.json", control, 1_048_576))
    records = [record for record in ledger["records"] if record["id"] == connection["connection_id"]]
    if len(records) != 1 or records[0]["revoked"] or type(records[0]["generation"]) is not int or records[0]["generation"] <= 0:
        raise AssertionError("Native fixture credential is absent, revoked or malformed")
    return {key: records[0][key] for key in ("id", "generation", "selection", "client", "expires_unix_ms")}


def existing_connection(fixture, selection, connection_id):
    response = fixture.run("runtime", "http-connection-info", "--install-dir", fixture.install,
                           "--connection-id", connection_id)
    connection = json.loads(response.stdout)
    return transport.validate_enrollment(connection, fixture.control, selection, connection_id)


def private_json(path, value):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
        json.dump(value, stream)


def harness_identity():
    paths = [ROOT / "examples" / ("local_https_client" + suffix) for suffix in (".py", ".cjs", ".ps1")]
    paths += [Path(__file__).resolve(), ROOT / "scripts/check-service-clients.py", ROOT / "scripts/http_fixture.py"]
    return {path.relative_to(ROOT).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}


def qualify(runtime, report):
    selected = commands()  # Fail before native setup if a client is missing.
    inputs = harness_identity()
    receipt = {"schema": "airk-raw-example-qualification/1", "passed": False,
               "selected_clients": list(selected), "selected_cases": list(CASES), "observations": [], "endpoints": [],
               "routes": ["GET /api/v1/tools", "POST /api/v1/tools/{name}"],
               "separate_required_coverage": ["GET /mcp session SSE native transport regression"],
               "normalization": "scripts/http_fixture.py parity_envelope: documented transport identities/timing only",
               "harness_inputs": inputs}
    report = report.resolve()
    if report.exists():
        raise FileExistsError("Qualification report already exists")
    fixture = None
    def save():
        report.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    try:
        with clients.owned_client_fixture() as directory:
            owner = Path(directory)
            receipt["fixture"] = str(owner)
            source, data = owner / "source", owner / "data"
            source.mkdir(); data.mkdir()
            source_file = source / "fixture.py"
            source_file.write_text("def rawexamplemarker():\n    return 42\n", encoding="utf-8")
            original = source_file.read_bytes()
            fixture = clients.Fixture(runtime, owner)
            with fixture:
                if not fixture.single_image:
                    raise AssertionError("Raw qualification requires the modern single-image runtime")
                receipt["runtime_manifest_sha256"] = hashlib.sha256((fixture.runtime / "airk-install.json").read_bytes()).hexdigest()
                receipt["compiled_source"] = json.loads((fixture.runtime / "airk-build.json").read_text())["source"]
                fixture.register("raw-example", source, data)
                fixture.job("raw-example", "index")
                selection = {"kind": "scoped", "selection": fixture.scopes["raw-example"]}
                connection_id = uuid.uuid4().hex
                previous = None
                for phase in ("initial", "changed_endpoint"):
                    connection = (fixture.enroll_http(selection, connection_id, "cli") if phase == "initial"
                                  else existing_connection(fixture, selection, connection_id))
                    transport.validate_native_identity(connection, fixture.rpc("health")["observation"])
                    token = transport.private_bytes(connection["credential_file"], fixture.control, 103).decode("ascii")
                    token_digest = hashlib.sha256(token.encode()).hexdigest()
                    credential = credential_observation(connection, fixture.control)
                    if previous is not None:
                        verify_relocated(previous["connection"], connection, previous["token_digest"], token_digest)
                        if credential != previous["credential"]:
                            raise AssertionError("Endpoint relocation changed credential generation or issuance authority")
                    receipt["endpoints"].append({"phase": phase, "endpoint": connection["endpoint"],
                                                 "connection_id": connection_id, "selection": connection["selection"],
                                                 "credential_sha256": token_digest, "credential": credential})
                    connection_file = fixture.control / "http-1" / (phase + "-raw-connection.json")
                    private_json(connection_file, connection)
                    network = transport.McpClient(connection, fixture.control)
                    try:
                        network.send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                            "protocolVersion": "2025-11-25", "capabilities": {},
                            "clientInfo": {"name": "raw-examples-parity", "version": "1"}}})
                        network.receive(1)
                        network.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
                        network.send({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
                        mcp_catalog = {tool["name"]: tool for tool in network.receive(2)["result"]["tools"]}
                        catalog = network.api_catalog()
                        schemas = verify_catalog(catalog, mcp_catalog)
                        for name, (command, flags) in selected.items():
                            observed = invoke(command, flags, connection_file, None, None, token, fixture.environment)
                            verify_result(observed, catalog)
                            receipt["observations"].append({"phase": phase, "client": name, "kind": "catalog", **observed})
                            save()
                        for ordinal, (tool, arguments, negative) in enumerate(CASES, 10):
                            if tool not in schemas:
                                raise AssertionError("Selected read is absent from the advertised catalog")
                            baseline = network.api_read(tool, arguments)
                            if baseline.get("isError", False) != negative:
                                raise AssertionError("Native positive/negative fixture assumption differs")
                            network.send({"jsonrpc": "2.0", "id": ordinal, "method": "tools/call", "params": {"name": tool, "arguments": arguments}})
                            mcp = network.receive(ordinal)["result"]
                            cli = fixture.run(*fixture.arguments("raw-example"), "tools", "call", tool, "--args", json.dumps(arguments), check=False)
                            cli_envelope = json.loads(cli.stdout)
                            if not (transport.parity_envelope(baseline) == transport.parity_envelope(mcp) == transport.parity_envelope(cli_envelope)):
                                raise AssertionError("Complete raw/MCP/CLI results differ")
                            receipt["observations"].append({"phase": phase, "kind": "canonical_parity", "tool": tool, "arguments": arguments,
                                                            "raw": baseline, "mcp": mcp, "cli": cli_envelope})
                            for name, (command, flags) in selected.items():
                                observed = invoke(command, flags, connection_file, tool, arguments, token, fixture.environment)
                                verify_result(observed, baseline, negative)
                                receipt["observations"].append({"phase": phase, "client": name, "tool": tool, "arguments": arguments, **observed})
                                save()
                        for tool in ("missing_fixture_tool", "index"):
                            try:
                                network.api_read(tool, {})
                            except transport.HttpRefused as refused:
                                baseline = refused.body
                                receipt["observations"].append({"phase": phase, "kind": "http_refusal", "tool": tool,
                                                                "evidence": refused.evidence()})
                            else:
                                raise AssertionError("Unknown or mutation tool was admitted by the raw API")
                            for name, (command, flags) in selected.items():
                                observed = invoke(command, flags, connection_file, tool, {}, token, fixture.environment)
                                verify_result(observed, baseline, True)
                                receipt["observations"].append({"phase": phase, "client": name, "tool": tool, "arguments": {}, **observed})
                                save()
                    finally:
                        network.close()
                    if phase == "initial":
                        previous = {"url": connection["endpoint"]["url"], "token_digest": token_digest,
                                    "connection": connection, "credential": credential}
                        fixture.close()  # Canonical one-dispatch no-force Stop, retaining its native receipt.
                        receipt["intermediate_stop"] = fixture.managed_stop
                        with socket.socket() as occupant:
                            if os.name == "nt":
                                occupant.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
                            occupant.bind(("127.0.0.1", urlsplit(previous["url"]).port)); occupant.listen()
                            # Start can take effect before an uncertain reply. Invalidate
                            # the old Stop receipt before dispatch, so teardown cannot
                            # report an earlier process's cleanup as this one.
                            fixture.managed_stop = None
                            fixture.managed_health = None
                            started = fixture.run("runtime", "start", "--install-dir", fixture.install)
                            receipt["ordinary_start"] = json.loads(started.stdout)
                            fixture.managed_health = fixture.rpc("health")["observation"]
                            fresh = existing_connection(fixture, selection, connection_id)
                            if fresh["endpoint"]["url"] == previous["url"]:
                                raise AssertionError("Occupied saved endpoint was reused")
                            occupant.setblocking(False)
                            try:
                                peer, _ = occupant.accept()
                            except BlockingIOError:
                                receipt["occupant_contacted"] = False
                            else:
                                peer.close()
                                raise AssertionError("Native relocation contacted the foreign endpoint")
                if source_file.read_bytes() != original:
                    raise AssertionError("Raw client fixture source changed")
                receipt["source_preserved"] = True
            receipt["native_stop_complete"] = fixture.managed_stop is not None
            receipt["selected_observation_count"] = len(receipt["observations"])
            receipt["expected_observation_count"] = 2 * (len(selected) + (len(CASES) + 2) * (1 + len(selected)))
            if receipt["selected_observation_count"] != receipt["expected_observation_count"]:
                raise AssertionError("Raw example campaign did not execute every selected observation")
            receipt["harness_inputs_unchanged"] = inputs == harness_identity()
            if not receipt["harness_inputs_unchanged"]:
                raise AssertionError("Consumed raw example/harness inputs changed during qualification")
            receipt["passed"] = receipt["native_stop_complete"] and receipt["selected_observation_count"] > 0
    except BaseException as error:
        receipt["failure_type"] = type(error).__name__
        if isinstance(error, transport.HttpResponseError):
            receipt["failure_evidence"] = error.evidence()
        message = str(error)
        if "token" in locals():
            message = message.replace(token, "[REDACTED]")
        receipt["failure_message"] = message[:4096]
        receipt["native_stop_complete"] = fixture is not None and fixture.managed_stop is not None
        raise
    finally:
        save()
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    result = qualify(args.runtime_dir, args.report)
    print(json.dumps({"passed": result["passed"], "selected_observation_count": result["selected_observation_count"], "report": str(args.report)}))


if __name__ == "__main__":
    main()
