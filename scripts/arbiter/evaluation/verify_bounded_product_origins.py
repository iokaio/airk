# SPDX-License-Identifier: Apache-2.0
"""Count CPU-incident value origins in the retained genuine Windows Compact QKV final graph."""
import argparse
import hashlib
import json
from collections import Counter
from pathlib import Path

GRAPH_SHA = "42a273598b5c8678535691426ed725041a5d9565ff1fbaf535a43938894f07a4"


def inspect(path):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != GRAPH_SHA:
        raise ValueError("unreviewed Windows graph observation")
    report = json.loads(raw)
    graph = report["graph_observation"]["graph_metadata"]
    if not (report["complete"] and graph["complete"] and graph["shim_status"] == graph["ort_error_code"] == 0):
        raise ValueError("native graph observation incomplete")
    rows = graph["rows"]
    cpu = {(r["graph"], r["node"]) for r in rows if r["kind"] == "node" and r["ep"] == "CPUExecutionProvider"}
    values = {(r["graph"], r["value"]): r["provenance"] for r in rows if r["kind"] == "value"}
    incident = {(r["graph"], r["value"]) for r in rows if r["kind"] == "edge" and r["value"] is not None
                and (r["graph"], r["node"]) in cpu}
    origins = Counter(values[v] for v in incident)
    if origins != {"immutable_session_initializer": 66, "node_output": 460}:
        raise ValueError("retained bounded input population differs")
    return {"native_graph_sha256": GRAPH_SHA, "profile": report["profile"], "cpu_nodes": len(cpu),
            "cpu_incident_values": len(incident), "origin_counts": dict(sorted(origins.items())),
            "unresolved_or_overridable_cpu_values": 0, "new_model_calls": 0}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("graph", type=Path)
    print(json.dumps(inspect(parser.parse_args().graph), sort_keys=True))
