# SPDX-License-Identifier: Apache-2.0
"""Inspect model-visible tool offers in synthetic, loopback-only Codex requests."""

from __future__ import annotations

import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
from queue import Empty, Queue
import subprocess
import sys
from threading import Thread
from time import monotonic

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import mcp_wrapper
import search_focus_ab as search


CANARY = "AIRK_SYNTHETIC_UNEXPECTED_PROJECT_INSTRUCTIONS"
SYNTHETIC_KEY = "airk-loopback-probe-no-live-credential"
DYNAMIC_NAMESPACE = "airk_eval"
SYNTHETIC_TASK = "Synthetic isolation preflight."
SYNTHETIC_VERDICT = {"verdict": "correct"}
SYNTHETIC_SCHEMA = {
    "type": "object", "properties": {"verdict": {"type": "string", "enum": ["correct", "wrong"]}},
    "required": ["verdict"], "additionalProperties": False,
}


def dynamic_specs(tool_catalog):
    return [{"type": "namespace", "name": DYNAMIC_NAMESPACE, "description": "AIRK indexed evidence", "tools": [
        {"type": "function", "name": tool["name"], "description": tool["description"],
         "inputSchema": tool["inputSchema"], "deferLoading": False}
        for tool in tool_catalog
    ]}] if tool_catalog else []


def dispatch_tool(message, thread_id, turn_id, dispatch, allowed, attempts):
    params = message["params"]
    call_id = params["callId"]
    rpc_id = message["id"]
    common.require(params["threadId"] == thread_id and params["turnId"] == turn_id, "crossed dynamic tool call")
    common.require(params.get("namespace") == DYNAMIC_NAMESPACE and params["tool"] in allowed and dispatch,
                   "unexpected dynamic tool call")
    common.require(isinstance(call_id, str) and call_id and type(rpc_id) in (str, int),
                   "invalid dynamic tool call identity")
    common.require(not any(row["call_id"] == call_id or row["rpc_id"] == rpc_id for row in attempts),
                   "duplicate dynamic tool call")
    tool, arguments = params["tool"], params["arguments"]
    reply = dispatch(tool, arguments)
    content = reply["content"]
    common.require(all(block["type"] == "text" and isinstance(block["text"], str) for block in content),
                   "unsupported wrapper content type")
    result = {"success": not reply.get("isError", False),
              "contentItems": [{"type": "inputText", "text": block["text"]} for block in content]}
    attempts.append({"call_id": call_id, "rpc_id": rpc_id, "tool": tool, "arguments": arguments,
                     "success": result["success"], "output_sha256": search.digest(result["contentItems"])})
    return {"id": rpc_id, "result": result}


def offered_tools(request):
    """Project both Responses tool carriers without retaining the prompt."""
    carriers = [("tools", request.get("tools", []))]
    carriers.extend(
        ("additional_tools", item["tools"])
        for item in request.get("input", [])
        if item.get("type") == "additional_tools"
    )
    tools = []
    for carrier, definitions in carriers:
        for definition in definitions:
            namespace = definition.get("name") if definition.get("type") == "namespace" else None
            entries = definition["tools"] if namespace else [definition]
            if namespace and not entries:
                entries = [definition]
            for tool in entries:
                name = tool.get("name")
                if namespace and name and tool is not definition:
                    name = f"{namespace}__{name}"
                tools.append({"carrier": carrier, "type": tool.get("type"), "name": name,
                              "schema_sha256": search.digest(tool)})
    return tools


def authorization_isolated(offer, expect_synthetic_key):
    expected = (expect_synthetic_key, expect_synthetic_key)
    observed = (offer["authorization_present"], offer["synthetic_authorization"])
    return observed == expected


