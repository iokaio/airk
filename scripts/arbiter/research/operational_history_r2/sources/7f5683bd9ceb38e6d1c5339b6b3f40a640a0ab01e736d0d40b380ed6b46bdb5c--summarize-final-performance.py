# SPDX-License-Identifier: Apache-2.0
import json
from pathlib import Path

base = Path(__file__).resolve().parent
before = json.loads((base / "detail-real-final/summary-private.json").read_text(encoding="utf8"))
after = json.loads((base / "enhancements-qualified-final/summary-private.json").read_text(encoding="utf8"))
assert after["original_workspaces_unchanged"]
print("Native SHA-256:", after["binary_sha256"])
print("Machine:", after["machine"])
for row in after["runs"]:
    old = next(r for r in before["runs"] if (r["workspace"], r["level"]) == (row["workspace"], row["level"]))
    for field in ["rows", "files", "source_bytes"]:
        assert old["storage"][field] == row["storage"][field]
    assert row["automatic_cleanup"]["completed"]
    assert row["automatic_cleanup"]["storage"]["retained_generations"] == 1
    def seconds(r, name):
        return r[name]["wall_ms"] / 1000
    def edit(r):
        return r["one_file_edit_in_isolated_copy"]["timing"]["wall_ms"] / 1000
    def symbols(r):
        values = [item["seconds"] * 1000 for item in r["mcp"] if item["tool"] == "symbols"]
        return [round(values[0], 2), round(sum(values[1:]) / len(values[1:]), 2)]
    print(json.dumps({"workspace":row["workspace"], "level":row["level"],
        "initial_s":[round(seconds(r,"initial"),2) for r in [old,row]],
        "noop_s":[round(seconds(r,"noop"),2) for r in [old,row]],
        "edit_s":[round(edit(r),2) for r in [old,row]],
        "initial_MiB":[round(r["storage"]["logical_bytes"]/1048576,2) for r in [old,row]],
        "peak_MiB":[round(r["initial"]["peak_rss_bytes"]/1048576,2) for r in [old,row]],
        "symbol_first_warm_ms":[symbols(r) for r in [old,row]],
        "extra_cleanup_passes":len([v for v in row["automatic_cleanup"]["passes"] if v["receipt"] is not None]),
        "cleanup_s":round(sum(v["timing"]["wall_ms"] for v in row["automatic_cleanup"]["passes"] if v["receipt"] is not None)/1000,2)}))
