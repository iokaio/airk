# SPDX-License-Identifier: Apache-2.0
"""Compare two cuBLAS FP32 accumulation modes on exact Compact FP16 operands."""

import argparse
from array import array
import json
import math
import os
from pathlib import Path
import struct
import sys
import time
import importlib.util

ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
CONVERSION_PLAN = CAMPAIGN / "compact-fp16-present-conversion-plan-r2/compact.json"
PLAN_SHA = "b43ddbceb00a82b9ae5cc700bb3e6b633d1ed966cc028592c6e0ab5fe836e715"
CAPTURE = CAMPAIGN / "compact-fp16-layer0-projection-capture-r5/native-result.json"
CAPTURE_SHA = "8a3d7eef63fb0dca71fc5184bb37c151fa75903f95ecbf20dc39a29e09736d0e"
CPU_PROOF = CAMPAIGN / "compact-fp16-layer0-projection-reference-r7/compact-layer0-projections.json"
CPU_PROOF_SHA = "b2c117de8372bc63fd6451c7b7d06ac5316a8e5a45d648585ae80396ff319742"
CANDIDATE = CAMPAIGN / "compact-fp16-present-conversion-owned-r2/candidate"
GRAPH_SHA = "971cf2d22d8bb7c874f3e5f5ce014bf2c494d1fb7245126d27d722223ac1577f"
WEIGHTS_SHA = "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3"
CUDA_MANIFEST_SHA = "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5"
VERIFIER_SHA = "50d5c7237cd057a1d08ea79794bbdda0f1e141438a2d22580a760e418f9e3d75"
LIBRARIES = ("cublaslt64_12.dll", "cudart64_12.dll", "cublas64_12.dll")
HELPERS = ("projection_gemm_cuda.py", "projection_gemm_inputs.py")
MODES = ("GEMM2", "FP32_PEDANTIC")
FLOAT_MODES = ("GEMM2_FP32_OUTPUT", "FP32_PEDANTIC_FP32_OUTPUT")
ALL_MODES = MODES + FLOAT_MODES
PROJECTIONS = ("k", "v", "q")
MARKER = b"AIRK_COMPACT_PROJECTION_GEMM "
SCHEMA = "airk.private.compact-fp16-projection-gemm-experiment/1"
HOST_BYTES = 32 << 30
DEADLINE_SECONDS = 120


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def pinned(path, check):
    import hashlib

    check()
    require(path.is_absolute() and path.resolve(strict=True) == path
            and not any(p.is_symlink() or p.is_junction() for p in (path, *path.parents))
            and path.is_file(), "canonical and regular experiment input required")
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    check()
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest}


def bootstrap(check):
    require(sys.platform == "win32" and sys.byteorder == "little"
            and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "isolated Windows Python required")
    path = Path(__file__).with_name("verify_compact.py")
    require(pinned(path, check)["sha256"] == VERIFIER_SHA, "reviewed verifier source differs")
    verifier = load(path, "projection_experiment_verifier")
    plan, plan_pin = verifier.read_pinned_json(CONVERSION_PLAN, PLAN_SHA, check)
    converter_path = ROOT / "scripts/arbiter-fp16-execution.py"
    require(pinned(converter_path, check) == plan["wrapper"], "reviewed conversion owner differs")
    converter = load(converter_path, "projection_experiment_converter")
    token = converter.windows_owner()
    require(pinned(Path(sys.executable), check) == plan["python"]
            and plan["repository_root"] == str(ROOT)
            and sys.pycache_prefix == plan["cache_prefix"]
            and not Path(plan["cache_prefix"]).exists(), "reviewed Python or campaign differs")
    return verifier, converter, plan, plan_pin, token


def cuda_libraries(verifier, converter, check):
    root = Path(os.environ["LOCALAPPDATA"]) / "airk-final-exec-20261116T052942Z/cu15/gpu-provider-stage"
    manifest, seal = verifier.read_pinned_json(root / "airk-cuda-runtime.json", CUDA_MANIFEST_SHA, check)
    converter.windows_owner(path=seal["path"])
    expected = {row["name"]: row for row in manifest["files"]}
    entries = {}
    for name in LIBRARIES:
        path = root / name
        converter.windows_owner(path=path)
        observed = pinned(path, check)
        require(all(observed[key] == expected[name][key] for key in ("bytes", "sha256")),
                "reviewed CUDA library differs: " + name)
        entries[name] = observed
    return root, seal, entries


