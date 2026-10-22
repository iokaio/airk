# SPDX-License-Identifier: Apache-2.0
"""Smoke-test one container, two collections and persistence using only public fixtures."""
import argparse
import json
import queue
from pathlib import Path
import subprocess
import threading
import uuid


def docker(*args, check=True):
    return subprocess.run(["docker", *args], check=check, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)


def remove_owned_container(name):
    result = docker("rm", "--force", name, check=False)
    if result.returncode and not result.stderr.strip().endswith(f"No such container: {name}"):
        result.check_returncode()


def check_stdio(name):
    process = subprocess.Popen(["docker", "exec", "-i", name, "airk", "--root",
                                "/workspaces/project-a", "--data-dir", "/data",
                                "--collection", "project-a", "mcp"],
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.DEVNULL, text=True)
    responses = queue.Queue()
    threading.Thread(target=lambda: [responses.put(line) for line in process.stdout], daemon=True).start()

    def send(message):
        process.stdin.write(json.dumps(message) + "\n")
        process.stdin.flush()

    def receive(expected):
        for _ in range(10):
            value = json.loads(responses.get(timeout=10))
            if value.get("id") == expected:
                return value
        raise AssertionError("No matching MCP response")

    try:
        send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "airk-container-check", "version": "1"}}})
        assert receive(1)["result"]["serverInfo"]["name"] == "airk"
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        send({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        tools = receive(2)["result"]["tools"]
        expected = {
            "airk_capabilities", "airk_changes", "airk_code_facts", "airk_context",
            "airk_document_facts", "airk_file_info", "airk_imports", "airk_list_files",
            "airk_outline", "airk_read_file", "airk_references", "airk_relationships",
            "airk_search", "airk_status", "airk_symbols",
        }
        assert len(tools) == len(expected) and {tool["name"] for tool in tools} == expected
        send({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
            "name": "airk_symbols", "arguments": {"name": "authenticate"}}})
        result = receive(3)["result"]
        assert not result.get("isError")
        evidence = json.loads(result["content"][0]["text"])
        assert evidence["collection"] == "project-a" and evidence["facts"]
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
    name = "airk-scaffold-check-" + uuid.uuid4().hex[:12]
    volume = name + "-data"
    fixtures = Path(__file__).resolve().parent.parent / "examples/workspaces"
    volume_created = False

    def start():
        docker("run", "--detach", "--name", name, "--network", "none",
               "--read-only", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
               "--tmpfs", "/tmp", "--mount", f"type=bind,source={fixtures},target=/workspaces,readonly",
               "--mount", f"type=volume,source={volume},target=/data", args.image)

    def command(collection, *command_args, check=True):
        return docker("exec", name, "airk", "--root", "/workspaces/" + collection,
                      "--data-dir", "/data", "--collection", collection, *command_args, check=check)

    try:
        docker("volume", "create", volume)
        volume_created = True
        start()
        for collection in ("project-a", "project-b"):
            indexed = json.loads(command(collection, "index").stdout)
            assert indexed["collection"] == collection
        a = json.loads(command("project-a", "symbol", "authenticate").stdout)
        b = json.loads(command("project-b", "symbol", "authenticate").stdout)
        assert a["facts"] and not b["facts"]
        assert a["collection"] == "project-a" and b["collection"] == "project-b"
        check_stdio(name)
        usage = json.loads(command("project-a", "collections", "list").stdout)
        assert len(usage) == 2 and all(c["disk_bytes"] > 0 for c in usage)
        docker("exec", name, "test", "-f", "/usr/share/licenses/airk/dependencies/inventory.json")
        assert docker("inspect", "--format", "{{.HostConfig.NetworkMode}}", name).stdout.strip() == "none"
        docker("stop", name)
        remove_owned_container(name)
        start()
        assert json.loads(command("project-a", "status").stdout)["files"] == 2
        removed = json.loads(command("project-a", "collections", "remove", "project-b").stdout)
        assert removed["status"] == "removed-derived-index"
        assert (fixtures / "project-b/README.md").is_file()
        assert command("project-b", "status", check=False).returncode != 0
        assert json.loads(command("project-a", "status").stdout)["files"] == 2
    finally:
        if volume_created:
            # Explicit removal waits for this container to release its mounts.
            # --rm can acknowledge stop before asynchronous removal completes.
            remove_owned_container(name)
            docker("volume", "rm", volume)
    print("Container smoke passed: stdio MCP, two isolated collections, offline/read-only runtime, persistent restart, notices, usage and safe removal.")


if __name__ == "__main__":
    main()
