# SPDX-License-Identifier: Apache-2.0
"""Stage a distinct, unqualified Compact Q/K/V or Q/K/V/O FP32 projection candidate."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import sys
import time


HERE = Path(__file__).resolve().parent
PROJECTION_PROBE_SHA = "aa47ae8ba52c27ff38e9ac13a1ffdd0737bfa21a264bc04074d885e8525158ec"
R11_RECEIPT_SHA = "789277ec76b57fe1ae7a30a47ea90da1a9e9ae2d8f68eddff8d164f6246e494e"
SCHEMA = "airk.private.compact-fp32-qkv-projection-candidate/1"
OUTPUT_SCHEMA = "airk.private.compact-fp32-qkvo-projection-candidate/1"
OUTPUT_RECEIPT_SHA = "355c354af9e55f36e6be41fbfd8b89719e13d5c6d5193cbb8bc03e2359ebd97e"
OUTPUT_CAPTURE_SHA = "9705db75efa3e84fc955c3edeaf03e1aef70e5362161087f24250e14b75aeac6"


def load_owner(check):
    path = HERE / "probe_projection_gemm.py"
    import hashlib

    check()
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    if digest != PROJECTION_PROBE_SHA:
        raise ValueError("reviewed projection experiment owner differs")
    spec = importlib.util.spec_from_file_location("compact_projection_candidate_owner", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def reviewed_output_experiment(owner, verifier, converter, check):
    receipt, pin = verifier.read_pinned_json(
        owner.CAMPAIGN / "compact-fp16-layer0-output-cublas-r1/execution.json", OUTPUT_RECEIPT_SHA, check)
    converter.windows_owner(pin["path"])
    schema = "airk.private.compact-fp16-output-projection-gemm/1"
    result = receipt["result"]
    modes = result["modes"]
    owner.require(receipt["schema"] == result["schema"] == schema and receipt["complete"] is True
                  and receipt["qualification"] is result["qualification"] is result["product_admission"] is False
                  and result["native_model_forward"] is result["captured_original_onnx_projection_output"] is False
                  and result["capture_sha256"] == OUTPUT_CAPTURE_SHA and result["words"] == 65_536
                  and result["candidate_graph_sha256"] == owner.GRAPH_SHA and result["weights_sha256"] == owner.WEIGHTS_SHA
                  and modes["GEMM2"]["half_mismatches"] == 823 and modes["ORT_FP32_PROJECTION"]["half_mismatches"] == 117
                  and receipt["owned"]["exit_code"] == 0 and receipt["owned"]["owned_descendants_settled"] is True,
                  "reviewed standalone O projection experiment differs")
    return pin


def stage(args):
    started = time.monotonic()
    deadline = started + 120

    def check():
        if time.monotonic() >= deadline:
            raise TimeoutError("original projection candidate staging deadline expired")

    owner = load_owner(check)
    verifier, converter, plan, plan_pin, token = owner.bootstrap(check)
    require, pin = owner.require, owner.pinned
    directory = args.output
    require(directory.parent == owner.CAMPAIGN and directory.is_absolute()
            and directory.resolve(strict=False) == directory and not directory.exists(),
            "fresh direct child of the checked Windows campaign required")
    r11, r11_pin = verifier.read_pinned_json(
        owner.CAMPAIGN / "compact-fp16-layer0-cublas-compare-r11/execution.json", R11_RECEIPT_SHA, check)
    converter.windows_owner(r11_pin["path"])
    require(r11["complete"] and r11["qualification"] is False
            and r11["result"]["baseline_reproduces_ort"]
            and r11["result"]["candidate_graph_sha256"] == owner.GRAPH_SHA
            and r11["result"]["weights_sha256"] == owner.WEIGHTS_SHA
            and r11["owned"]["exit_code"] == 0 and r11["owned"]["owned_descendants_settled"],
            "reviewed stock FP32 projection experiment differs")
    output_pin = reviewed_output_experiment(owner, verifier, converter, check) if args.output_projection else None
    graph, weights = owner.CANDIDATE / "model.onnx", owner.CANDIDATE / "weights.bin"
    for path in (graph, weights):
        converter.windows_owner(path)
    graph_pin, weights_pin = pin(graph, check), pin(weights, check)
    require(graph_pin["sha256"] == owner.GRAPH_SHA and graph_pin["bytes"] <= 16 << 20
            and weights_pin["sha256"] == owner.WEIGHTS_SHA
            and weights_pin["bytes"] == plan["expected_weights_bytes"],
            "retained Compact graph or weight storage differs")
    sources = []
    for leaf, expected in (("projection_candidate_graph.py", args.transform_sha256),
                           ("verify_projection_candidate_graph.py", args.verifier_sha256)):
        path = HERE / leaf
        converter.windows_owner(path)
        row = pin(path, check)
        require(row["sha256"] == expected, "reviewed projection candidate source differs: " + leaf)
        sources.append(row)
    runtime, runtime_pin = verifier.read_pinned_json(
        plan["runtime_manifest"]["path"], plan["runtime_manifest"]["sha256"], check)
    require(runtime_pin == plan["runtime_manifest"], "reviewed staging import closure differs")
    transform, check_graph = (owner.load(Path(row["path"]), Path(row["path"]).stem) for row in sources)
    graph_path, weights_path, manifest_path = (directory / leaf for leaf in ("model.onnx", "weights.bin", "stage.json"))
    converter.guard(plan, directory, deadline=deadline)
    directory.mkdir()
    converter.guard(plan, directory, True, deadline=deadline)
    for path in (graph_path, weights_path, manifest_path):
        converter.guard(plan, path, deadline=deadline)
    sys.addaudithook(lambda event, values: converter.output_access(event, values, directory))
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import onnx

    imported = verifier.loaded_runtime(runtime["files"], pin, check,
                                       (sys.modules[name] for name in sys.modules.keys() - before))
    baseline = onnx.load_model(graph, load_external_data=False)
    candidate = transform.rewrite_compact_qkv_fp32(onnx, baseline, output_projection=args.output_projection)
    graph_proof = check_graph.verify_projection_candidate_graph(
        baseline, candidate, onnx, output_projection=args.output_projection,
    )
    projections = 112 if args.output_projection else 84
    require(graph_proof["projection_count"] == projections and graph_proof["added_cast_count"] == projections * 3,
            "wrong reviewed projection graph scope")
    with graph_path.open("xb") as destination:
        destination.write(candidate.SerializeToString())
    with weights.open("rb") as source, weights_path.open("xb") as destination:
        for block in iter(lambda: source.read(16 << 20), b""):
            check()
            destination.write(block)
    for path in (graph_path, weights_path):
        converter.windows_owner(path)
    candidate_graph, candidate_weights = pin(graph_path, check), pin(weights_path, check)
    require(candidate_graph["sha256"] != graph_pin["sha256"] and candidate_graph["bytes"] <= 16 << 20
            and (candidate_weights["sha256"], candidate_weights["bytes"])
            == (weights_pin["sha256"], weights_pin["bytes"]), "candidate graph or storage identity differs")
    written = onnx.load_model(graph_path, load_external_data=False)
    require(check_graph.verify_projection_candidate_graph(
        baseline, written, onnx, output_projection=args.output_projection,
    ) == graph_proof, "written candidate graph proof differs")
    onnx.checker.check_model(os.fspath(graph_path))
    self_pin = pin(Path(__file__).resolve(), check)
    experiment_pin = pin(HERE / "probe_projection_gemm.py", check)
    input_pins = (graph_pin, weights_pin, r11_pin, plan_pin, runtime_pin, *sources)
    if output_pin is not None:
        input_pins += (output_pin,)
    for row in input_pins:
        require(pin(Path(row["path"]), check) == row, "staging input changed")
    require(experiment_pin["sha256"] == PROJECTION_PROBE_SHA, "projection owner changed")
    receipt = {
        "schema": OUTPUT_SCHEMA if output_pin is not None else SCHEMA,
        "complete": True, "qualification": False, "product_admission": False,
        "native_load_attempts": 0, "native_forward_attempts": 0, "baseline_graph": graph_pin,
        "baseline_weights": weights_pin, "candidate_graph": candidate_graph, "candidate_weights": candidate_weights,
        "graph_proof": graph_proof, "projection_experiment": output_pin or r11_pin, "conversion_plan": plan_pin,
        "staging_source": self_pin, "projection_sources": sources, "verified_onnx_imports": len(imported),
        "execution_token": token, "command": [sys.executable, *sys.orig_argv[1:]],
        "elapsed_seconds": time.monotonic() - started,
    }
    check()
    with manifest_path.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(receipt, destination, allow_nan=False)
        destination.write("\n")
    converter.windows_owner(manifest_path)
    check()
    print(json.dumps({"complete": True, "graph": candidate_graph, "receipt": pin(manifest_path, check)}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--transform-sha256", required=True)
    parser.add_argument("--verifier-sha256", required=True)
    parser.add_argument("--output-projection", action="store_true")
    stage(parser.parse_args())


if __name__ == "__main__":
    main()
