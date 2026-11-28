# SPDX-License-Identifier: Apache-2.0
"""Model-callable, session-scoped verification for the end-to-end experiment."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import threading
import time

import common
import mcp_wrapper

HERE = Path(__file__).resolve().parent
NAME = "mcp__verification__run_tests"
RAW_LIMIT = 2 * 1024 * 1024
DISPLAY_LIMIT = 48 * 1024
TIMEOUT = 120
NODE_MODULE = re.compile(r"pro/src/WritingForgePro-ui/src/(?:[A-Za-z0-9_-]+/)*[A-Za-z0-9_-]+\.ts")


def profile(path):
    data = common.read_json(path)
    executable = Path(data["runtime"]).resolve(strict=True)
    if not executable.is_file():
        raise ValueError("Verification runtime must be a file")
    if data["kind"] == "pytest":
        if executable.name.lower() not in ("python.exe", "python", "python3") or not data.get("modules"):
            raise ValueError("Provide the tested Python runtime and public pytest modules")
        for module in data["modules"]:
            if not re.fullmatch(r"clients/python/tests/(?:[A-Za-z0-9_]+/)*test_[A-Za-z0-9_]+\.py", module):
                raise ValueError(f"Invalid public pytest module: {module}")
    elif data["kind"] == "node-cases":
        if executable.name.lower() not in ("node.exe", "node") or not NODE_MODULE.fullmatch(data.get("module", "")) or not re.fullmatch(r"[A-Za-z_$][A-Za-z0-9_$]*", data.get("export", "")):
            raise ValueError("Invalid Bookbuilder verification runtime, module or export")
        cases = data["cases"]
        if not isinstance(cases, list) or not cases:
            raise ValueError("Provide public Node contract examples")
        for case in cases:
            if not isinstance(case["name"], str) or not case["name"].strip() or not isinstance(case["args"], list) or "expected" not in case:
                raise ValueError("Invalid public Node case")
        if len({c["name"] for c in cases}) != len(cases):
            raise ValueError("Public Node cases must have distinct names")
    else:
        raise ValueError("Unknown verification profile")
    return dict(data, runtime=str(executable))


def selection(data, args):
    if not isinstance(args, dict) or set(args) - {"selectors"}:
        raise ValueError("Expected an optional selectors array")
    selected = args.get("selectors", [])
    if not isinstance(selected, list) or len(selected) > 32 or any(not isinstance(s, str) or len(s) > 512 for s in selected):
        raise ValueError("Choose up to 32 public selectors")
    if len(set(selected)) != len(selected):
        raise ValueError("Repeated selector")
    if data["kind"] == "pytest":
        for item in selected:
            module, sep, node = item.partition("::")
            if module not in data["modules"] or (sep and not re.fullmatch(r"(?:[A-Za-z_]\w*::)*[A-Za-z_]\w*(?:\[[^\x00-\x1f\\/:\]]+\])?", node)):
                raise ValueError(f"Select a listed module or exact pytest node: {item}")
        return selected or list(data["modules"])
    names = [case["name"] for case in data["cases"]]
    if any(x not in names for x in selected):
        raise ValueError("Select a listed public contract case")
    return selected or names


def guard(mode, session, number=None):
    argv = ["pwsh", "-NoProfile", "-File", str(HERE / "checked_cost_fixture.ps1"),
            "-Mode", mode, "-Session", str(session)]
    if number is not None:
        argv += ["-Call", str(number)]
    subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=sys.stderr, check=True)


class Server:
    def __init__(self, session, data):
        self.session, self.data = session.resolve(strict=True), data
        self.work = self.session / "work"
        if Path(data["runtime"]).resolve().is_relative_to(self.session):
            raise ValueError("Verification runtime must be outside the editable session")
        self.records = self.session / "verification"
        self.records.resolve(strict=True)
        self.calls = max((int(p.name) for p in self.records.iterdir() if p.is_dir() and p.name.isdigit()), default=0)
        choices = selection(data, {})
        label = "public pytest modules or exact module::test node IDs" if data["kind"] == "pytest" else "public Node contract cases"
        self.tool = {"name": "run_tests", "description": f"Run {label} on your current checkout. Omit selectors to run all configured checks. "
                     + "Available: " + ", ".join(choices),
                     "inputSchema": {"type": "object", "additionalProperties": False, "properties": {
                         "selectors": {"type": "array", "maxItems": 32, "uniqueItems": True, "items": {"type": "string"}}}}}

    def execute(self, args):
        selected = selection(self.data, args)
        self.calls += 1
        guard("BeforeVerification", self.session, self.calls)
        folder = self.records / str(self.calls)
        scratch = folder / "tmp"
        # Pass the actual source paths only to the worker. The acting agent selects tests, never argv.
        paths = self.data["modules"] if self.data["kind"] == "pytest" else [self.data["module"]]
        for name in paths:
            path = self.work / name
            if path.exists() and not path.resolve().is_relative_to(self.work):
                raise ValueError("Test input escapes the session checkout")
        exe = self.data["runtime"]
        if self.data["kind"] == "pytest":
            argv = [exe, "-I", "-B", str(HERE / "session_verify_pytest.py")]
            payload = selected
        else:
            worker = HERE / "session_verify_node.mjs"
            subject = (self.work / self.data["module"]).resolve()
            argv = [exe, "--permission", f"--allow-fs-read={worker}", f"--allow-fs-read={subject}",
                    "--experimental-vm-modules", "--disable-warning=ExperimentalWarning", str(worker)]
            cases = {case["name"]: case for case in self.data["cases"]}
            payload = {"module": str(subject), "exported": self.data["export"], "cases": [cases[s] for s in selected]}
        env = {key: os.environ[key] for key in ("SYSTEMROOT", "WINDIR", "COMSPEC", "SystemDrive") if key in os.environ}
        env.update({"TEMP": str(scratch), "TMP": str(scratch), "TMPDIR": str(scratch),
                    "PYTEST_DISABLE_PLUGIN_AUTOLOAD": "1", "NO_COLOR": "1", "PATH": str(Path(exe).parent)})
        start = time.monotonic()
        proc = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                cwd=self.work, env=env, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        buf = bytearray()
        limit = threading.Event()
        def collect():
            while chunk := proc.stdout.read(16384):
                remaining = RAW_LIMIT - len(buf)
                buf.extend(chunk[:remaining])
                if len(chunk) > remaining:
                    limit.set()
                    common.kill_tree(proc.pid)
                    return

        reader = threading.Thread(target=collect, daemon=True)
        reader.start()
        timed_out = False
        try:
            try:
                proc.stdin.write(json.dumps(payload, ensure_ascii=True).encode())
                proc.stdin.close()
            except BrokenPipeError:
                pass
            proc.wait(timeout=TIMEOUT)
        except subprocess.TimeoutExpired:
            timed_out = True
            common.kill_tree(proc.pid)
            proc.wait(timeout=10)
        except BaseException:
            if proc.poll() is None:
                common.kill_tree(proc.pid)
                proc.wait(timeout=10)
            raise
        finally:
            reader.join(timeout=10)
            proc.stdout.close()
        if reader.is_alive():
            raise RuntimeError("Verifier output pipe failed to close")
        raw = bytes(buf)
        (folder / "output.log").write_bytes(raw)
        receipt = {"kind": self.data["kind"], "selectors": selected, "all_configured": selected == selection(self.data, {}),
                   "argv": argv, "exit_code": proc.returncode, "seconds": round(time.monotonic()-start, 3),
                   "timed_out": timed_out, "output_limit_hit": limit.is_set(), "raw_bytes": len(raw),
                   "sha256": hashlib.sha256(raw).hexdigest(), "display_truncated": len(raw) > DISPLAY_LIMIT}
        (folder / "result.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
        guard("AfterVerification", self.session, self.calls)
        shown = raw if len(raw) <= DISPLAY_LIMIT else raw[:DISPLAY_LIMIT//2] + b"\n[... middle of test output omitted ...]\n" + raw[-DISPLAY_LIMIT//2:]
        state = " (timed out)" if timed_out else " (output limit reached)" if limit.is_set() else ""
        text = f"Test exit {proc.returncode}{state}\n{shown.decode('utf-8', errors='replace')}"
        return {"content": [{"type": "text", "text": text}], "isError": bool(proc.returncode or timed_out or limit.is_set())}

    def handle_message(self, message):
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
            return mcp_wrapper._error(None, -32600, "Invalid request")
        if "id" not in message:
            return None
        method, ident, params = message.get("method"), message["id"], message.get("params", {})
        if method == "initialize":
            version = params.get("protocolVersion")
            result = {"protocolVersion": version if version in mcp_wrapper.PROTOCOL_VERSIONS else mcp_wrapper.PROTOCOL_VERSIONS[0],
                      "capabilities": {"tools": {}}, "serverInfo": {"name": "verification", "version": "1"}}
        elif method == "tools/list":
            result = {"tools": [self.tool]}
        elif method == "ping":
            result = {}
        elif method == "tools/call" and isinstance(params, dict) and params.get("name") == "run_tests":
            try:
                result = self.execute(params.get("arguments", {}))
            except (ValueError, OSError, subprocess.SubprocessError) as error:
                result = {"content": [{"type": "text", "text": str(error)}], "isError": True}
        else:
            return mcp_wrapper._error(ident, -32601, "Unknown method or test tool")
        return mcp_wrapper._result(ident, result)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", required=True, type=Path)
    parser.add_argument("--profile", required=True, type=Path)
    args = parser.parse_args()
    mcp_wrapper.serve(Server(args.session, profile(args.profile)), sys.stdin.buffer, sys.stdout.buffer)