def words(data):
    result = array("H")
    result.frombytes(data)
    return result


def compare(verifier, source, weights, observed, outputs, cpu_proof):
    require(set(outputs) == set(MODES) and all(set(outputs[mode]) == set(PROJECTIONS) for mode in MODES),
            "exact projection modes and roster required")
    captured = {name: words(observed[name]) for name in PROJECTIONS}
    results = {mode: {name: words(outputs[mode][name]) for name in PROJECTIONS} for mode in MODES}
    differences = {}
    for name in PROJECTIONS:
        count = 32 * weights[name][0]
        require(len(captured[name]) == count and all(len(results[mode][name]) == count for mode in MODES),
                "projection output count differs")
        require(all(word & 0x7c00 != 0x7c00 for mode in MODES for word in results[mode][name]),
                "finite projection experiment result required")
        differences[name] = {
            "elements": count,
            "baseline_vs_ort": sum(a != b for a, b in zip(results["GEMM2"][name], captured[name], strict=True)),
            "pedantic_vs_ort": sum(a != b for a, b in zip(results["FP32_PEDANTIC"][name], captured[name], strict=True)),
            "pedantic_vs_baseline": sum(a != b for a, b in zip(
                results["FP32_PEDANTIC"][name], results["GEMM2"][name], strict=True)),
        }
    source_words = words(source)
    witnesses = []
    for name, old in zip(PROJECTIONS, cpu_proof["projection_witnesses"], strict=True):
        width, raw_weights = weights[name]
        token, column = old["token"], old["column"]
        require(old["name"].endswith(f"/self_attn/{name}_proj/MatMul/output16")
                and type(token) is int and 0 <= token < 32
                and type(column) is int and 0 <= column < width, "projection witness identity differs")
        index = token * width + column
        require(captured[name][index] == old["cuda_half_word"], "retained GPU witness differs")
        x = source_words[token * 2048:(token + 1) * 2048]
        w = words(raw_weights)[column::width]
        witnesses.append({
            "projection": name, "token": token, "column": column, "cpu_half_word": old["cpu_half_word"],
            "modes": {
                mode: {
                    "half_word": results[mode][name][index],
                    **verifier.exact_dot_distances(x, w, old["cpu_half_word"], results[mode][name][index]),
                }
                for mode in MODES
            },
        })
    return {
        "baseline_reproduces_ort": all(row["baseline_vs_ort"] == 0 for row in differences.values()),
        "differences": differences, "exact_dot_witnesses": witnesses,
    }


def rounded_dot(dot, precision, minimum_shift):
    """Round an exact signed dot in units of 2**-48 to the requested precision."""
    magnitude = abs(dot)
    shift = max(minimum_shift, magnitude.bit_length() - precision)
    quotient = magnitude
    if shift:
        quotient, remainder = divmod(magnitude, 1 << shift)
        midpoint = 1 << (shift - 1)
        if remainder > midpoint or remainder == midpoint and quotient & 1:
            quotient += 1
    rounded = math.ldexp(float(quotient), shift - 48)
    return -rounded if dot < 0 else rounded


def rounded_half_word(dot):
    return struct.unpack("<H", struct.pack("<e", rounded_dot(dot, 11, 24)))[0]


def rounded_float_word(dot):
    # The smallest nonzero input unit, 2**-48, is already normal in binary32.
    return struct.unpack("<I", struct.pack("<f", rounded_dot(dot, 24, 0)))[0]


def half_array_units(np, data):
    half_bits = np.frombuffer(data, dtype="<u2")
    exponent = ((half_bits >> 10) & 31).astype(np.int64)
    require(np.all(exponent < 31), "finite half operands required for exact dot comparison")
    fraction = (half_bits & 1023).astype(np.int64)
    magnitude = np.where(exponent == 0, fraction, (1024 + fraction) << np.maximum(exponent - 1, 0))
    return np.where(half_bits & 0x8000, -magnitude, magnitude)


