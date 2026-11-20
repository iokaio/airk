# SPDX-License-Identifier: Apache-2.0
"""Prepare and verify the original Windows Large layer-zero K/V/Q capture."""

import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
BASE_GRAPH = "fff6dbe2f0c2fac4b9ba215da7d5029421d2706ee48c01c2485b9f157ee15ca6"
NATIVE = "0903b38c801a69f2f2866e269164e3867bafc00a999f755a70b0adca06c0b4f4"
NATIVE_OWNED = "e932240f96d539a810b684b83d0b27ce9bfaaa9368f6c49a47fa2433b7f9453a"
REFERENCE = "c214278ab06a24053d0a309b294e459ec7ba717e99f4e39edd3fedaa26c878bf"
CONTROL = "f29acef6a06fcd75e290ec5dfb3d7ceee447e9cf216afa5df6763e204d066ca8"
ORIGINAL_MODEL = "c00371ad9407c25bd52999d87327115025b7c6f55a4e585645ca112d858de5bd"
ORIGINAL_WEIGHTS = "5b053f02795e72ff88123b28f89d3f2c89da055e9ed2228bc60809dd0a2b4270"
INPUT = "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335"
POSITIONS = (0, 15, 31)
ROUTES = {"k": 1024, "v": 1024, "q": 4096}
NODES = {k: "/scorer/layers.0/self_attn/" + k + "_proj/MatMul" for k in ROUTES}
SHARED = "/airk_g5/weight/" + NODES["k"] + "/input16"
TAPS = {SHARED: 2560, **{"/airk_g5/weight/" + NODES[k] + "/output16": w for k, w in ROUTES.items()}}


def require(value, message):
    if not value:
        raise ValueError(message)


def pin(path):
    require(path.resolve(strict=True).is_relative_to(CAMPAIGN) and path.is_file(), "existing campaign input required")
    with path.open("rb") as file:
        return {"path": str(path), "bytes": path.stat().st_size,
                "sha256": hashlib.file_digest(file, "sha256").hexdigest()}


def read(path, expected):
    p = pin(path)
    require(p["sha256"] == expected, "input digest changed: " + path.name)
    return p, json.loads(path.read_bytes())


def edges(metadata, *, promoted):
    require(metadata["complete"] is True and metadata["failure"] is None, "complete graph metadata required")
    rows = metadata["rows"]
    graph = [r for r in rows if r["graph"] == 0]
    nodes = {r["name"]: r for r in graph if r["kind"] == "node"}
    values = {r["name"]: r for r in graph if r["kind"] == "value"}
    require(len(nodes) == len([r for r in graph if r["kind"] == "node"]), "duplicate node name")
    def edge(node, role, slot):
        matched = [r["value"] for r in graph if r["kind"] == "edge" and r["node"] == node
                   and r["role"] == role and r["slot"] == slot]
        require(len(matched) == 1, "missing or repeated edge")
        return matched[0]
    for name, width in TAPS.items():
        v = values[name]
        dims = sorted((r for r in graph if r["kind"] == "dimension" and r["value"] == v["value"]),
                      key=lambda r: r["index"])
        require(v["rank"] == 3 and v["element_type"] == 10 and v["provenance"] == "node_output"
                and [(r["number"], r["symbol"]) for r in dims] == [(1, None), (None, "tokens"), (width, None)],
                "Large half tap ABI changed: " + name)
        count = sum(r["kind"] == "edge" and r["node"] is None and r["role"] == 4 and r["value"] == v["value"]
                    for r in graph)
        require(count == int(promoted), "incorrect graph output exposure")
    cast = nodes[SHARED]
    shared = values[SHARED]["value"]
    require(cast["ep"] == "CUDAExecutionProvider" and cast["op"] == "Cast" and cast["domain"] == ""
            and edge(cast["node"], 2, 0) == shared, "shared input must be the CUDA Cast output")
    measured = {}
    for route, name in NODES.items():
        node = nodes[name]
        output = values["/airk_g5/weight/" + name + "/output16"]["value"]
        require(node["domain"] == "" and node["op"] == "MatMul" and node["ep"] == "CUDAExecutionProvider"
                and edge(node["node"], 0, 0) == shared and edge(node["node"], 2, 0) == output,
                "Large projection lost its exact measured input or output")
        measured[route] = {"node": node["node"], "shared_fp16_input": shared, "fp16_output": output}
    return measured


def selected_words(taps):
    require(len(taps) == len(TAPS) and {r["name"] for r in taps} == set(TAPS), "tap roster differs")
    out = {}
    for row in taps:
        width = TAPS[row["name"]]
        words = row["bits"]
        require(row["width"] == width and len(words) == 32 * width
                and all(type(x) is int and 0 <= x <= 65535 and x & 0x7c00 != 0x7c00 for x in words),
                "finite original FP16 words for all 32 rows required")
        out[row["name"]] = [words[i * width:(i + 1) * width] for i in POSITIONS]
    return out


