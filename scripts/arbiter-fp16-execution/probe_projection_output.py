# SPDX-License-Identifier: Apache-2.0
"""Compare CUDA arithmetic on the sealed original Compact layer-zero O projection."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import time


HERE = Path(__file__).resolve().parent
BASE_SHA = "aa47ae8ba52c27ff38e9ac13a1ffdd0737bfa21a264bc04074d885e8525158ec"
CAPTURE_SHA = "9705db75efa3e84fc955c3edeaf03e1aef70e5362161087f24250e14b75aeac6"
SCHEMA = "airk.private.compact-fp16-output-projection-gemm/1"
MARKER = b"AIRK_COMPACT_OUTPUT_PROJECTION_GEMM "
HELPERS = ("projection_gemm_cuda.py", "projection_gemm_inputs.py", "projection_output_operands.py")


def base_owner():
    path = HERE / "probe_projection_gemm.py"
    if hashlib.sha256(path.read_bytes()).hexdigest() != BASE_SHA:
        raise ValueError("reviewed Compact projection bootstrap differs")
    spec = importlib.util.spec_from_file_location("compact_output_projection_base", path)
    base = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = base
    spec.loader.exec_module(base)
    return base


def ideal_rounding(base, np, activation, weights, outputs):
    left = base.half_array_units(np, activation).reshape(32, 2048)
    right = base.half_array_units(np, weights).reshape(2048, 2048)
    bound = 2048 * int(np.abs(left).max()) * int(np.abs(right).max())
    base.require(bound < 1 << 63, "checked output projection dot exceeds signed accumulation")
    dots = (left @ right).reshape(-1)
    target = np.fromiter((base.rounded_half_word(int(dot)) for dot in dots), dtype="<u2", count=dots.size)
    direct = np.fromiter((base.rounded_float_word(int(dot)) for dot in dots), dtype="<u4", count=dots.size)
    via_float = direct.view("<f4").astype("<f2").view("<u2")
    results = {}
    base.require(set(outputs) == set(base.ALL_MODES)
                 and all(set(mode) == {"o"} for mode in outputs.values()), "exact output mode roster required")
    for mode in base.ALL_MODES:
        raw = outputs[mode]["o"]
        data = np.frombuffer(raw, dtype="<f4" if mode in base.FLOAT_MODES else "<f2")
        base.require(data.size == dots.size and np.isfinite(data).all(), "complete finite output required")
        half = data.astype("<f2")
        base.require(np.isfinite(half).all(), "output projection cast must fit binary16")
        row = {
            "half_mismatches": int(np.count_nonzero(half.view("<u2") != target)),
            "max_abs_half_error": float(np.max(np.abs(half.astype("<f4") - target.view("<f2").astype("<f4")))),
        }
        if mode in base.FLOAT_MODES:
            row["fp32_mismatches"] = int(np.count_nonzero(data.view("<u4") != direct))
        results[mode] = row
    return {"words": int(dots.size), "ideal_fp32_then_half_double_rounding_words":
            int(np.count_nonzero(via_float != target)), "modes": results}


def worker(base, deadline, expected, helpers):
    def check():
        base.require(time.monotonic() < deadline, "original output projection deadline expired")

    base.require(0 < deadline - time.monotonic() <= base.DEADLINE_SECONDS, "bounded output projection deadline required")
    verifier, converter, plan, plan_pin, token = base.bootstrap(check)
    source = base.pinned(Path(__file__).resolve(), check)
    member_pins = [base.pinned(HERE / name, check) for name in HELPERS]
    base.require(source["sha256"] == expected and [pin["sha256"] for pin in member_pins] == helpers,
                 "owned output projection source differs")
    sys.addaudithook(verifier.read_only)
    root, manifest, libraries = base.cuda_libraries(verifier, converter, check)
    runtime, runtime_pin = verifier.read_pinned_json(
        plan["runtime_manifest"]["path"], plan["runtime_manifest"]["sha256"], check)
    base.require(runtime_pin == plan["runtime_manifest"], "reviewed output projection import closure differs")
    gpu = base.load(HERE / HELPERS[0], "compact_output_projection_cuda")
    loader = base.load(HERE / HELPERS[-1], "compact_output_projection_inputs")
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import numpy as np
    import onnx

    imports = dict(runtime["files"])
    imports.update({p["path"]: p for p in (source, *member_pins)})
    loaded = verifier.loaded_runtime(imports, base.pinned, check,
                                     (sys.modules[name] for name in sys.modules.keys() - before))
    capture = base.CAMPAIGN / "compact-fp16-layer0-native-capture-r4/native-result.json"
    graph, weights = base.CANDIDATE / "model.onnx", base.CANDIDATE / "weights.bin"
    for path in (capture, graph, weights):
        converter.windows_owner(path=path)
    activation, matrix = loader.load_output_operands(
        onnx, capture, CAPTURE_SHA, graph, base.GRAPH_SHA, weights, base.WEIGHTS_SHA)
    check()
    float_activation = np.frombuffer(activation, dtype="<f2").astype("<f4").tobytes()
    float_matrix = np.frombuffer(matrix, dtype="<f2").astype("<f4").tobytes()
    started = time.monotonic()
    with os.add_dll_directory(str(root)):
        outputs = gpu.run_modes(root / "cudart64_12.dll", root / "cublas64_12.dll", activation,
                                {"o": (2048, matrix)}, (float_activation, {"o": float_matrix}),
                                output_projection=True)
    native_seconds = time.monotonic() - started
    observation = ideal_rounding(base, np, activation, matrix, outputs)
    for path, expected_sha in ((capture, CAPTURE_SHA), (graph, base.GRAPH_SHA), (weights, base.WEIGHTS_SHA)):
        base.require(base.pinned(path, check)["sha256"] == expected_sha, "retained output projection operand changed")
    for pin in (source, plan_pin, runtime_pin, manifest, *member_pins, *libraries.values()):
        base.require(base.pinned(Path(pin["path"]), check) == pin, "output projection dependency changed")
    return {
        "schema": SCHEMA, "qualification": False, "product_admission": False, "native_model_forward": False,
        "captured_original_onnx_projection_output": False, "pid": os.getpid(), "execution_token": token,
        "source": source, "helpers": member_pins, "original_deadline": deadline, "capture_sha256": CAPTURE_SHA,
        "candidate_graph_sha256": base.GRAPH_SHA, "weights_sha256": base.WEIGHTS_SHA,
        "cuda_manifest": manifest, "libraries": libraries, "verified_onnx_imports": len(loaded),
        "native_calls_elapsed_seconds": native_seconds, **observation,
    }


def run_owned(base, out, hashes):
    started = time.monotonic()
    deadline = started + base.DEADLINE_SECONDS

    def check():
        base.require(time.monotonic() < deadline, "original output projection deadline expired")

    _, converter, plan, plan_pin, token = base.bootstrap(check)
    base.require(out.is_absolute() and out.parent == base.CAMPAIGN and out.resolve(strict=False) == out
                 and not out.exists(), "fresh checked output projection directory required")
    helpers_root = base.ROOT / "scripts/cuda-native-execution/helpers"
    for name, expected in converter.HELPERS.items():
        pin = plan["helpers"][name]
        base.require(base.pinned(helpers_root / name, check) == pin and pin["sha256"] == expected,
                     "reviewed Windows execution helper differs")
    sys.path.insert(0, str(helpers_root))
    parent = base.load(helpers_root / "reference_parent_no_site_r2.py", "compact_output_projection_parent")
    owner = base.load(helpers_root / "reference_owned.py", "compact_output_projection_job")
    base.require(Path(parent.worker.__file__) == helpers_root / "reference_worker.py", "Windows worker helper shadowed")
    source_path = Path(__file__).resolve()
    converter.windows_owner(path=source_path)
    source = base.pinned(source_path, check)
    member_pins = []
    for name, expected in zip(HELPERS, hashes, strict=True):
        path = HERE / name
        converter.windows_owner(path=path)
        pin = base.pinned(path, check)
        base.require(pin["sha256"] == expected, "reviewed output projection helper differs: " + name)
        member_pins.append(pin)
    temporary, log, receipt_path = (out / leaf for leaf in ("temp", "stdout-stderr.log", "execution.json"))
    for path in (out, temporary):
        converter.guard(plan, path, deadline=deadline)
        path.mkdir()
        converter.guard(plan, path, True, deadline=deadline)
    for path in (log, receipt_path):
        converter.guard(plan, path, deadline=deadline)
    argv = [plan["python"]["path"], "-I", "-B", "-S", "-X", "pycache_prefix=" + plan["cache_prefix"],
            source["path"], "--worker", "--deadline-monotonic", repr(deadline), "--source-sha256", source["sha256"],
            "--helper-sha256", *hashes]
    environment = {key: value for key, value in os.environ.items()
                   if not key.upper().startswith(("AIRK_", "ORT_", "CUDA_", "CUDNN_", "CUBLAS_", "PYTHON"))}
    environment.update(CUDA_CACHE_DISABLE="1", TEMP=str(temporary), TMP=str(temporary), TMPDIR=str(temporary))
    receipt = {"schema": SCHEMA, "complete": False, "qualification": False, "command": argv, "source": source,
               "helpers": member_pins, "plan": plan_pin, "original_deadline": deadline, "host_bytes": base.HOST_BYTES}

    def suspended(api, job, info, child_deadline):
        result = parent.suspended_authentication(api, info, deadline, child_deadline, converter.SID,
                                                 plan["python"], owner.image_proof)
        base.require(converter.windows_owner(process=info.process) == token
                     and owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1), "owned child or default owner differs")
        return dict(result, token=token)

    try:
        with log.open("xb") as destination, open(os.devnull, "rb") as stdin:
            converter.guard(plan, log, True, deadline=deadline)
            owned = owner.run_windows_owned_training(
                argv, base.ROOT, destination, deadline - time.monotonic(), environment, stdin, base.HOST_BYTES,
                before_resume=suspended, absolute_deadline=deadline, cleanup_seconds=5)
        receipt["owned"] = owned
        base.require(parent.settled(owned) and 0 < owned["peak_host_committed_bytes"] <= base.HOST_BYTES
                     and 0 < log.stat().st_size <= 1 << 20 and not any(temporary.iterdir()),
                     "output projection child or output did not settle within bounds")
        lines = [line[len(MARKER):] for line in log.read_bytes().splitlines() if line.startswith(MARKER)]
        base.require(len(lines) == 1, "one output projection observation required")
        result = json.loads(lines[0])
        base.require(result["schema"] == SCHEMA and result["qualification"] is result["product_admission"] is False
                     and result["native_model_forward"] is result["captured_original_onnx_projection_output"] is False
                     and result["pid"] == owned["owned_leader_pid"] and result["execution_token"] == token
                     and result["original_deadline"] == deadline and result["source"] == source
                     and result["helpers"] == member_pins and result["words"] == 65536,
                     "owned output projection observation differs from the request")
        receipt["result"] = result
        receipt["log"] = base.pinned(log, check)
        receipt["complete"] = True
    except (OSError, ValueError, KeyError, TimeoutError, RuntimeError) as problem:
        receipt["failure"] = str(problem)
    receipt["elapsed_seconds"] = time.monotonic() - started
    if receipt["elapsed_seconds"] >= base.DEADLINE_SECONDS:
        receipt["complete"] = False
        receipt.setdefault("failure", "original output projection deadline expired")
    with receipt_path.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(receipt, stream, allow_nan=False)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    converter.guard(plan, receipt_path, True, deadline=deadline)
    print(json.dumps({"complete": receipt["complete"], "failure": receipt.get("failure"), "output": str(out)}))
    return receipt["complete"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--owned-output", type=Path)
    mode.add_argument("--worker", action="store_true")
    parser.add_argument("--helper-sha256", nargs=3, required=True)
    parser.add_argument("--source-sha256")
    parser.add_argument("--deadline-monotonic", type=float)
    args = parser.parse_args()
    base = base_owner()
    if args.owned_output is not None:
        raise SystemExit(0 if run_owned(base, args.owned_output, args.helper_sha256) else 1)
    if args.deadline_monotonic is None or args.source_sha256 is None:
        parser.error("owned worker deadline and source pin required")
    result = worker(base, args.deadline_monotonic, args.source_sha256, args.helper_sha256)
    print(MARKER.decode("ascii") + json.dumps(result, allow_nan=False), flush=True)


if __name__ == "__main__":
    main()