def exact_projection_rounding(np, source, weights, raw_outputs, half_outputs, check):
    activation = half_array_units(np, source).reshape(32, 2048)
    largest_activation = int(np.abs(activation).max())
    half_summary, float_summary = {}, {}
    for name in PROJECTIONS:
        check()
        width, data = weights[name]
        matrix = half_array_units(np, data).reshape(2048, width)
        bound = 2048 * largest_activation * int(np.abs(matrix).max())
        require(bound < 1 << 63, "checked projection integer dot exceeds signed accumulation")
        dots = (activation @ matrix).reshape(-1)
        half_reference = np.fromiter((rounded_half_word(int(value)) for value in dots),
                                     dtype="<u2", count=dots.size)
        float_reference = np.fromiter((rounded_float_word(int(value)) for value in dots),
                                      dtype="<u4", count=dots.size)
        via_float = float_reference.view("<f4").astype("<f2").view("<u2")
        half_summary[name] = {
            mode: int(np.count_nonzero(half_reference != np.frombuffer(half_outputs[mode][name], dtype="<u2")))
            for mode in ALL_MODES
        }
        float_summary[name] = {
            "ideal_fp32_then_half_vs_ideal_half": int(np.count_nonzero(via_float != half_reference)),
            "fp32_output_vs_exact_rounded_fp32": {
                mode: int(np.count_nonzero(float_reference != np.frombuffer(raw_outputs[mode][name], dtype="<u4")))
                for mode in FLOAT_MODES
            },
            "fp32_output_cast_vs_ideal_fp32_cast": {
                mode: int(np.count_nonzero(via_float != np.frombuffer(half_outputs[mode][name], dtype="<u2")))
                for mode in FLOAT_MODES
            },
        }
    check()
    return half_summary, float_summary


def half_outputs_for_comparison(np, raw_outputs, weights):
    require(set(raw_outputs) == set(ALL_MODES)
            and all(set(raw_outputs[mode]) == set(PROJECTIONS) for mode in ALL_MODES),
            "exact projection modes and roster required")
    half_outputs = {mode: raw_outputs[mode] for mode in MODES}
    for mode in FLOAT_MODES:
        half_outputs[mode] = projected = {}
        for name in PROJECTIONS:
            output = np.frombuffer(raw_outputs[mode][name], dtype="<f4")
            require(output.size == 32 * weights[name][0] and np.all(np.isfinite(output)),
                    "finite FP32 projection experiment output with exact shape required")
            half = output.astype("<f2")
            require(np.all(np.isfinite(half)), "FP32 projection output must fit finite binary16")
            projected[name] = half.tobytes()
    return half_outputs