def check_capture(report, baseline):
    require(report["schema"] == "airk.private.large-fp16-layer0-projection-t32/1"
            and report["rows"][0]["input_digest"] == baseline["rows"][0]["input_digest"] == INPUT
            and report["rows"][0]["raw_scores"] == baseline["rows"][0]["raw_scores"],
            "promoted graph changed original T32 scores or input")
    live = edges(report["layer0_final_graph"], promoted=True)
    selected = selected_words(report["layer0_taps"])
    require(sum(len(row) for rows in selected.values() for row in rows) * 2 == 52224,
            "three complete original Large operand rows required")
    return {"live_edges": live, "selected_positions": list(POSITIONS), "selected_bytes": 52224,
            "original_scores_unchanged": True}


def read_capture(path, sha, *, all_rows=False):
    require(path.parent.parent == CAMPAIGN and path.name == "execution.json", "owned campaign capture required")
    proof, rec = read(path, sha)
    job = rec["owned"]
    require(rec["mode"] == "projection-t32" and rec["complete"] is True and not rec["whole_phase_expired"]
            and job["exit_code"] == 0 and job["owned_job_final_active_processes"] == 0 and not job["timed_out"],
            "successful bounded native CUDA capture required")
    native_path = Path(rec["native"]["path"])
    require(native_path == path.with_name("native.json"), "expected colocated capture")
    native_pin, report = read(native_path, rec["native"]["sha256"])
    require(native_pin == rec["native"] and native_pin["bytes"] < 64 << 20
            and report["pid"] == job["owned_leader_pid"], "capture differs from its owned process")
    _, baseline = read(CAMPAIGN / "windows-large-fp16-t32-r2/native.json", NATIVE)
    require(check_capture(report, baseline) == rec["capture"], "capture parent and child observation differ")
    if all_rows:
        return proof, {tap["name"]: tap["bits"] for tap in report["layer0_taps"]}
    return proof, selected_words(report["layer0_taps"])


def prepare():
    bp, base = read(CAMPAIGN / "windows-large-fp16-graph-r1/native.json", BASE_GRAPH)
    op, owned = read(CAMPAIGN / "windows-large-fp16-t32-r2/execution.json", NATIVE_OWNED)
    np, native = read(Path(owned["native"]["path"]), NATIVE)
    rp, ref = read(CAMPAIGN / "large-fp16-sm86-flash-independent-t32-r1/reference.json", REFERENCE)
    cp, control = read(CAMPAIGN / "large-fp16-t32-layer0-cpu-rounding-r2/execution.json", CONTROL)
    require(owned["native"] == np and native["reviewed_graph"]["sha256"] == BASE_GRAPH and owned["complete"]
            and owned["owned"]["owned_job_final_active_processes"] == control["owned"]["owned_job_final_active_processes"] == 0
            and owned["owned"]["exit_code"] == control["owned"]["exit_code"] == 0
            and native["rows"][0]["input_digest"] == ref["input_digest"] == control["observation"]["input_digest"] == INPUT
            and control["observation"]["token_positions"] == list(POSITIONS)
            and control["observation"]["reference_sha256"] == REFERENCE, "Large original scientific evidence differs")
    for key, expected in (("graph", ORIGINAL_MODEL), ("weights", ORIGINAL_WEIGHTS)):
        stored = ref[key]
        require(stored == pin(Path(stored["path"])) and stored["sha256"] == expected, "original Large artifact differs")
    edges_before = edges(base["graph_metadata"], promoted=False)
    trace_pin, trace = read(Path(native["runtime_profile"]["path"]), native["runtime_profile"]["sha256"])
    for route, width in ROUTES.items():
        events = [e["args"] for e in trace if e["name"] == NODES[route] + "_kernel_time"]
        require(len(events) == 1 and events[0]["provider"] == "CUDAExecutionProvider"
                and events[0]["op_name"] == "MatMul"
                and events[0]["input_type_shape"] == [{"float16": [1,32,2560]}, {"float16": [2560,width]}]
                and events[0]["output_type_shape"] == [{"float16": [1,32,width]}], "baseline runtime shape differs")
    return {"model": ref["graph"], "weights": ref["weights"], "original_graph": bp, "original_native": np,
            "original_native_owned": op, "trace": trace_pin, "reference": rp, "cpu_operand_control": cp,
            "input_digest": INPUT, "positions": POSITIONS, "original_edges": edges_before, "outputs": TAPS,
            "full_tap_bytes": 32 * 2 * sum(TAPS.values()), "selected_tap_bytes": len(POSITIONS) * 2 * sum(TAPS.values()),
            "new_weight_bytes": 0, "capture_run": False, "qualification": False}


if __name__ == "__main__":
    print(json.dumps(prepare(), allow_nan=False))