class Capture(BaseHTTPRequestHandler):
    def log_message(self, *_):
        # Preserve only projections of request shapes. Never log HTTP headers.
        pass

    def do_GET(self):
        self.server.traffic.append({"method": "GET", "models": self.path.startswith("/v1/models")})
        self.send_error(404)

    def scripted_response(self, request):
        index = len(self.server.offers) - 1
        calls = self.server.scripted_calls
        if index > len(calls):
            return self.send_error(400)
        if index:
            call_id = calls[index - 1]["call_id"]
            outputs = [item["output"] for item in request["input"]
                       if item["type"] == "function_call_output" and item["call_id"] == call_id]
            self.server.returned_outputs.append({"call_id": call_id, "outputs": outputs})
        response_id = f"response-{index}"
        item = calls[index] if index < len(calls) else {
            "type": "message", "id": "msg_final", "role": "assistant", "phase": "final_answer",
            "content": [{"type": "output_text", "text": self.server.expected_final, "annotations": []}],
        }
        events = [
            {"type": "response.created", "response": {"id": response_id}},
            {"type": "response.output_item.done", "item": item},
            {"type": "response.completed", "response": {"id": response_id, "status": "completed"}},
        ]
        body = "".join(f"data: {json.dumps(event)}\n\n" for event in events).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        self.server.traffic.append({"method": "POST", "responses": self.path.startswith("/v1/responses")})
        size = int(self.headers.get("Content-Length", "0"))
        if self.path != "/v1/responses" or not 0 < size <= 4 * 1024 * 1024:
            self.send_error(400)
            return
        body = self.rfile.read(size)
        request = json.loads(body)
        tools = offered_tools(request)
        self.server.offers.append({
            "model": request.get("model"), "reasoning_effort": (request.get("reasoning") or {}).get("effort"),
            "tools": tools, "request_fields": sorted(request), "authorization_present": bool(self.headers.get("Authorization")),
            "synthetic_authorization": self.headers.get("Authorization") == ("Bearer " + SYNTHETIC_KEY),
            "project_canary_present": CANARY.encode("utf-8") in body,
            "repository_instructions_present": b"Working on AIRK" in body,
        })
        if self.server.scripted_calls is not None:
            self.server.wire_formats.append((request.get("text") or {}).get("format"))
            return self.scripted_response(request)
        response = b'{"error":{"message":"AIRK synthetic offer capture complete","type":"invalid_request_error"}}'
        self.send_response(400)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)


