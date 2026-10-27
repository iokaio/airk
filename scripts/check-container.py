# SPDX-License-Identifier: Apache-2.0
"""Smoke-test one container, two collections and persistence using only public fixtures."""
import argparse
import importlib.util
import json
import queue
from pathlib import Path
import subprocess
import sys
import threading
import time
import uuid


spec = importlib.util.spec_from_file_location("service_clients", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


def docker(*args, check=True, timeout=60):
    result = subprocess.run(["docker", *args], check=False, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)
    if check and result.returncode:
        print(result.stderr[-4096:], file=sys.stderr, flush=True)
        result.check_returncode()
    return result


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


def check_stdio(name):
    process = subprocess.Popen(["docker", "exec", "-i", name, "airk", "--root",
                                "/workspaces/project-a", "--data-dir", "/data/indexes",
                                "--collection", "project-a", "mcp"],
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.DEVNULL, text=True)
    responses = queue.Queue()
    threading.Thread(target=lambda: [responses.put(line) for line in process.stdout], daemon=True).start()

    def send(message):
        process.stdin.write(json.dumps(message) + "\n")
        process.stdin.flush()

    def receive(expected, timeout=10):
        deadline = time.monotonic() + timeout
        for _ in range(10):
            value = json.loads(responses.get(timeout=clients.remaining(deadline, timeout)))
            if value.get("id") == expected:
                return value
        raise AssertionError("No matching MCP response")

    try:
        send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "airk-container-check", "version": "1"}}})
        # Debug candidates still hash every approved runtime component before
        # bridge creation. Evidence/control deadlines remain separately bounded.
        assert receive(1, timeout=60)["result"]["serverInfo"]["name"] == "airk"
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        send({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        tools = receive(2)["result"]["tools"]
        expected = {
            "airk_capabilities", "airk_changes", "airk_code_facts", "airk_context",
            "airk_document_facts", "airk_file_info", "airk_help", "airk_imports", "airk_list_files",
            "airk_outline", "airk_read_file", "airk_references", "airk_relationships",
            "airk_search", "airk_status", "airk_symbols",
        }
        assert len(tools) == len(expected) and {tool["name"] for tool in tools} == expected
        request = {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
            "name": "airk_symbols", "arguments": {"name": "authenticate", "response_mode": "full"}}}
        def read(until):
            send(request)
            reply = receive(3, clients.remaining(until, 10))
            assert "error" not in reply, "Container stdio protocol failure"
            return reply["result"]
        result, _ = clients.read_envelope(read, time.monotonic() + 10)
        evidence = json.loads(result["content"][0]["text"])
        assert evidence["collection"] == "project-a" and evidence["items"]
    finally:
        process.stdin.close()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default="airk:dev")
    args = parser.parse_args()
    token = uuid.uuid4().hex
    name = "airk-service-check-" + token[:12]
    volume = name + "-data"
    fixtures = Path(__file__).resolve().parent.parent / "examples/workspaces"
    volume_created = False
    projects = {}

    def start():
        docker("run", "--detach", "--name", name, "--label", f"io.airk.fixture={token}", "--network", "none",
               "--read-only", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
               "--tmpfs", "/tmp", "--mount", f"type=bind,source={fixtures},target=/workspaces,readonly",
               "--mount", f"type=volume,source={volume},target=/data", args.image)

    def command(collection, *command_args, check=True, timeout=60):
        return docker("exec", name, "airk", "--root", "/workspaces/" + collection,
                      "--data-dir", "/data/indexes", "--collection", collection, *command_args, check=check, timeout=timeout)

    def ready():
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            location = docker("exec", name, "airk", "runtime", "location", check=False)
            if location.returncode == 0:
                control = json.loads(location.stdout)["user_paths"]["state"] + "/control"
                health = docker("exec", name, "airk", "service", "health", "--runtime-dir", control, check=False)
                if health.returncode == 0:
                    return control
            time.sleep(0.2)
        raise AssertionError("Container service did not become healthy")

    def wait_job(control, accepted):
        job_id = accepted["job"]["id"]
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            def invoke(exact, until):
                result = subprocess.run(["docker", "exec", "-i", name, "airk", "service", "rpc", "--runtime-dir", control, "--stdin"],
                                        input=exact, text=True, capture_output=True, check=True, timeout=clients.remaining(until, 60))
                return json.loads(result.stdout)
            job = clients.metadata_rpc(invoke, {"job": {"id": job_id}}, deadline)["job"]
            if job["state"] == "succeeded":
                return job
            assert job["state"] not in ("failed", "canceled"), job
            time.sleep(0.2)
        raise AssertionError("Native container job did not finish")

    def tool(collection, name, arguments=None, deadline=None):
        deadline = min(deadline if deadline is not None else float("inf"), time.monotonic() + 60)
        exact = ("tools", "call", name, "--args", json.dumps(arguments or {"response_mode": "full"}))
        def read(until):
            result = command(collection, *exact, check=False, timeout=clients.remaining(until, 60))
            assert result.returncode in (0, 1), f"Container read process failed: exit {result.returncode}"
            envelope = json.loads(result.stdout)
            assert result.returncode == 0 or clients.read_retry_delay(envelope) is not None, "Container read failed without typed rejection"
            return envelope
        envelope, _ = clients.read_envelope(read, deadline)
        return clients.tool_value(envelope)

    def published_status(collection):
        return clients.observe_publication(lambda until: tool(collection, "airk_status", deadline=until),
                                           collection, projects[collection], time.monotonic() + 60)

    try:
        docker("volume", "create", "--label", f"io.airk.fixture={token}", volume)
        assert json.loads(docker("volume", "inspect", volume).stdout)[0]["Labels"].get("io.airk.fixture") == token
        volume_created = True
        start()
        control = ready()
        print("Native container service is healthy.", flush=True)
        for collection in ("project-a", "project-b"):
            registered = json.loads(command(collection, "init").stdout)
            assert registered["registration"]["collection"] == collection
            projects[collection] = registered["registration"]["binding"]
            accepted = json.loads(command(collection, "index").stdout)
            wait_job(control, accepted)
            print(f"Indexed isolated collection {collection}.", flush=True)
        a = tool("project-a", "airk_symbols", {"name": "authenticate", "response_mode": "full"})
        b = tool("project-b", "airk_symbols", {"name": "authenticate", "response_mode": "full"})
        assert a["items"] and not b["items"]
        assert a["collection"] == "project-a" and b["collection"] == "project-b"
        install = json.loads(docker("exec", name, "airk", "runtime", "location").stdout)["install_dir"]
        client = json.loads(docker("exec", name, "airk", "runtime", "prepare-client", "--install-dir", install).stdout)
        assert client["launcher"] == install + "/airk-launcher" and client["service_lifecycle_changed"] is False
        pin = json.loads(docker("exec", name, "airk", "runtime", "inspect", "--install-dir", install).stdout)
        assert pin["managed_policy"]["lifecycle"] == "external" and pin["launcher"] == client["launcher"]
        assert docker("exec", name, client["launcher"], "--version").stdout.strip() == "airk " + pin["approval"]["version"]
        check_stdio(name)
        print("Read-only stdio MCP and collection isolation passed.", flush=True)
        docker("exec", name, "test", "-f", "/usr/lib/airk/notices/inventory.json")
        docker("exec", name, "test", "-x", "/usr/lib/airk/airk-embed")
        assert docker("inspect", "--format", "{{.HostConfig.NetworkMode}}", name).stdout.strip() == "none"
        identity = owned_container(name, token)
        assert identity is not None, "Owned container disappeared before restart"
        docker("stop", identity)
        remove_owned_container(name, token)
        start()
        ready()
        assert published_status("project-a")["files"] == 2
        # Retired direct-store deletion cannot remove either service-owned collection.
        assert command("project-a", "collections", "remove", "project-b", check=False).returncode != 0
        assert (fixtures / "project-b/README.md").is_file()
        assert published_status("project-b")["files"] > 0
        assert published_status("project-a")["files"] == 2
    finally:
        if volume_created:
            # Explicit removal waits for this container to release its mounts.
            # --rm can acknowledge stop before asynchronous removal completes.
            remove_owned_container(name, token)
            assert json.loads(docker("volume", "inspect", volume).stdout)[0]["Labels"].get("io.airk.fixture") == token
            docker("volume", "rm", volume)
    print("Container smoke passed: stdio MCP, two isolated collections, offline/read-only runtime, persistent restart, paired helper, notices and refusal of retired direct writers.")


if __name__ == "__main__":
    main()