def worker(deadline, expected_source):
    def check():
        require(time.monotonic() < deadline, "original projection experiment deadline expired")

    require(0 < deadline - time.monotonic() <= DEADLINE_SECONDS, "bounded projection deadline required")
    verifier, converter, plan, plan_pin, token = bootstrap(check)
    source_pin = pinned(Path(__file__).resolve(), check)
    require(source_pin["sha256"] == expected_source, "owned projection worker source differs")
    sys.addaudithook(verifier.read_only)
    root, cuda_manifest, libraries = cuda_libraries(verifier, converter, check)
    cpu_proof, cpu_pin = verifier.read_pinned_json(CPU_PROOF, CPU_PROOF_SHA, check)
    require(cpu_proof["layer0_capture"]["sha256"] == CAPTURE_SHA, "original projection proof capture differs")
    runtime, runtime_pin = verifier.read_pinned_json(
        plan["runtime_manifest"]["path"], plan["runtime_manifest"]["sha256"], check)
    require(runtime_pin == plan["runtime_manifest"], "reviewed import closure differs")
    member_pins = [pinned(Path(__file__).with_name(name), check) for name in HELPERS]
    operators = [load(Path(row["path"]), "compact_" + Path(row["path"]).stem) for row in member_pins]
    gpu, loader = operators
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import numpy as np
    import onnx

    import_sources = dict(runtime["files"])
    for row in (source_pin, *member_pins):
        import_sources[row["path"]] = row
    imports = verifier.loaded_runtime(import_sources, pinned, check,
                                      (sys.modules[name] for name in sys.modules.keys() - before))
    for path in (CAPTURE, CANDIDATE / "model.onnx", CANDIDATE / "weights.bin"):
        converter.windows_owner(path=path)
    activation, weights, observed = loader.load_operands(
        onnx, CAPTURE, CAPTURE_SHA, CANDIDATE / "model.onnx", GRAPH_SHA, CANDIDATE / "weights.bin", WEIGHTS_SHA)
    check()
    started = time.monotonic()
    with os.add_dll_directory(str(root)):
        outputs = gpu.run_modes(root / "cudart64_12.dll", root / "cublas64_12.dll", activation, weights)
    native_seconds = time.monotonic() - started
    half_outputs = half_outputs_for_comparison(np, outputs, weights)
    result = compare(verifier, activation, weights, observed, {mode: half_outputs[mode] for mode in MODES}, cpu_proof)
    require(result["baseline_reproduces_ort"], "GEMM2 no longer reproduces the sealed ONNX Runtime projections")
    result["exact_rounded_word_mismatches"], result["fp32_output_rounding"] = exact_projection_rounding(
        np, activation, weights, outputs, half_outputs, check)
    for path, digest in ((CAPTURE, CAPTURE_SHA), (CANDIDATE / "model.onnx", GRAPH_SHA),
                         (CANDIDATE / "weights.bin", WEIGHTS_SHA)):
        require(pinned(path, check)["sha256"] == digest, "projection operand file changed")
    for row in (source_pin, plan_pin, cuda_manifest, cpu_pin, runtime_pin, *libraries.values(), *member_pins):
        require(pinned(Path(row["path"]), check) == row, "projection experiment input changed")
    return {
        "schema": SCHEMA, "qualification": False, "product_admission": False,
        "pid": os.getpid(), "execution_token": token, "original_deadline": deadline,
        "capture_sha256": CAPTURE_SHA, "candidate_graph_sha256": GRAPH_SHA, "weights_sha256": WEIGHTS_SHA,
        "cuda_manifest": cuda_manifest, "libraries": libraries, "owner": source_pin, "helpers": member_pins,
        "verified_onnx_imports": len(imports), "native_calls_elapsed_seconds": native_seconds, **result,
    }


