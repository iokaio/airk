# SPDX-License-Identifier: Apache-2.0
"""Inspect the tool offer of a synthetic, loopback-only Codex judge request."""

from __future__ import annotations

import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import sys
from threading import Thread

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import search_focus_ab as search


CANARY = "AIRK_SYNTHETIC_UNEXPECTED_PROJECT_INSTRUCTIONS"
SYNTHETIC_KEY = "airk-loopback-probe-no-live-credential"


class Capture(BaseHTTPRequestHandler):
    def log_message(self, *_):
        # Preserve only projections of request shapes. Never log HTTP headers.
        pass

    def do_GET(self):
        self.server.traffic.append({"method": "GET", "models": self.path.startswith("/v1/models")})
        self.send_error(404)

    def do_POST(self):
        self.server.traffic.append({"method": "POST", "responses": self.path.startswith("/v1/responses")})
        size = int(self.headers.get("Content-Length", "0"))
        if self.path != "/v1/responses" or not 0 < size <= 4 * 1024 * 1024:
            self.send_error(400)
            return
        body = self.rfile.read(size)
        request = json.loads(body)
        tools = [
            {"type": tool.get("type"), "name": tool.get("name"), "schema_sha256": search.digest(tool),
             "child_names": [child.get("name") for child in tool.get("tools", [])]}
            for tool in request.get("tools", [])
        ]
        self.server.offers.append({
            "model": request.get("model"), "reasoning_effort": (request.get("reasoning") or {}).get("effort"),
            "tools": tools, "request_fields": sorted(request), "authorization_present": bool(self.headers.get("Authorization")),
            "synthetic_authorization": self.headers.get("Authorization") == ("Bearer " + SYNTHETIC_KEY),
            "project_canary_present": CANARY.encode("utf-8") in body,
            "repository_instructions_present": b"Working on AIRK" in body,
        })
        response = b'{"error":{"message":"AIRK synthetic offer capture complete","type":"invalid_request_error"}}'
        self.send_response(400)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)


def probe(args):
    scratch = args.scratch.resolve(strict=True)
    workspace, logs, state, temporary = (scratch / key for key in ("workspace", "logs", "state", "tmp"))
    common.require(all(path.is_dir() for path in (workspace, logs, state, temporary)), "preflight directories missing")
    with ThreadingHTTPServer(("127.0.0.1", 0), Capture) as server:
        server.offers = []
        server.traffic = []
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
            "features.code_mode.enabled": False,
            "history.persistence": "none", "analytics.enabled": False, "otel.metrics_exporter": "none",
            "log_dir": logs.as_posix(), "sqlite_home": state.as_posix(),
        }
        if args.built_in_provider:
            options = {key: value for key, value in options.items() if not key.startswith("model_providers.")}
            endpoint = f"http://127.0.0.1:{server.server_port}"
            options.update({"model_provider": "openai", "openai_base_url": endpoint + "/v1",
                            "chatgpt_base_url": endpoint, "cli_auth_credentials_store": "ephemeral",
                            "forced_login_method": "api"})
        if args.path:
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
        cmd = [str(args.codex), "-a", "never", "exec", "--ignore-user-config", "--ignore-rules", "--strict-config",
               "--ephemeral", "--skip-git-repo-check", "--sandbox", "read-only", "--json",
               "--model", args.model, "-C", str(workspace)]
        for key, value in options.items():
            cmd += ["-c", f"{key}={json.dumps(value)}"]
        cmd += ["Synthetic isolation preflight. Return a short verdict."]
        env = {key: value for key, value in os.environ.items()
               if key.upper() not in {"OPENAI_API_KEY", "CODEX_API_KEY", "OPENAI_BASE_URL"}}
        env.update({"TEMP": str(temporary), "TMP": str(temporary), "TMPDIR": str(temporary),
                    "NO_PROXY": "127.0.0.1,localhost"})
        if args.built_in_provider:
            env["CODEX_API_KEY"] = SYNTHETIC_KEY
        try:
            completed = subprocess.run(cmd, env=env, cwd=workspace, stdin=subprocess.DEVNULL,
                                       capture_output=True, timeout=45,
                                       creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            exit_code, stdout, stderr = completed.returncode, completed.stdout, completed.stderr
        except subprocess.TimeoutExpired as error:
            exit_code, stdout, stderr = None, error.stdout or b"", error.stderr or b""
        finally:
            server.shutdown()
            worker.join()
    diagnostic = (stdout + b"\n" + stderr).lower()
    markers = ("thread.started", "turn.started", "turn.failed", "reconnecting", "sandbox", "mcp",
               "metadata", "provider", "authentication", "configuration", "synthetic offer capture complete",
               "mcp client for", "mcp startup: failed", "tools/list", "invalid type", "failed to initialize",
               "mcp server startup timed out", "handshaking", "transport closed", "failed to deserialize")
    return {"schema": "airk.private.codex-isolation-probe/1",
            "codex_sha256": common.sha256_file(args.codex), "requested_model": args.model, "path": args.path,
            "built_in_provider": args.built_in_provider, "codex_exit": exit_code,
            "timed_out": exit_code is None, "traffic": server.traffic,
            "offers": server.offers, "diagnostic_markers": [marker for marker in markers if marker.encode() in diagnostic],
            "stdout_bytes": len(stdout), "stdout_sha256": common.sha256_bytes(stdout),
            "stderr_bytes": len(stderr), "stderr_sha256": common.sha256_bytes(stderr)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--codex", type=Path, required=True)
    parser.add_argument("--model", choices=("gpt-6.2-sol", "gpt-6.2-astra"), required=True)
    parser.add_argument("--path", choices=("answer", "parse", "search"))
    parser.add_argument("--built-in-provider", action="store_true")
    parser.add_argument("--scratch", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = probe(args)
    expected_tools = sorted(f"mcp__airk__{name}" for name in common.PATH_TOOLS[args.path]) if args.path else []
    report["expected_tools"] = expected_tools
    report["matched"] = (
        len(report["offers"]) == 1
        and (offer := report["offers"][0])["model"] == args.model
        and offer["reasoning_effort"] == "medium"
        and offer["authorization_present"] is args.built_in_provider
        and offer["synthetic_authorization"] is args.built_in_provider
        and not any(offer[key] for key in ("project_canary_present", "repository_instructions_present"))
        and all(item["type"] == "function" and item["name"] for item in offer["tools"])
        and sorted(item["name"] for item in offer["tools"]) == expected_tools
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
