# SPDX-License-Identifier: Apache-2.0
"""Exercise real MCP EOF and abrupt client loss; only temporary fixture clients are stopped."""
import argparse
import ctypes
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time


def lines(stream):
    replies = queue.Queue()
    def read():
        for line in stream:
            replies.put(line)
        replies.put(None)
    threading.Thread(target=read, daemon=True).start()
    return replies


def receive(replies):
    line = replies.get(timeout=45)
    if line is None:
        raise RuntimeError("Fixture connection closed before its reply")
    return json.loads(line)


def owner(launcher, root, control):
    with subprocess.Popen([str(launcher), "--root", str(root / "source"), "--data-dir", str(root / "data"),
                           "--collection", "lifetime-fixture", "mcp"],
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=sys.stderr,
                          text=True, encoding="utf-8", creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0)) as server:
        replies = lines(server.stdout)
        def send(value):
            server.stdin.write(json.dumps(value) + "\n")
            server.stdin.flush()
        send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "lifetime-fixture", "version": "1"}}})
        if "result" not in receive(replies):
            raise RuntimeError("MCP fixture initialization failed")
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        for request in sys.stdin:
            if request.strip() == "close":
                server.stdin.close()
                code = server.wait(timeout=10)
                print(json.dumps({"closed": code}), flush=True)
                return
            send({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
            tools = receive(replies).get("result", {}).get("tools", [])
            print(json.dumps({"pid": server.pid, "tools": len(tools)}), flush=True)
        server.stdin.close()
        server.wait(timeout=10)


class ObservedProcess:
    def __init__(self, pid):
        self.pid = pid
        self.handle = None
        if os.name == "nt":
            from ctypes import wintypes
            self.kernel = ctypes.WinDLL("kernel32", use_last_error=True)
            self.kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
            self.kernel.OpenProcess.restype = wintypes.HANDLE
            self.kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
            self.kernel.WaitForSingleObject.restype = wintypes.DWORD
            self.kernel.CloseHandle.argtypes = [wintypes.HANDLE]
            self.handle = self.kernel.OpenProcess(0x00100000, False, pid)
            if not self.handle:
                raise ctypes.WinError(ctypes.get_last_error())

    def wait(self):
        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if self.handle:
                    if self.kernel.WaitForSingleObject(self.handle, 0) == 0:
                        return
                else:
                    observed = subprocess.run(["ps", "-p", str(self.pid), "-o", "stat="],
                                              capture_output=True, text=True, timeout=5)
                    # A Unix orphan may briefly await its reaper; it has already exited.
                    if not observed.stdout.strip() or observed.stdout.strip().startswith("Z"):
                        return
                time.sleep(0.05)
            raise RuntimeError("MCP process remained alive after its client exited")
        finally:
            if self.handle:
                self.kernel.CloseHandle(self.handle)
                self.handle = None


def check(launcher, root, control):
    root.mkdir(exist_ok=True)
    (root / "source").mkdir(exist_ok=True)
    (root / "data").mkdir(exist_ok=True)
    def native(arguments, value=None):
        result = subprocess.run([str(launcher), *map(str, arguments)],
                                input=json.dumps(value) if value is not None else None,
                                capture_output=True, text=True, encoding="utf-8", timeout=60,
                                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        if result.returncode:
            raise RuntimeError(f"Fixture registration failed: {result.stderr[-4096:]}")
        return json.loads(result.stdout)
    project = native(["--root", root / "source", "--data-dir", root / "data",
                      "--collection", "lifetime-fixture", "service", "prepare-registration"])
    registered = native(["service", "rpc", "--runtime-dir", control, "--stdin"], {"register": {"project": project}})
    if registered.get("status") != "registered":
        raise RuntimeError(f"Fixture registration was not accepted: {registered}")
    clients = []
    try:
        for _ in range(2):
            child = subprocess.Popen([sys.executable, __file__, "--owner", "--launcher", str(launcher),
                                      "--root", str(root), "--control-dir", str(control)],
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=sys.stderr,
                                     text=True, encoding="utf-8", creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            clients.append((child, lines(child.stdout)))
        def catalog(client):
            child, replies = client
            child.stdin.write("tools\n")
            child.stdin.flush()
            value = receive(replies)
            if value.get("tools") != 26:
                raise RuntimeError("Live MCP connection lost its tool catalog")
            return value["pid"]
        first = ObservedProcess(catalog(clients[0]))
        second = ObservedProcess(catalog(clients[1]))
        clients[0][0].kill()  # This exact fixture client; never the service or a name-based sweep.
        clients[0][0].wait(timeout=10)
        first.wait()
        catalog(clients[1])
        clients[1][0].stdin.write("close\n")
        clients[1][0].stdin.flush()
        if receive(clients[1][1]) != {"closed": 0}:
            raise RuntimeError("Normal MCP EOF did not close cleanly")
        clients[1][0].wait(timeout=10)
        second.wait()
        print("MCP abrupt client loss, concurrent session preservation and normal EOF passed.", flush=True)
    finally:
        for child, _ in clients:
            if child.poll() is None:
                child.kill()
            child.wait(timeout=10)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--launcher", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--control-dir", type=Path, required=True, help="Isolated fixture service control directory")
    parser.add_argument("--owner", action="store_true")
    args = parser.parse_args()
    (owner if args.owner else check)(args.launcher.resolve(strict=True), args.root.resolve(strict=True),
                                    args.control_dir.resolve(strict=True))