def run_app_server(cmd, env, workspace, *, model, effort, instruction, tool_catalog,
                   clean_home, dispatch=None, output_schema=None, base_instructions=None, timeout=45):
    tools = dynamic_specs(tool_catalog)
    allowed = {tool["name"] for tool in tool_catalog}
    records = []
    errors = []
    status = {"stage": "initialize", "configured_mcp": None, "rpc_error": None}
    messages = Queue()
    deadline = monotonic() + timeout
    attempts = []
    status["attempts"] = attempts
    thread_id = turn_id = None
    with subprocess.Popen(cmd, env=env, cwd=workspace, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0)) as process:
        def read_messages():
            for line in process.stdout:
                if len(line) <= 1024 * 1024:
                    messages.put(line)
            messages.put(None)

        def read_errors():
            errors.append(process.stderr.read())

        reader = Thread(target=read_messages, daemon=True)
        stderr = Thread(target=read_errors, daemon=True)
        reader.start()
        stderr.start()

        def write(message):
            process.stdin.write(json.dumps(message).encode("utf-8") + b"\n")
            process.stdin.flush()

        def send(method, params, key=None):
            request = {"method": method, "params": params}
            if key is not None:
                request["id"] = key
            write(request)

        def receive(key, method=None):
            while (remaining := deadline - monotonic()) > 0:
                line = messages.get(timeout=remaining)
                if line is None:
                    raise RuntimeError("app-server exited before the expected response")
                records.append(line)
                message = json.loads(line)
                if key is not None and message.get("id") == key and "method" not in message:
                    if "error" in message:
                        error = message["error"]
                        status["rpc_error"] = {"code": error.get("code"), "kind": [
                            marker for marker in ("unknown field", "invalid", "disabled", "not found",
                                                  "unsupported", "missing field", "expected", "experimental",
                                                  "environment", "permission", "reserved", "namespace")
                            if marker in str(error.get("message", "")).lower()
                        ]}
                    return message["result"]
                event = message.get("method")
                if event and "id" in message:
                    common.require(event == "item/tool/call", "unexpected app-server request")
                    write(dispatch_tool(message, thread_id, turn_id, dispatch, allowed, attempts))
                if event == "item/completed":
                    params = message["params"]
                    item = params["item"]
                    if (params["threadId"] == thread_id and params["turnId"] == turn_id
                            and item["type"] == "agentMessage" and item.get("phase") != "commentary"):
                        status["final"] = item["text"]
                if method and event == method:
                    return message["params"]
            raise Empty

        code = None
        try:
            send("initialize", {"clientInfo": {"name": "airk-synthetic-preflight", "version": "1"},
                                "capabilities": {"experimentalApi": True}}, 1)
            receive(1)
            send("initialized", {})
            status["stage"] = "configuration"
            send("config/read", {"cwd": str(workspace), "includeLayers": False}, 4)
            effective = receive(4)["config"]
            configured = effective.get("mcp_servers") or {}
            status["configured_mcp"] = {name: spec.get("enabled", True) for name, spec in configured.items()}
            if clean_home:
                common.require(not configured, "app-server inherited an MCP server")
            else:
                common.require(not any(status["configured_mcp"].values()), "app-server inherited an enabled MCP server")
            status["stage"] = "thread/start"
            thread = {"model": model, "cwd": str(workspace), "approvalPolicy": "never",
                      "sandbox": "read-only", "environments": [], "ephemeral": True,
                      "dynamicTools": tools, "config": {"mcp_servers": {}}}
            if base_instructions is not None:
                thread["baseInstructions"] = base_instructions
            send("thread/start", thread, 2)
            started = receive(2)
            common.require(started["model"] == model and started.get("reasoningEffort") == effort,
                           "app-server substituted a different model or effort")
            thread_id = status["thread_id"] = started["thread"]["id"]
            status["instructions_count"] = len(started.get("instructionSources") or [])
            common.require(not status["instructions_count"], "app-server inherited an instruction source")
            status["stage"] = "turn/start"
            turn = {"threadId": thread_id, "effort": effort, "input": [{"type": "text", "text": instruction}]}
            if output_schema is not None:
                turn["outputSchema"] = output_schema
            send("turn/start", turn, 3)
            turn_id = status["turn_id"] = receive(3)["turn"]["id"]
            status["stage"] = "turn/completed"
            completed = receive(None, "turn/completed")
            common.require(completed["threadId"] == thread_id and completed["turn"]["id"] == turn_id,
                           "crossed terminal turn")
            status["turn_status"] = completed["turn"]["status"]
            code = 0 if status["turn_status"] == "completed" else 1
        except Empty:
            pass
        except common.HarnessError as error:
            status["protocol_error"] = str(error)
            code = process.poll() or 2
        except (KeyError, RuntimeError, ValueError, OSError):
            code = process.poll() or 2
        finally:
            process.stdin.close()
            try:
                process.wait(timeout=4)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=4)
            reader.join(timeout=2)
            stderr.join(timeout=2)
    return code, b"".join(records), b"".join(errors), status


def synthetic_backend(tool, arguments):
    text = json.dumps({"synthetic": tool, "arguments": arguments}, sort_keys=True)
    envelope = {"content": [{"type": "text", "text": text}]}
    return common.CallOutcome(True, 0, json.dumps(envelope).encode("utf-8"), b"", 0, envelope, None)


def scripted_calls(path):
    tool = common.PATH_TOOLS[path][0]
    field = common.PRIMARY_ARG[tool]
    arguments = [{field: "wrong first call"}, {field: SYNTHETIC_TASK}]
    arguments.extend({field: f"synthetic follow-up {n}"} for n in range(1, common.TURN_CAP + 1))
    return [{"type": "function_call", "call_id": f"call-{n:02d}", "namespace": DYNAMIC_NAMESPACE,
             "name": tool, "arguments": json.dumps(argument)}
            for n, argument in enumerate(arguments, start=1)]


