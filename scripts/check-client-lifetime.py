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
import uuid


def lines(stream):
    replies = queue.Queue()
    def read():
        for line in stream:
            replies.put(line)
        replies.put(None)
    threading.Thread(target=read, daemon=True).start()
    return replies


class NativeConsole:
    """One actual native administrative peer for context-bound grant review."""
    def __init__(self, launcher, control):
        self.child = subprocess.Popen([str(launcher), "service", "console", "--runtime-dir", str(control)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=sys.stderr,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.replies = queue.Queue(maxsize=2)
        def exact(length):
            chunks = bytearray()
            while len(chunks) < length:
                part = self.child.stdout.read(length - len(chunks))
                if not part:
                    raise EOFError("Native console closed before its complete response")
                chunks.extend(part)
            return chunks
        def read():
            try:
                while True:
                    length = int.from_bytes(exact(4), "big")
                    if not 0 < length <= 65536:
                        raise RuntimeError("Native console response exceeds its frame bound")
                    self.replies.put(json.loads(exact(length)))
            except Exception as error:
                self.replies.put(error)
        threading.Thread(target=read, daemon=True).start()

    def request(self, method):
        identifier = uuid.uuid4().hex
        body = json.dumps({"id": identifier, "method": method}).encode()
        if not 0 < len(body) <= 65536:
            raise RuntimeError("Native console request exceeds its frame bound")
        self.child.stdin.write(len(body).to_bytes(4, "big") + body)
        self.child.stdin.flush()
        reply = self.replies.get(timeout=60)
        if isinstance(reply, Exception):
            raise reply
        if reply.get("id") != identifier:
            raise RuntimeError("Crossed native console reply")
        response = reply["response"]
        if response.get("status") == "failure":
            raise RuntimeError(f"Native console refused fixture request: {response}")
        return response

    def close(self):
        self.child.stdin.close()
        try:
            self.child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.child.kill()  # Only this owned metadata adapter.
            self.child.wait(timeout=10)
            raise RuntimeError("Owned native console did not drain after EOF") from None
        if self.child.returncode:
            raise RuntimeError(f"Owned native console exited {self.child.returncode}")


def receive(replies):
    line = replies.get(timeout=45)
    if line is None:
        raise RuntimeError("Fixture connection closed before its reply")
    return json.loads(line)


def owner(launcher, root, control, selection):
    with subprocess.Popen([str(launcher), "--catalog-session", selection, "mcp"],
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
    console = NativeConsole(launcher, control)
    try:
        return check_connected(launcher, root, control, console.request)
    finally:
        console.close()


def check_connected(launcher, root, control, rpc):
    root.mkdir(exist_ok=True)
    (root / "source").mkdir(exist_ok=True)
    (root / "data").mkdir(exist_ok=True)
    cached = root / "lifetime-scope.json"
    if cached.is_file():
        if cached.stat().st_size > 4096:
            raise RuntimeError("Retained fixture selector exceeds its bound")
        selected = json.loads(cached.read_text(encoding="utf-8"))
        detail = rpc({"catalog_detail": {"entity": {"kind": "index", "id": selected["index_id"]}, "expected_catalog_revision": None}})["detail"]
        index = detail["item"]["value"]
        backing = index["backing"]
        grant = rpc({"catalog_detail": {"entity": {"kind": "grant", "id": selected["grant_id"]}, "expected_catalog_revision": detail["revision"]}})["detail"]["item"]["value"]["grant"]
        if (detail["namespace"] != selected["namespace"] or index["tombstone"] or index["source_import"] != "complete"
            or not Path(backing["root"]).samefile(root / "source") or not Path(backing["data_root"]).samefile(root / "data")
            or backing["collection"] != "lifetime-fixture" or grant["revoked"] or grant["revision"] != selected["expected_grant_revision"]
            or len(grant["memberships"]) != 1 or grant["memberships"][0]["membership"]["index_id"] != selected["index_id"]
            or [grant["memberships"][0]["membership"]["project_id"]] != selected["project_ids"]):
            raise RuntimeError("Retained fixture selector differs from its actual native scope")
        selected["expected_catalog_revision"] = detail["revision"]
        return exercise_clients(launcher, root, control, json.dumps(selected))
    def key():
        epoch = rpc("request_epoch")
        return {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex}
    context_key = uuid.uuid4().hex + uuid.uuid4().hex
    descriptor = rpc({"catalog_host_context": {"key": context_key, "expected_catalog_revision": None}})["descriptor"]
    selection = {"key": key(), "expected_catalog_revision": descriptor["catalog_revision"],
                 "name": "Lifetime fixture", "primary_root": str(root / "source"),
                 "data_root": str(root / "data"), "collection": "lifetime-fixture",
                 "additional_roots": [], "setup_watch": False}
    review = rpc({"catalog_index_setup_draft": {"selection": selection}})
    setup = rpc({"catalog_index_setup_apply": {"draft": review["draft"]}})
    if setup.get("status") != "catalog_index_setup_applied" or setup["receipt"]["stage"] != "ready":
        raise RuntimeError(f"Reviewed native fixture setup was not completed: {setup}")
    receipt = setup["receipt"]
    revision, index, project = receipt["catalog_revision"], receipt["index_id"], "prj_" + uuid.uuid4().hex
    def apply(operation):
        nonlocal revision
        request = {"caller": uuid.uuid4().hex, "key": key(), "expected_catalog_revision": revision, "operation": operation}
        preview = rpc({"catalog_preview": {"request": request}})
        result = rpc({"catalog_apply": {"request": request, "preview_digest": preview["preview_digest"]}})
        if result.get("status") != "catalog_applied":
            raise RuntimeError(f"Reviewed native fixture metadata was not applied: {result}")
        revision = result["receipt"]["catalog_revision"]
    apply({"operation": "create_project", "id": project, "name": "Lifetime Project"})
    apply({"operation": "set_membership", "membership": {"project_id": project, "index_id": index, "revision": 1,
           "scope": [{"source_root_id": receipt["source_ids"][0], "kind": "subtree", "path": ""}]},
           "expected_scope_revision": 1, "expected_membership_revision": None})
    descriptor = rpc({"catalog_host_context": {"key": context_key, "expected_catalog_revision": revision}})["descriptor"]
    grant = rpc({"catalog_grant_draft": {"context_key": context_key, "selection": {"key": key(),
        "expected_catalog_revision": revision, "project_ids": [project], "index_ids": [index]}}})
    if grant.get("status") != "catalog_grant_draft":
        raise RuntimeError(f"Native fixture grant review was refused: {grant}")
    draft = grant["draft"]
    granted = rpc({"catalog_apply": {"request": draft["request"], "preview_digest": draft["preview_digest"]}})
    if granted.get("status") != "catalog_applied":
        raise RuntimeError(f"Reviewed native fixture grant was not applied: {granted}")
    selector = json.dumps({"schema": 1, "context_key": context_key, "namespace": descriptor["namespace"],
        "expected_catalog_revision": granted["receipt"]["catalog_revision"], "grant_id": draft["request"]["operation"]["grant"]["id"],
        "expected_grant_revision": 1, "index_id": index, "project_ids": [project]})
    with cached.open("x", encoding="utf-8") as stream:
        stream.write(selector + "\n")
    return exercise_clients(launcher, root, control, selector)


def exercise_clients(launcher, root, control, selector):
    clients = []
    try:
        for _ in range(2):
            child = subprocess.Popen([sys.executable, __file__, "--owner", "--launcher", str(launcher),
                                      "--catalog-session", selector,
                                      "--root", str(root), "--control-dir", str(control)],
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=sys.stderr,
                                     text=True, encoding="utf-8", creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            clients.append((child, lines(child.stdout)))
        def catalog(client):
            child, replies = client
            child.stdin.write("tools\n")
            child.stdin.flush()
            value = receive(replies)
            if value.get("tools") != 28:
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
    parser.add_argument("--catalog-session")
    args = parser.parse_args()
    arguments = (args.launcher.resolve(strict=True), args.root.resolve(strict=True), args.control_dir.resolve(strict=True))
    if args.owner:
        if not args.catalog_session:
            parser.error("owned MCP process requires the explicitly selected native grant")
        owner(*arguments, args.catalog_session)
    else:
        check(*arguments)
