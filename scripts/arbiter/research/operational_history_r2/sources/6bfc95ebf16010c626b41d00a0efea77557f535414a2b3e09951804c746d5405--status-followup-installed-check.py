# SPDX-License-Identifier: Apache-2.0
"""Verify the selected local payload and bounded MCP reads through its launcher."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("replay", ROOT / "scripts/benchmark-mcp-investigation.py")
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--build", type=Path, required=True)
    parser.add_argument("--install-dir", required=True)
    args = parser.parse_args()
    build = args.build.resolve()
    runtime = build / "runtime"
    binary = runtime / "airk.exe"
    expected = digest(runtime / "airk-install.json")
    inspection = json.loads(subprocess.check_output([
        str(binary), "runtime", "inspect", "--install-dir", args.install_dir
    ], text=True, encoding="utf-8"))
    assert inspection["payload_verified"] and inspection["selection"]["active"] == expected, inspection
    installed = Path(inspection["executable"]).parent
    hashes = {}
    for name in ["airk.exe", "airk-embed.exe", "airk-install.json"]:
        hashes[name] = digest(installed / name)
        assert hashes[name] == digest(runtime / name), name
    health = json.loads(subprocess.check_output([
        str(binary), "runtime", "ensure", "--install-dir", args.install_dir
    ], text=True, encoding="utf-8"))
    assert health["runtime_sha256"] == expected and health["version"] == "1.8.3", health
    configured = tomllib.loads((ROOT / ".codex/config.toml").read_text(encoding="utf-8"))["mcp_servers"]["airk"]
    configuration = {**configured, "cwd": str(ROOT), "expected_version": "1.8.3"}
    observations = []
    with replay.Session(configuration, 30) as session:
        assert session.first_status["observation"]["index_state"] == "available", session.first_status
        for batch in range(10):
            # Invalidate the scalar cache before each batch, without mutations.
            warm, _ = session.tool(("read_file", {"path": "crates/airk-mcp/src/response.rs", "start_line": 94, "end_line": 97}))
            assert warm.get("isError") is not True, warm
            tasks = [("status", {"response_mode": "full"})]
            if batch % 2 == 0:
                tasks += [("search", {"path_prefix": "docs/mcp-recommendations.md", "query": "Session", "mode": mode,
                            "language": "markdown", "evidence_kind": "document", "limit": 3, "max_bytes": 4000})
                          for mode in ["lexical", "vector", "hybrid"]]
            else:
                tasks += [("symbols", {"path_prefix": "crates/airk-mcp/src/", "name": "Session", "limit": 10}),
                          ("references", {"path_prefix": "crates/airk-mcp/src/response.rs", "name": "Session", "limit": 20}),
                          ("read_file", {"path": "crates/airk-mcp/src/response.rs", "start_line": 94, "end_line": 105})]
            with ThreadPoolExecutor(max_workers=4) as pool:
                completed = list(pool.map(session.tool, tasks))
            for (name, _), (envelope, metadata) in zip(tasks, completed):
                body = replay.body(envelope)
                observations.append({"batch": batch + 1, "tool": name, "metadata": metadata,
                    "is_error": envelope.get("isError", False), "status_observation": body.get("status_observation"),
                    "index_state": body.get("index_state"), "publication": body.get("publication")})
                assert envelope.get("isError") is not True, body
                if name == "status":
                    assert body["index_state"] == "available", body
    report = {"schema": "airk-local-status-followup/1", "version": "1.8.3", "runtime_sha256": expected,
        "installed_hashes": hashes, "payload_verified": True, "health": health,
        "batches": 10, "concurrency": 4, "batch_calls": len(observations),
        "supersession_notes": sum(bool(o["status_observation"]) for o in observations),
        "final_tool_errors": sum(o["is_error"] for o in observations),
        "typed_retries": sum(o["metadata"]["retries"] for o in observations),
        "observations": observations, "cleanup": session.cleanup}
    (build / "installed-mcp-check.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in report.items() if k not in ["observations", "health"]}))


if __name__ == "__main__":
    main()