def run_owned(out):
    started = time.monotonic()
    deadline = started + DEADLINE_SECONDS

    def check():
        require(time.monotonic() < deadline, "original projection experiment deadline expired")

    verifier, converter, plan, plan_pin, token = bootstrap(check)
    require(out.is_absolute() and out.is_relative_to(CAMPAIGN) and out.resolve(strict=False) == out
            and out.parent.is_dir() and not out.exists(), "fresh checked projection output required")
    helpers = ROOT / "scripts/cuda-native-execution/helpers"
    for name, expected in converter.HELPERS.items():
        row = plan["helpers"][name]
        require(pinned(helpers / name, check) == row and row["sha256"] == expected,
                "reviewed Windows execution helper differs")
    sys.path.insert(0, str(helpers))
    parent = load(helpers / "reference_parent_no_site_r2.py", "projection_experiment_parent")
    owner = load(helpers / "reference_owned.py", "projection_experiment_job")
    require(Path(parent.worker.__file__) == helpers / "reference_worker.py", "Windows worker helper shadowed")
    converter.guard(plan, out, deadline=deadline)
    out.mkdir()
    converter.guard(plan, out, True, deadline=deadline)
    temporary = out / "temp"
    converter.guard(plan, temporary, deadline=deadline)
    temporary.mkdir()
    converter.guard(plan, temporary, True, deadline=deadline)
    log, receipt_path = out / "stdout-stderr.log", out / "execution.json"
    for path in (log, receipt_path):
        converter.guard(plan, path, deadline=deadline)
    source_path = Path(__file__).resolve()
    converter.windows_owner(path=source_path)
    source = pinned(source_path, check)
    members = []
    for name in HELPERS:
        path = source_path.with_name(name)
        converter.windows_owner(path=path)
        members.append(pinned(path, check))
    argv = [plan["python"]["path"], "-I", "-B", "-S", "-X", "pycache_prefix=" + plan["cache_prefix"],
            source["path"], "--worker", "--deadline-monotonic", repr(deadline), "--owner-sha256", source["sha256"]]
    environment = {key: value for key, value in os.environ.items()
                   if not key.upper().startswith(("AIRK_", "ORT_", "CUDA_", "CUDNN_", "CUBLAS_", "PYTHON"))}
    environment.update(CUDA_CACHE_DISABLE="1", TEMP=str(temporary), TMP=str(temporary), TMPDIR=str(temporary))
    receipt = {"schema": SCHEMA, "complete": False, "qualification": False, "source": source, "command": argv,
               "plan": plan_pin, "helpers": members, "original_deadline": deadline,
               "host_bytes": HOST_BYTES, "failure": None}

    def suspended(api, job, info, child_deadline):
        observed = parent.suspended_authentication(api, info, deadline, child_deadline, converter.SID,
                                                   plan["python"], owner.image_proof)
        require(converter.windows_owner(process=info.process) == token
                and owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1), "original owned child differs")
        return dict(observed, token=token)

    try:
        with log.open("xb") as destination, open(os.devnull, "rb") as stdin:
            converter.guard(plan, log, True, deadline=deadline)
            owned = owner.run_windows_owned_training(
                argv, ROOT, destination, deadline - time.monotonic(), environment, stdin, HOST_BYTES,
                before_resume=suspended, absolute_deadline=deadline, cleanup_seconds=5)
        receipt["owned"] = owned
        require(parent.settled(owned) and 0 < owned["peak_host_committed_bytes"] <= HOST_BYTES,
                "owned projection child failed or did not settle within bounds")
        require(0 < log.stat().st_size <= 1 << 20 and not any(temporary.iterdir()),
                "projection log or temporary output differs")
        rows = [line[len(MARKER):] for line in log.read_bytes().splitlines() if line.startswith(MARKER)]
        require(len(rows) == 1, "exact projection experiment observation required")
        result = json.loads(rows[0])
        require(result["schema"] == SCHEMA and result["qualification"] is result["product_admission"] is False
                and result["pid"] == owned["owned_leader_pid"] and result["execution_token"] == token
                and result["original_deadline"] == deadline and result["owner"] == source
                and result["helpers"] == members,
                "projection observation differs from the owned request")
        for row in (source, plan_pin, *members):
            require(pinned(Path(row["path"]), check) == row, "owned projection source input changed")
        receipt["result"] = result
        receipt["log"] = pinned(log, check)
        receipt["complete"] = True
    except (OSError, ValueError, KeyError, TimeoutError, RuntimeError) as problem:
        receipt["failure"] = str(problem)
    receipt["elapsed_seconds"] = time.monotonic() - started
    if receipt["elapsed_seconds"] >= DEADLINE_SECONDS:
        receipt["complete"] = False
        receipt["failure"] = receipt["failure"] or "original projection experiment deadline expired"
    with receipt_path.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(receipt, stream, allow_nan=False)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    converter.guard(plan, receipt_path, True, deadline=deadline)
    print(json.dumps({"complete": receipt["complete"], "failure": receipt["failure"], "output": str(out)}))
    return receipt["complete"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--owned-output", type=Path)
    mode.add_argument("--worker", action="store_true")
    parser.add_argument("--deadline-monotonic", type=float)
    parser.add_argument("--owner-sha256")
    args = parser.parse_args()
    if args.owned_output is not None:
        raise SystemExit(0 if run_owned(args.owned_output) else 1)
    if args.deadline_monotonic is None or args.owner_sha256 is None:
        parser.error("owned worker deadline and source pin required")
    result = worker(args.deadline_monotonic, args.owner_sha256)
    print(MARKER.decode("ascii") + json.dumps(result, allow_nan=False), flush=True)


if __name__ == "__main__":
    main()
