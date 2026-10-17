# SPDX-License-Identifier: Apache-2.0
"""Private real-workspace detail comparison. Never edits the input workspaces.

Config: {"workspaces":[{"label":"example","root":"/source",
"query":"unique_symbol","path":"src/example.rs"}, ...]}.
Results and excerpts must remain in an ignored local output directory.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import queue
import shutil
import sqlite3
import subprocess
import threading
import time

spec = importlib.util.spec_from_file_location("scale", Path(__file__).with_name("benchmark-scale.py"))
scale = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scale)
LEVELS = ("essential", "balanced", "detailed")


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def inventory(root, copied_tree=False):
    names = [p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file()] if copied_tree else subprocess.check_output(["git", "-c", "safe.directory=" + root.as_posix(),
        "-C", str(root), "ls-files", "-z", "--cached", "--others", "--exclude-standard"]).decode("utf-8").split("\0")
    result = {}
    for name in sorted(set(names)):
        if not name or any(part in ("temp", "artifacts", ".airk") for part in Path(name).parts):
            continue
        path = root / name
        if path.is_file() and not path.is_symlink():
            result[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


class Mcp:
    def __init__(self, command, root, collection, output):
        self.root, self.collection = root, collection
        self.errors = output.open("wb")
        self.process = subprocess.Popen(command + ["mcp"], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=self.errors, text=True, encoding="utf-8")
        self.responses, self.sequence = queue.Queue(), 0
        threading.Thread(target=lambda: [self.responses.put(line) for line in self.process.stdout], daemon=True).start()
        self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "airk-detail-qualification", "version": "1"}})
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def send(self, message):
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def request(self, method, params):
        self.sequence += 1
        self.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": params})
        while True:
            response = json.loads(self.responses.get(timeout=180))
            if response.get("id") == self.sequence:
                return response

    def validate(self, value):
        if isinstance(value, list):
            for item in value:
                self.validate(item)
        if isinstance(value, dict):
            if "collection" in value:
                assert value["collection"] == self.collection, "foreign collection"
            if "path" in value and "content_hash" in value:
                source = (self.root / value["path"]).resolve()
                assert source.is_relative_to(self.root.resolve()), "escaped citation"
                assert hashlib.sha256(source.read_bytes()).hexdigest() == value["content_hash"], "wrong source hash"
            for item in value.values():
                self.validate(item)

    def call(self, tool, arguments, rejected=False):
        start = time.perf_counter()
        result = self.request("tools/call", {"name": tool, "arguments": arguments})
        elapsed = time.perf_counter() - start
        error = "error" in result or result.get("result", {}).get("isError", False)
        if rejected:
            assert error, "foreign override/traversal accepted"
            return None, elapsed
        assert not error, result
        value = json.loads(result["result"]["content"][0]["text"])
        self.validate(value)
        return value, elapsed

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=10)
        self.errors.close()


def storage(directory):
    parts = {}
    for path in directory.rglob("*"):
        if path.is_file():
            relative = path.relative_to(directory).parts
            category = "/".join(relative[:2]) if relative[0] == "objects" else (
                path.name if relative[0] == "generations" and len(relative) == 3 else relative[0])
            parts[category] = parts.get(category, 0) + path.stat().st_size
    generations = list((directory / "generations").glob("*/inventory.json"))
    with sqlite3.connect((directory / "index.sqlite3").as_uri() + "?mode=ro", uri=True) as db:
        header = json.loads(db.execute("SELECT payload FROM index_components WHERE id=1").fetchone()[0])
    active = directory / "generations" / header["revision"]
    inv = json.loads((active / "inventory.json").read_text(encoding="utf-8"))
    catalog = active / "catalog.sqlite"
    with sqlite3.connect(catalog.as_uri() + "?mode=ro&immutable=1", uri=True) as db:
        rows = {name: db.execute("SELECT count(*) FROM " + name).fetchone()[0]
            for name in ("facts", "relationships", "files", "continuity_records")}
    return {"logical_bytes": sum(parts.values()), "components": parts, "rows": rows,
        "files": len(inv["files"]), "source_bytes": sum(f["source_bytes"] for f in inv["files"].values()),
        "detail": inv.get("index_detail"), "generation": inv["generation"], "retained_generations": len(generations)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--incremental-copies", action="store_true")
    parser.add_argument("--continue-cleanup", action="store_true", help="Measure bounded automatic cleanup continuations after each edit")
    args = parser.parse_args()
    binary, output = args.binary.resolve(), args.output.resolve()
    assert not output.exists(), "use a fresh ignored output directory"
    workspaces = json.loads(args.config.read_text(encoding="utf-8"))["workspaces"]
    assert len(workspaces) == 2, "two independent workspace probes required"
    output.mkdir(parents=True)
    os.environ["AIRK_USAGE_MODE"] = "disabled"
    os.environ["AIRK_CLIENT"] = "vscode"
    baseline = {w["label"]: inventory(Path(w["root"]).resolve(), w.get("copied_tree", False)) for w in workspaces}
    result = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "version": subprocess.check_output([str(binary), "--version"], text=True).strip(),
        "machine": scale.machine_environment(output),
        "basis": "Sequential optimized builds; fresh shared data root for each level/workspace pair; OS caches not flushed; usage recording disabled", "runs": []}

    def run(command, label, *parameters):
        return scale.measured(command + list(parameters), output, label, timeout=3600)

    for level in LEVELS:
        clients = []
        try:
            for index, workspace in enumerate(workspaces):
                label = workspace["label"] + "-" + level
                root = Path(workspace["root"]).resolve()
                collection = "comparison-" + label
                state = output / "state" / level
                command = [str(binary), "--root", str(root), "--data-dir", str(state), "--collection", collection]
                print(label + ": indexing original workspace", flush=True)
                run(command, label + "-configure", "collections", "extraction", "detail", level)
                report, timing = run(command, label + "-initial", "index", "--scheduled", "--automatic")
                initial = storage(state / "collections" / collection)
                noop, noop_timing = run(command, label + "-noop", "index", "--scheduled", "--automatic")
                assert noop["indexed"] == 0 and noop["generation"] == report["generation"], "no-change index republished"
                row = {"workspace": workspace["label"], "level": level, "initial": timing,
                    "noop": noop_timing, "index_report": report, "storage": initial, "mcp": []}
                result["runs"].append(row)
                client = Mcp(command, root, collection, output / (label + "-mcp.stderr"))
                clients.append(client)
                foreign = workspaces[1 - index]
                # Fresh helper connection for the first call; subsequent calls retain process caches.
                for attempt in range(3):
                    symbols, seconds = client.call("airk_symbols", {"name": workspace["query"]})
                    assert symbols["indexing"]["level"] == level
                    assert bool(symbols["facts"]) == (level != "essential")
                    row["mcp"].append({"tool": "symbols", "attempt": attempt, "seconds": seconds, "count": len(symbols["facts"])})
                for query, positive in [(workspace["query"], True), (foreign["query"], False)]:
                    evidence, seconds = client.call("airk_search", {"query": query, "limit": 10})
                    assert bool(evidence["hits"]) == positive
                    row["mcp"].append({"tool": "search", "positive": positive, "seconds": seconds})
                evidence, seconds = client.call("airk_read_file", {"path": workspace["path"], "start_line": 1, "end_line": 1})
                assert evidence["hits"]
                evidence, _ = client.call("airk_list_files", {"prefix": foreign["path"]})
                assert not evidence["paths"]
                client.call("airk_search", {"query": foreign["query"], "collection": "foreign"}, rejected=True)
                client.call("airk_read_file", {"path": "../foreign/file", "start_line": 1, "end_line": 2}, rejected=True)
                row["isolation_and_citations_passed"] = True
                save(output / "summary-private.json", result)
                print(json.dumps({"label": label, "storage_bytes": initial["logical_bytes"], "timing": timing}), flush=True)
        finally:
            for client in clients:
                client.close()

    if args.incremental_copies:
        for workspace in workspaces:
            root = Path(workspace["root"]).resolve()
            copied = output / "copies" / workspace["label"]
            for relative, expected in baseline[workspace["label"]].items():
                source = root / relative
                destination = copied / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, destination)
                assert hashlib.sha256(destination.read_bytes()).hexdigest() == expected
            changed = copied / workspace["path"]
            original = changed.read_bytes()
            for level in LEVELS:
                label = workspace["label"] + "-" + level + "-incremental"
                collection = "comparison-" + label
                state = output / "incremental-state" / level
                command = [str(binary), "--root", str(copied), "--data-dir", str(state), "--collection", collection]
                print(label + ": indexing isolated source copy", flush=True)
                run(command, label + "-configure", "collections", "extraction", "detail", level)
                run(command, label + "-baseline", "index", "--scheduled", "--automatic")
                copied_storage = storage(state / "collections" / collection)
                original_row = next(r for r in result["runs"] if r["workspace"] == workspace["label"] and r["level"] == level)
                assert copied_storage["files"] == original_row["storage"]["files"], "copy changed eligible file count"
                assert copied_storage["source_bytes"] == original_row["storage"]["source_bytes"], "copy changed eligible source size"
                try:
                    changed.write_bytes(original + b"\n// Original AIRK incremental qualification comment.\n")
                    report, timing = run(command, label + "-changed", "index", "--scheduled", "--automatic")
                    assert report["indexed"] == 1 and report["reused"] == copied_storage["files"] - 1, report
                    row = next(r for r in result["runs"] if r["workspace"] == workspace["label"] and r["level"] == level)
                    row["one_file_edit_in_isolated_copy"] = {"timing": timing, "report": report,
                        "storage_after_cleanup": storage(state / "collections" / collection)}
                    client = Mcp(command, copied, collection, output / (label + "-mcp.stderr"))
                    try:
                        facts, _ = client.call("airk_symbols", {"name": workspace["query"]})
                        assert bool(facts["facts"]) == (level != "essential")
                        assert facts["indexing"]["level"] == level
                        evidence, _ = client.call("airk_search", {"query": workspace["query"], "limit": 10})
                        assert evidence["hits"]
                        foreign = next(w for w in workspaces if w["label"] != workspace["label"])
                        evidence, _ = client.call("airk_search", {"query": foreign["query"], "limit": 10})
                        assert not evidence["hits"]
                        row["one_file_edit_in_isolated_copy"]["mcp_passed"] = True
                    finally:
                        client.close()
                    if args.continue_cleanup:
                        passes = []
                        for attempt in range(8):
                            cleanup, measured = run(command, label + "-cleanup-" + str(attempt), "collections", "retention", "continue")
                            passes.append({"timing": measured, "receipt": cleanup})
                            if cleanup is None or not cleanup["receipt"]["pending"]:
                                break
                        completed = cleanup is None or (not cleanup["receipt"]["pending"] and not cleanup["receipt"]["error"])
                        row["automatic_cleanup"] = {"passes": passes, "completed": completed,
                            "storage": storage(state / "collections" / collection)}
                        save(output / "summary-private.json", result)
                        assert completed, "automatic cleanup still pending; inspect recorded receipts"
                        assert row["automatic_cleanup"]["storage"]["retained_generations"] == 1, "automatic cleanup did not reach retention target"
                        client = Mcp(command, copied, collection, output / (label + "-after-cleanup.stderr"))
                        try:
                            facts, _ = client.call("airk_symbols", {"name": workspace["query"]})
                            assert bool(facts["facts"]) == (level != "essential")
                            evidence, _ = client.call("airk_read_file", {"path": workspace["path"], "start_line": 1, "end_line": 1})
                            assert evidence["hits"]
                            evidence, _ = client.call("airk_search", {"query": workspace["query"], "limit": 10})
                            assert evidence["hits"]
                            evidence, _ = client.call("airk_search", {"query": foreign["query"], "limit": 10})
                            assert not evidence["hits"]
                            row["automatic_cleanup"]["mcp_passed"] = True
                        finally:
                            client.close()
                    save(output / "summary-private.json", result)
                finally:
                    changed.write_bytes(original)
    for workspace in workspaces:
        assert inventory(Path(workspace["root"]).resolve(), workspace.get("copied_tree", False)) == baseline[workspace["label"]], "input workspace changed"
    result["original_workspaces_unchanged"] = True
    save(output / "summary-private.json", result)
    print("All detail, isolation, citation and source-preservation checks passed.", flush=True)


if __name__ == "__main__":
    main()