def matches_wrapper(server, status, rows):
    attempts = (status or {}).get("attempts", [])
    script = server.scripted_calls
    if not len(attempts) == len(rows) == len(script) == len(server.returned_outputs):
        return False
    for n, (attempt, row, expected, wire) in enumerate(zip(attempts, rows, script, server.returned_outputs), 1):
        refused = n in (1, len(script))
        if not (attempt["call_id"] == expected["call_id"] == wire["call_id"]
                and attempt["tool"] == row["tool"] == expected["name"]
                and attempt["arguments"] == row["arguments"] == json.loads(expected["arguments"])
                and row["call"] == n and row["rejected"] is refused
                and row["turn"] == (None if refused else n - 1)
                and attempt["success"] is not refused and len(wire["outputs"]) == 1):
            return False
        content = wire["outputs"][0]
        if isinstance(content, str):
            content = [{"type": "inputText", "text": content}]
        elif isinstance(content, list):
            content = [{**block, "type": "inputText"} if block.get("type") == "input_text" else block
                       for block in content]
        if search.digest(content) != attempt["output_sha256"]:
            return False
    return True


def probe(args):
    scratch = args.scratch.resolve(strict=True)
    workspace, logs, state, temporary = (scratch / key for key in ("workspace", "logs", "state", "tmp"))
    common.require(all(path.is_dir() for path in (workspace, logs, state, temporary)), "preflight directories missing")
    private_home = scratch / "codex-home"
    if args.clean_app_server_home:
        common.require(private_home.is_dir() and not private_home.is_symlink()
                       and private_home.resolve(strict=True) == private_home
                       and not any(private_home.iterdir()), "clean app-server requires an empty scratch/codex-home")
    with ThreadingHTTPServer(("127.0.0.1", 0), Capture) as server:
        server.offers = []
        server.traffic = []
        server.scripted_calls = None
        server.returned_outputs = []
        server.wire_formats = []
        tool_catalog = []
        wrapper = None
        output_schema = None
        if args.app_server and args.path:
            catalog = common.load_catalog(scratch / "fixture" / "tools-catalog.json")
            if args.round_trip:
                wrapper = mcp_wrapper.Wrapper(handle=None, repo="airk", path=args.path, instruction=SYNTHETIC_TASK,
                                              catalog=catalog, log_dir=scratch / "wrapper", backend=synthetic_backend)
                tool_catalog = wrapper.list_tools()
                server.scripted_calls = scripted_calls(args.path)
                server.expected_final = "Synthetic brief complete."
            else:
                tool_catalog = [{"name": name, "description": catalog[name]["description"],
                                 "inputSchema": common.agent_schema(name, catalog[name]["inputSchema"])}
                                for name in common.PATH_TOOLS[args.path]]
        elif args.round_trip:
            output_schema = SYNTHETIC_SCHEMA
            server.scripted_calls = []
            server.expected_final = json.dumps(SYNTHETIC_VERDICT)
        worker = Thread(target=server.serve_forever, daemon=True)
        worker.start()
        provider = "airk_loopback_probe"
        options = {
            "model_provider": provider, f"model_providers.{provider}.name": "AIRK synthetic capture",
            f"model_providers.{provider}.base_url": f"http://127.0.0.1:{server.server_port}/v1",
            f"model_providers.{provider}.requires_openai_auth": False,
            f"model_providers.{provider}.supports_websockets": False,
            f"model_providers.{provider}.request_max_retries": 0,
            f"model_providers.{provider}.stream_max_retries": 0,
            "model_reasoning_effort": "medium", "web_search": "disabled",
            "features.shell_tool": False, "features.view_image": False, "project_doc_max_bytes": 0,
            "features.apps": False, "features.multi_agent": False, "features.mcp_lazy_stdio_startup": False,
            "features.code_mode.enabled": False, "features.multi_agent_v2.disable_native_tools": True,
            "agents.enabled": False, "tools.experimental_request_user_input.enabled": False,
            "features.goals": False, "features.plugins": False, "features.remote_plugin": False,
            "history.persistence": "none", "analytics.enabled": False, "otel.metrics_exporter": "none",
            "log_dir": logs.as_posix(), "sqlite_home": state.as_posix(),
        }
        if args.model_catalog:
            options["model_catalog_json"] = args.model_catalog.resolve(strict=True).as_posix()
        if args.built_in_provider:
            options = {key: value for key, value in options.items() if not key.startswith("model_providers.")}
            endpoint = f"http://127.0.0.1:{server.server_port}"
            options.update({"model_provider": "openai", "openai_base_url": endpoint + "/v1",
                            "chatgpt_base_url": endpoint, "cli_auth_credentials_store": "ephemeral",
                            "forced_login_method": "api"})
        if args.path and not args.app_server:
            fixture = scratch / "fixture"
            wrapper = [
                "-I", "-B", str(HERE / "mcp_wrapper.py"), "--handle", str(fixture / "lifetime" / "handle.json"),
                "--repo", "bookbuilder", "--path", args.path, "--instruction-file", str(fixture / "instruction.txt"),
                "--log-dir", str(scratch / "wrapper"), "--catalog", str(fixture / "tools-catalog.json"),
            ]
            options.update({
                "mcp_servers.airk.command": sys.executable, "mcp_servers.airk.args": wrapper,
                "mcp_servers.airk.required": True, "mcp_servers.airk.startup_readiness": "catalog",
                "mcp_servers.airk.enabled_tools": common.PATH_TOOLS[args.path],
                "mcp_servers.airk.omit_tools_from": ["deferred", "code_mode"],
            })
        if args.app_server:
            cmd = [str(args.codex), "app-server", "--stdio", "--strict-config"]
            for name in args.disable_mcp_server:
                cmd += ["-c", f"mcp_servers.{name}.enabled=false"]
        else:
            cmd = [str(args.codex), "-a", "never", "exec", "--ignore-user-config", "--ignore-rules", "--strict-config",
                   "--ephemeral", "--skip-git-repo-check", "--sandbox", "read-only", "--json",
                   "--model", args.model, "-C", str(workspace)]
        for key, value in options.items():
            cmd += ["-c", f"{key}={json.dumps(value)}"]
        if not args.app_server:
            cmd += ["Synthetic isolation preflight. Return a short verdict."]
        env = {key: value for key, value in os.environ.items()
               if key.upper() not in {"OPENAI_API_KEY", "CODEX_API_KEY", "OPENAI_BASE_URL"}}
        env.update({"TEMP": str(temporary), "TMP": str(temporary), "TMPDIR": str(temporary),
                    "NO_PROXY": "127.0.0.1,localhost"})
        if args.clean_app_server_home:
            env["CODEX_HOME"] = str(private_home)
        elif args.built_in_provider:
            env["CODEX_API_KEY"] = SYNTHETIC_KEY
        status = None
        try:
            if args.app_server:
                exit_code, stdout, stderr, status = run_app_server(
                    cmd, env, workspace, model=args.model, effort="medium", instruction=SYNTHETIC_TASK,
                    tool_catalog=tool_catalog, clean_home=args.clean_app_server_home,
                    dispatch=wrapper.call if wrapper else None, output_schema=output_schema, timeout=60)
            else:
                completed = subprocess.run(cmd, env=env, cwd=workspace, stdin=subprocess.DEVNULL,
                                           capture_output=True, timeout=45,
                                           creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
                exit_code, stdout, stderr = completed.returncode, completed.stdout, completed.stderr
        except subprocess.TimeoutExpired as error:
            exit_code, stdout, stderr = None, error.stdout or b"", error.stderr or b""
        finally:
            server.shutdown()
            worker.join()
    exercise = None
    if args.round_trip:
        rows = common.read_jsonl(wrapper.turn_log) if wrapper else []
        final = (status or {}).get("final")
        wire_format = server.wire_formats[-1] if server.wire_formats else None
        matched = (exit_code == 0 and final == server.expected_final
                   and len(server.offers) == len(server.scripted_calls) + 1)
        if wrapper:
            matched = matched and wrapper.counted == common.TURN_CAP and matches_wrapper(server, status, rows)
        else:
            matched = (matched and not (status or {}).get("attempts")
                       and (wire_format or {}).get("type") == "json_schema"
                       and (wire_format or {}).get("strict") is True
                       and (wire_format or {}).get("schema") == SYNTHETIC_SCHEMA
                       and json.loads(final) == SYNTHETIC_VERDICT)
        exercise = {"matched": bool(matched), "attempted": len(rows),
                    "admitted": wrapper.counted if wrapper else 0,
                    "refused": sum(row["rejected"] for row in rows),
                    "returned_outputs": server.returned_outputs, "wire_format": wire_format}
    diagnostic = (stdout + b"\n" + stderr).lower()
    markers = ("thread.started", "turn.started", "turn.failed", "reconnecting", "sandbox", "mcp",
               "metadata", "provider", "authentication", "configuration", "synthetic offer capture complete",
               "mcp client for", "mcp startup: failed", "tools/list", "invalid type", "failed to initialize",
               "mcp server startup timed out", "handshaking", "transport closed", "failed to deserialize")
    return {"schema": "airk.private.codex-isolation-probe/2",
            "codex_sha256": common.sha256_file(args.codex), "requested_model": args.model, "path": args.path,
            "built_in_provider": args.built_in_provider, "app_server": args.app_server, "app_server_status": status,
            "model_catalog_sha256": args.model_catalog_sha256,
            "clean_app_server_home": args.clean_app_server_home,
            "codex_exit": exit_code,
            "timed_out": exit_code is None, "traffic": server.traffic, "round_trip": exercise,
            "offers": server.offers, "diagnostic_markers": [marker for marker in markers if marker.encode() in diagnostic],
            "stdout_bytes": len(stdout), "stdout_sha256": common.sha256_bytes(stdout),
            "stderr_bytes": len(stderr), "stderr_sha256": common.sha256_bytes(stderr)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--codex", type=Path, required=True)
    parser.add_argument("--model", choices=("gpt-6.2-sol", "gpt-6.2-astra"), required=True)
    parser.add_argument("--path", choices=("answer", "parse", "search"))
    parser.add_argument("--built-in-provider", action="store_true")
    parser.add_argument("--app-server", action="store_true")
    parser.add_argument("--disable-mcp-server", action="append", default=[])
    parser.add_argument("--clean-app-server-home", action="store_true",
                        help="use a precreated empty scratch/codex-home without an auth credential")
    parser.add_argument("--codex-sha256")
    parser.add_argument("--round-trip", action="store_true", help="exercise a synthetic tool or judge turn")
    parser.add_argument("--model-catalog", type=Path)
    parser.add_argument("--model-catalog-sha256")
    parser.add_argument("--scratch", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    common.require(bool(args.model_catalog) == bool(args.model_catalog_sha256),
                   "supply both the local model catalog and its SHA-256")
    common.require(all(name and all(c.isascii() and (c.isalnum() or c in "_-") for c in name)
                       for name in args.disable_mcp_server), "invalid MCP server name")
    common.require(not args.app_server or (args.built_in_provider and args.model_catalog),
                   "app-server preflight requires the pinned catalog and built-in OpenAI provider")
    if args.clean_app_server_home:
        common.require(args.app_server and args.codex_sha256 and not args.disable_mcp_server,
                       "clean home requires app-server and pinned Codex, with no MCP overrides")
    if args.round_trip:
        common.require(args.clean_app_server_home, "round-trip requires clean app-server home")
        common.require(args.path is None or args.path == "search", "round-trip expects Search or a tool-free judge")
    if args.codex_sha256:
        common.require(common.sha256_file(args.codex) == args.codex_sha256, "Codex executable SHA-256 differs")
    if args.model_catalog:
        search.pinned(args.model_catalog, args.model_catalog_sha256)
    report = probe(args)
    namespace = DYNAMIC_NAMESPACE if args.app_server else "mcp__airk"
    expected_tools = sorted(f"{namespace}__{name}" for name in common.PATH_TOOLS[args.path]) if args.path else []
    report["expected_tools"] = expected_tools
    expect_synthetic_key = args.built_in_provider and not args.clean_app_server_home
    offers = report["offers"]
    report["matched"] = (
        (bool(offers) and report["round_trip"]["matched"] if args.round_trip else len(offers) == 1)
        and (not args.clean_app_server_home or (report["app_server_status"] or {}).get("configured_mcp") == {})
        and all(offer["model"] == args.model
                and offer["reasoning_effort"] == "medium"
                and authorization_isolated(offer, expect_synthetic_key)
                and not any(offer[key] for key in ("project_canary_present", "repository_instructions_present"))
                and all(item["type"] == "function" and item["name"] for item in offer["tools"])
                and sorted(item["name"] for item in offer["tools"]) == expected_tools for offer in offers)
    )
    search.write_new(args.output, report)
    print(json.dumps(report))
    return 0 if report["matched"] else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (common.HarnessError, OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
