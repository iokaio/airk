# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
import os
from pathlib import Path

root = Path(__file__).resolve().parents[1]
os.environ["AIRK_USAGE_MODE"] = "disabled"
spec = importlib.util.spec_from_file_location("detail", root / "scripts/benchmark-detail.py")
detail = importlib.util.module_from_spec(spec)
spec.loader.exec_module(detail)
output = root / "artifacts/enhancements-qualified-final"
summary = json.loads((output / "summary-private.json").read_text(encoding="utf8"))
assert summary["original_workspaces_unchanged"]
config = json.loads((root / "artifacts/enhancements-inputs.json").read_text(encoding="utf8"))
binary = root / "target/release/airk.exe"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == summary["binary_sha256"]
results = []
for workspace in config["workspaces"]:
    copied = output / "copies" / workspace["label"]
    changed = copied / workspace["path"]
    original = changed.read_bytes()
    assert original == (Path(workspace["root"]) / workspace["path"]).read_bytes()
    try:
        changed.write_bytes(original + b"\n// Original AIRK incremental qualification comment.\n")
        for level in detail.LEVELS:
            row = next(r for r in summary["runs"] if (r["workspace"], r["level"]) == (workspace["label"], level))
            assert row["automatic_cleanup"]["completed"]
            assert row["automatic_cleanup"]["storage"]["retained_generations"] == 1
            collection = "comparison-" + workspace["label"] + "-" + level + "-incremental"
            command = [str(binary), "--root", str(copied), "--data-dir", str(output / "incremental-state" / level), "--collection", collection]
            client = detail.Mcp(command, copied, collection, output / (collection + "-after-cleanup.stderr"))
            try:
                symbols, _ = client.call("airk_symbols", {"name": workspace["query"]})
                assert bool(symbols["facts"]) == (level != "essential")
                evidence, _ = client.call("airk_read_file", {"path": workspace["path"], "start_line": 1, "end_line": 1})
                assert evidence["hits"]
                evidence, _ = client.call("airk_search", {"query": workspace["query"], "limit": 10})
                assert evidence["hits"]
                other = next(w for w in config["workspaces"] if w["label"] != workspace["label"])
                evidence, _ = client.call("airk_search", {"query": other["query"], "limit": 10})
                assert not evidence["hits"]
                results.append({"workspace": workspace["label"], "level": level, "after_cleanup_mcp_passed": True})
                print(workspace["label"], level, "post-cleanup MCP passed", flush=True)
            finally:
                client.close()
    finally:
        changed.write_bytes(original)
    assert changed.read_bytes() == original
(output / "after-cleanup-mcp.json").write_text(json.dumps(results, indent=2), encoding="utf8")
