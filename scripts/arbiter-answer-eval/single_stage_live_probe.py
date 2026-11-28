# SPDX-License-Identifier: Apache-2.0
"""Qualify the selected real stdio Gather on an already exposed, disposable Munarium edit."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

import common
import companion_admission
import mcp_wrapper
import product_skill_http as http
import workspace_scope

SOURCE = "clients/python/src/munarium_client/rest.py"
TASK = {"task": "Locate the Python REST command retryability helper and show its implementation.",
        "needs": ["Where is the _retryable decision defined in the Python REST transport?"],
        "anchors": ["_retryable", "clients/python/src/munarium_client/rest.py"]}
MARKER = b"# _retryable stable publication experiment P2R2\n"


@contextmanager
def local_session(argv, stdout_log):
    with stdout_log.open("xb") as error_log:
        child = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=error_log)
        pool = ThreadPoolExecutor(max_workers=1)
        try:
            def request(ident, method, params):
                msg = {"jsonrpc": "2.0", "method": method, "params": params}
                if ident is not None:
                    msg["id"] = ident
                mcp_wrapper.write_message(child.stdin, msg, "line")
                if ident is None:
                    return None
                raw = pool.submit(child.stdout.readline).result(timeout=330)
                result = json.loads(raw)
                common.require(result["id"] == ident, "Response identity mismatch")
                return raw, result
            yield request
        finally:
            child.stdin.close()
            try:
                child.wait(timeout=4)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=10)
            pool.shutdown()


def run(args):
    handle = common.load_handle(args.handle.resolve(strict=True))
    admitted = companion_admission.observe(handle, args.admission)
    work = args.work.resolve(strict=True)
    original = args.baseline.resolve(strict=True)
    output = args.output.resolve(strict=True)
    common.require(work.name == "work" and work.parent.name == "munarium-P2-B1.candidate" and
                   work.parent.parent.name == "runs" and work.parent.parent.parent.name == "E2E-single-r1" and
                   not original.is_relative_to(work) and not work.is_relative_to(original) and
                   Path(common.read_json(args.handle)["indexes"][args.repo]["root"]).resolve(strict=True) == work,
                   "Require the exact disposable candidate work registration")
    target = work / SOURCE
    authentic = (original / SOURCE).read_bytes()
    common.require(target.resolve(strict=True).is_relative_to(work) and authentic == target.read_bytes() and
                   MARKER not in authentic, "Unexpected source file or bytes")
    definition = b"def _retryable("
    common.require(authentic.count(definition) == 1, "Expected the frozen development symbol")
    offset = authentic.index(definition)
    line_at = authentic[:offset].count(b"\n") + 1
    common.require(authentic[offset-1:offset] == b"\n", "Unexpected indented development symbol")
    changed = authentic[:offset] + MARKER + authentic[offset:]
    sha = lambda data: hashlib.sha256(data).hexdigest()
    surface = http.read_surface(handle, args.repo)
    tool = next(t for t in surface["tools"] if t["name"] == "gather_context")
    for name, value in (("tool.json", tool), ("native-surface.json", surface)):
        with (output / name).open("x", encoding="utf-8") as stream:
            json.dump(value, stream)
    argv = [sys.executable, "-B", str(Path(__file__).with_name("gather_context_mcp.py")), "--repo", args.repo,
            "--handle", str(args.handle), "--tool-file", str(output / "tool.json"), "--log-dir", str(output / "gather"),
            "--live-source", str(work), "--workspace-map", "--native-surface", str(output / "native-surface.json"),
            "--judge", str(args.judge), "--live-layout", "qwen-finalist", "--finalist-freeze", str(args.freeze)]
    receipt = {"before": admitted, "baseline_sha256": sha(authentic), "changed_sha256": sha(changed), "phases": {}}
    with local_session(argv, output / "stderr.log") as rpc:
        count = 0
        def call(name, arguments):
            nonlocal count
            count += 1
            raw, envelope = rpc(count, "tools/call", {"name": name, "arguments": arguments})
            return raw, envelope["result"]

        def wait_publication(previous, expected, forbidden):
            deadline = time.monotonic() + 120
            while True:
                _, result = call("read_indexed_range", {"path": SOURCE, "start_line": line_at, "end_line": line_at + 2})
                common.require(not result.get("isError"), f"Native read failed; no replay: {result}")
                data = result["structuredContent"]
                token = data["publication"]["token"]
                text = "\n".join(hit["text"] for hit in data["hits"] if hit["citation"]["path"] == SOURCE)
                if token != previous and expected in text and (not forbidden or forbidden not in text):
                    return token
                common.require(time.monotonic() < deadline, "Watcher did not publish expected source content")
                time.sleep(0.8)

        def settled(label, expected, forbidden, original_bytes=False):
            record = common.read_json(args.handle)["indexes"][args.repo]
            deadline = time.monotonic() + 180
            samples = []
            while True:
                cmd = [*handle.cli, "--service-runtime-dir", handle.control, "--service-project", record["binding"],
                       "--output", "json", "settings", "vector_status"]
                status = json.loads(subprocess.run(cmd, capture_output=True, timeout=30, check=True).stdout)
                semantic, discovery = status["semantic_readiness"], status["discovery_readiness"]
                samples.append([semantic["state"], discovery["state"], semantic["completed_artifacts"],
                                semantic["selected_artifacts"], status["indexed_vectors"]])
                if (discovery["state"] == semantic["state"] == "ready" and discovery["committed_artifacts"] == 1023 and
                        semantic["pipeline_digest"] == record["pipeline_digest"] and
                        semantic["completed_artifacts"] == semantic["selected_artifacts"] == 1020 and
                        (status["indexed_vectors"] == 40536 if original_bytes else status["indexed_vectors"] > 0)):
                    break
                common.require(time.monotonic() < deadline, "Native discovery/embeddings did not settle")
                time.sleep(0.8)
            metrics = {}
            paths = workspace_scope.eligible_paths(handle, args.repo, metrics)
            token = wait_publication(None, expected, forbidden)
            common.require(SOURCE in paths and token == metrics["publication"], "Native read and ready inventory differ")
            receipt.setdefault("stable", {})[label] = {"read": token, "inventory": metrics, "samples": samples}
            return token

        def immediate_inventory(label):
            metrics = {}
            try:
                paths = workspace_scope.eligible_paths(handle, args.repo, metrics)
                outcome = {"present": SOURCE in paths, "metrics": metrics}
            except workspace_scope.InventoryUnavailable as exc:
                outcome = {"page": exc.page, "publication": exc.publication, "native": exc.reply}
            except ValueError as exc:
                outcome = {"error": str(exc)}
            receipt.setdefault("immediate_inventory", {})[label] = outcome

        def gather(label, expected_hash, token, require_marker=False):
            wire, result = call("gather_context", TASK)
            body = json.loads(result["content"][0]["text"])
            rows = [p for p in body.get("passages", []) if p["at"].startswith(SOURCE + ":")]
            covered = any(MARKER.decode().strip() in p["text"] for p in rows)
            valid = (not result["isError"] and body.get("eligibility", {}).get("publication") == token and
                     bool(rows) and all(p["workspace_sha256"] == expected_hash for p in rows) and
                     (covered if require_marker else not covered))
            receipt["phases"][label] = {"response_id": count, "bytes": len(wire), "wire_sha256": sha(wire),
                                       "publication": token, "reported_target": len(rows), "marker_seen": covered, "valid": valid}
            return valid

        _, init = rpc(0, "initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                        "clientInfo": {"name": "local-freshness-audit", "version": "1"}})
        common.require(init["result"]["instructions"] == surface["instructions"], "Native instructions changed")
        rpc(None, "notifications/initialized", {})
        _, listing = rpc(-1, "tools/list", {})
        common.require(listing["result"]["tools"] == surface["tools"], "Native tool surface changed")
        for label, invalid in (("unsupported", {"task": "probe", "fresh": True}), ("malformed", {"task": ""})):
            wire, rejected = call("gather_context", invalid)
            receipt["phases"][label] = {"response_id": count, "bytes": len(wire), "valid": bool(rejected.get("isError")),
                                       "reply": rejected}
        original_pub = wait_publication(None, "def _retryable", MARKER.decode().strip())
        original_pub = settled("baseline", "def _retryable", MARKER.decode().strip(), original_bytes=True)
        gather("baseline", sha(authentic), original_pub)
        new_pub = None
        try:
            target.write_bytes(changed)
            new_pub = wait_publication(original_pub, MARKER.decode().strip(), None)
            immediate_inventory("edited")
            new_pub = settled("edited", MARKER.decode().strip(), None)
            gather("edited", sha(changed), new_pub, require_marker=True)
        finally:
            actual = target.read_bytes()
            common.require(actual in (authentic, changed), "Another writer changed the candidate file; refusing overwrite")
            if actual == changed:
                target.write_bytes(authentic)
            common.require(target.read_bytes() == authentic, "Restore failed")
            restored_pub = wait_publication(new_pub, "def _retryable", MARKER.decode().strip())
            immediate_inventory("restored")
            restored_pub = settled("restored", "def _retryable", MARKER.decode().strip(), original_bytes=True)
            gather("restored", sha(authentic), restored_pub)
    receipt["after"] = companion_admission.observe(handle, args.admission)
    receipt["valid"] = receipt["after"] == admitted and all(p["valid"] for p in receipt["phases"].values())
    with (output / "receipt.json").open("x", encoding="utf-8") as stream:
        json.dump(receipt, stream, indent=2)
    print(json.dumps({"valid": receipt["valid"], "phases": {k: v["valid"] for k, v in receipt["phases"].items()}}))
    return int(not receipt["valid"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ("handle", "judge", "admission", "freeze", "work", "baseline", "output"):
        parser.add_argument("--" + key, type=Path, required=True)
    parser.add_argument("--repo", default="munarium")
    raise SystemExit(run(parser.parse_args()))