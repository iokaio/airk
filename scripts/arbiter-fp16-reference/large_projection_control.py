# SPDX-License-Identifier: Apache-2.0
"""Check real Large T32 layer-zero CPU projection words against exact integer dot products."""

import argparse
import hashlib
import json
import os
import runpy
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
REFERENCE = "c214278ab06a24053d0a309b294e459ec7ba717e99f4e39edd3fedaa26c878bf"
MATH = "06ee65a7c9cad968affb0109e1c9628dad6475c8c999116191a57119a55568cc"
CONTROL = "aa47ae8ba52c27ff38e9ac13a1ffdd0737bfa21a264bc04074d885e8525158ec"
TOKENS = (0, 15, 31)


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def load(pin):
    path = Path(pin["path"])
    require(digest(path) == pin["sha256"], "changed input: " + path.name)
    return json.loads(path.read_bytes())


GPU_UUID = "GPU-08e2b3f6-6a39-3456-77cb-a57287a702ec"


def execute(path, capture=None, capture_sha=None, gpu_treatment=False, dense=None, dense_sha=None, panel_mask=0):
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and os.environ.get("CUDA_VISIBLE_DEVICES") == (GPU_UUID if gpu_treatment else ""),
            "isolated interpreter and exactly requested device visibility required")
    ref = load({"path": path, "sha256": REFERENCE})
    inputs = load(ref["source_inputs"])
    plan = load(ref["conversion_plan"])
    runtime = load(plan["runtime_manifest"])
    require(str(Path(sys.executable)) == plan["python"]["path"] and digest(Path(sys.executable)) == plan["python"]["sha256"],
            "reference interpreter differs")
    support_path = ROOT / "scripts/arbiter-fp16-execution/verify_compact.py"
    require(digest(support_path) == "50d5c7237cd057a1d08ea79794bbdda0f1e141438a2d22580a760e418f9e3d75"
            and sys.pycache_prefix == plan["cache_prefix"] and not Path(sys.pycache_prefix).exists(),
            "verified runtime and absent pycache prefix required")
    support = runpy.run_path(str(support_path))
    cache = Path(os.environ["TMP"]).resolve(strict=True)
    require(cache.is_relative_to(ROOT / "artifacts") and cache.name == "temp", "private cache required")
    sys.addaudithook(lambda event, args: support["read_only"](event, args, cache))
    placement = None
    if panel_mask:
        placement_source = Path(__file__).with_name("large_qkv_reference.py")
        require(digest(placement_source) == "70d2a406c8fc42de7059c2d5ed15d5a5a9af53a00d9915fe780438109fe5d7ce", "CPU placement owner changed")
        placement = runpy.run_path(str(placement_source))["cpu_limits"](panel_mask)
    preloaded = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import numpy as np
    import onnx
    import torch
    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    math_path = ROOT / "scripts/arbiter-fp16-reference.py"
    control_path = ROOT / "scripts/arbiter-fp16-execution/probe_projection_gemm.py"
    require(digest(math_path) == MATH and digest(control_path) == CONTROL, "arithmetic source changed")
    half = runpy.run_path(str(math_path))
    exact = runpy.run_path(str(control_path))
    half["freeze_reference_math"](torch)
    graph = Path(ref["graph"]["path"])
    require(digest(graph) == ref["graph"]["sha256"], "Large graph differs")
    model = onnx.load_model(graph, load_external_data=False)
    tensors = {tensor.name: tensor for tensor in model.graph.initializer}
    params = {row["parameter"]: row for row in ref["verification"]["parameters"]}
    weights = Path(ref["weights"]["path"])
    require(weights == graph.parent / "weights.bin" and weights.stat().st_size == ref["weights"]["bytes"],
            "Large sidecar differs")

    def read_tensor(name, selected=None):
        p = params[name]
        t = tensors[p["initializer"]]
        ext = {entry.key: entry.value for entry in t.external_data}
        require(ext["location"] == weights.name and list(t.dims) == p["shape"]
                and (t.data_type, p["dtype"]) in ((1, "float32"), (10, "float16")),
                "unexpected tensor representation")
        dtype = np.dtype("<f4" if t.data_type == 1 else "<f2")
        count = int(ext["length"])
        require(count == int(np.prod(p["shape"])) * dtype.itemsize, "external span differs")
        h = hashlib.sha256()
        with weights.open("rb") as file:
            file.seek(int(ext["offset"]))
            remaining = count
            while remaining:
                part = file.read(min(1 << 20, remaining))
                require(part, "incomplete tensor")
                h.update(part)
                remaining -= len(part)
            require(h.hexdigest() == p["actual_initializer_sha256"] == p["independent_expected_sha256"],
                    "candidate weights differ from independent publisher")
            if selected is not None:
                stride = p["shape"][1] * dtype.itemsize
                values = []
                for i in selected:
                    require(0 <= i < p["shape"][0], "bad embedding row")
                    file.seek(int(ext["offset"]) + i * stride)
                    values.append(np.frombuffer(file.read(stride), dtype=dtype))
                return np.stack(values)
            file.seek(int(ext["offset"]))
            return np.frombuffer(file.read(count), dtype=dtype).reshape(p["shape"]).copy()

    def runtime_count():
        def pin_runtime(module_path, _check):
            return {"path":str(module_path),"bytes":module_path.stat().st_size,"sha256":digest(module_path)}
        allowed = dict(runtime["files"])
        source = Path(__file__).resolve()
        allowed[str(source)] = pin_runtime(source,None)
        return support["loaded_runtime"](allowed,pin_runtime,lambda:None,
                                         (sys.modules[k] for k in sys.modules.keys()-preloaded))

    if dense is not None:
        require(capture is not None and not gpu_treatment, "both original layer17 operand captures required")
        helper = Path(__file__).with_name("large_layer17_replay.py")
        require(digest(helper) == "012d9bf1ec47061752c2beaa6d03d73bf5543f901409e3fbe894b70605d6e9a5", "layer17 replay source differs")
        observed = runpy.run_path(str(helper))["replay"](np,torch,half,exact,read_tensor,load,
            path.parent.parent,[{"path":str(capture),"sha256":capture_sha},{"path":str(dense),"sha256":dense_sha}],ref)
        observed["loaded_runtime_count"] = len(runtime_count())
        observed["arithmetic_source"] = digest(helper)
        return observed
    row, = [r["input"] for r in inputs["rows"] if r["input"]["input_digest"] == ref["input_digest"]]
    require(len(row["input_ids"]) == 32 and row["attention_mask"] == [1] * 32, "all-live T32 required")
    if panel_mask:
        require(capture is not None and not gpu_treatment and dense is None and torch.get_num_threads()==2 and torch.get_num_interop_threads()==1, "original CPU panel requires two verified quiet cores and no GPU")
        panel = Path(__file__).with_name("large_roundback_panel.py")
        panel_sha = "4b4ff8f3d831f65e3f858f7fb8fed2d746b0641b30528b95c5b421f6e43ec822"
        require(digest(panel)==panel_sha, "panel changed")
        capture_owner = runpy.run_path(str(ROOT / "scripts/arbiter-fp16-execution/large_layer0_capture.py"))
        capture_proof, full = capture_owner["read_capture"](capture,capture_sha,all_rows=True)
        observed = runpy.run_path(str(panel))["measure"](np,torch,half,exact,read_tensor,load,path.parent.parent,row,full,capture_owner)
        require(digest(panel)==panel_sha, "panel changed during arithmetic")
        return dict(observed,placement=placement,source_sha256=panel_sha,capture=capture_proof,loaded_runtime_count=len(runtime_count()))
    embed = torch.from_numpy(read_tensor("embed_tokens.weight", row["input_ids"])).float().unsqueeze(0)
    norm = torch.from_numpy(read_tensor("layers.0.input_layernorm.weight"))
    activation = half["rms"](torch, embed, norm, 1e-6).half().contiguous()
    left = exact["half_array_units"](np, activation.numpy().tobytes()).reshape(32, 2560)[list(TOKENS)]
    capture_proof = None
    capture_inputs = None
    capture_rows = None
    capture_differences = {}
    if capture is not None:
        capture_owner = runpy.run_path(str(ROOT / "scripts/arbiter-fp16-execution/large_layer0_capture.py"))
        capture_proof, capture_rows = capture_owner["read_capture"](capture, capture_sha)
        native_activation = np.asarray(capture_rows[capture_owner["SHARED"]], dtype="<u2")
        cpu_activation = activation.numpy().view("<u2")[0, list(TOKENS)]
        capture_inputs = {"words": int(cpu_activation.size),
                          "mismatched_words": int(np.count_nonzero(cpu_activation != native_activation)),
                          "max_abs_error": float(abs(cpu_activation.view("<f2").astype("<f4")
                                                     - native_activation.view("<f2")).max())}
        left_native = exact["half_array_units"](np, native_activation.tobytes()).reshape(len(TOKENS), 2560)
    treatment = None
    if gpu_treatment:
        require(capture_proof is not None, "actual Large operands required for a CUDA treatment")
        algorithm = ROOT / "scripts/arbiter-fp16-execution/large_projection_gemm.py"
        gpu_sha = "7315a752db13779295104d06bc24947c4af6b1312fffaa0b7551c4bf0473c581"
        require(digest(algorithm) == gpu_sha, "GPU treatment program changed")
        _, full = capture_owner["read_capture"](capture, capture_sha, all_rows=True)
        gpu_weights = {k: read_tensor("layers.0.self_attn." + k + "_proj.weight") for k in ("k", "v", "q")}
        gpu_activation = np.asarray(full[capture_owner["SHARED"]], dtype="<u2").reshape(32,2560).view("<f2")
        outputs, libraries = runpy.run_path(str(algorithm))["gemms"](
            np, capture.parent.parent / "windows-cu15-scientific-provider-stage-r1", gpu_activation, gpu_weights)
        require(digest(algorithm) == gpu_sha, "GPU treatment changed during execution")
        treatment = {"gpu_uuid": GPU_UUID, "libraries": libraries, "program_sha256": gpu_sha, "routes": {}}
    results = {}
    for route, width in (("k", 1024), ("v", 1024), ("q", 4096)):
        name = "layers.0.self_attn." + route + "_proj.weight"
        right = read_tensor(name)
        require(right.shape == (2560, width), "Large projection geometry differs")
        actual = torch.nn.functional.linear(activation, torch.from_numpy(right.T.copy())).numpy()
        actual = actual[0, list(TOKENS)].view("<u2")
        mismatches = 0
        max_error = 0.0
        native_misses = 0
        native_max_error = 0.0
        protected_misses = 0
        protected_max = 0.0
        for start in range(0, width, 64):
            col = right[:, start:start + 64].copy()
            units = exact["half_array_units"](np, col.tobytes()).reshape(col.shape)
            left_max = int(abs(left).max())
            if capture_rows is not None:
                left_max = max(left_max, int(abs(left_native).max()))
            bound = left_max * int(abs(units).sum(axis=0).max())
            require(bound < 1 << 63, "64-bit exact dot overflow")
            dots = left @ units
            expected = np.fromiter((exact["rounded_half_word"](int(v)) for v in dots.flat),
                                   dtype="<u2", count=dots.size).reshape(dots.shape)
            obs = actual[:, start:start + 64]
            mismatches += int(np.count_nonzero(obs != expected))
            max_error = max(max_error, float(abs(obs.view("<f2").astype("<f4") - expected.view("<f2")).max()))
            if capture_rows is not None:
                native_dots = left_native @ units
                gold = np.fromiter((exact["rounded_half_word"](int(v)) for v in native_dots.flat),
                                   dtype="<u2", count=native_dots.size).reshape(native_dots.shape)
                name = "/airk_g5/weight//scorer/layers.0/self_attn/" + route + "_proj/MatMul/output16"
                observed = np.asarray(capture_rows[name], dtype="<u2")[:, start:start + 64]
                if gpu_treatment:
                    candidate = outputs["fp32"][route][list(TOKENS), start:start + 64]
                    protected_misses += int(np.count_nonzero(candidate != gold))
                    protected_max = max(protected_max, float(abs(candidate.view("<f2").astype("<f4") - gold.view("<f2")).max()))
                native_misses += int(np.count_nonzero(observed != gold))
                native_max_error = max(native_max_error, float(abs(observed.view("<f2").astype("<f4") - gold.view("<f2")).max()))
        if gpu_treatment:
            native_full = np.asarray(full["/airk_g5/weight//scorer/layers.0/self_attn/" + route + "_proj/MatMul/output16"],
                                     dtype="<u2").reshape(32, width)
            treatment["routes"][route] = {"paired_words": int(native_full.size),
                "replayed_original_vs_captured_word_mismatches": int(np.count_nonzero(outputs["original"][route] != native_full)),
                "evaluated_exact_dots": int(actual.size), "fp32_then_half_vs_exact_mismatches": protected_misses,
                "fp32_then_half_vs_exact_max_error": protected_max}
        if capture_rows is not None:
            capture_differences[route] = {"words": int(actual.size), "native_vs_exact_mismatches_on_native_input": native_misses,
                                           "native_vs_exact_max_abs_error_on_native_input": native_max_error}
        results[route] = {"words": int(actual.size), "mismatched_words": mismatches, "max_abs_error": max_error}
    if gpu_treatment:
        treatment["original_reproduced"] = all(x["replayed_original_vs_captured_word_mismatches"] == 0
                                                for x in treatment["routes"].values())
        treatment["treatment_supported"] = treatment["original_reproduced"] and all(
            treatment["routes"][k]["fp32_then_half_vs_exact_mismatches"] < v["native_vs_exact_mismatches_on_native_input"]
            and treatment["routes"][k]["fp32_then_half_vs_exact_max_error"] <= v["native_vs_exact_max_abs_error_on_native_input"]
            for k, v in capture_differences.items())
    loaded = runtime_count()
    require(not torch.cuda.is_initialized(), "CPU-only control required")
    return {"loaded_runtime_count": len(loaded), "math_source": MATH, "rounding_control_source": CONTROL,
            "reference_sha256": REFERENCE, "input_digest": row["input_digest"], "graph": ref["graph"],
            "verified_parameter_count": 5, "token_positions": TOKENS, "activation_sha256":
            hashlib.sha256(activation.numpy().tobytes()).hexdigest(), "routes": results,
            "native_intermediates_observed": capture_proof is not None, "capture": capture_proof,
            "gpu_treatment": treatment, "normalized_input_comparison": capture_inputs, "native_projection_comparison": capture_differences,
            "qualification": False}


def run_owned(reference, log, capture=None, capture_sha=None, gpu_treatment=False, dense=None, dense_sha=None, panel_mask=0):
    ref = load({"path": reference, "sha256": REFERENCE})
    plan = load(ref["conversion_plan"])
    owner_path = ROOT / "scripts/arbiter-fp16-execution.py"
    require(digest(owner_path) == plan["wrapper"]["sha256"] and str(owner_path) == plan["wrapper"]["path"],
            "ownership bootstrap differs")
    converter = runpy.run_path(str(owner_path))
    helpers = ROOT / "scripts/cuda-native-execution/helpers"
    for name, row in plan["helpers"].items():
        require(Path(row["path"]) == helpers / name and digest(helpers / name) == converter["HELPERS"][name],
                "ownership helper changed")
    sys.path.insert(0, str(helpers))
    import reference_owned as owner
    import reference_parent_no_site_r2 as parent
    token = converter["windows_owner"]()
    deadline = time.monotonic() + 120
    source = Path(__file__).resolve()
    source_sha = digest(source)
    temp = str(log.parent / "temp")
    env = {key: value for key, value in os.environ.items() if not key.upper().startswith(
        ("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_", "TORCH_", "TRITON_", "HF_", "HUGGINGFACE_"))}
    env.update(CUDA_VISIBLE_DEVICES=GPU_UUID if gpu_treatment else "", HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1", TOKENIZERS_PARALLELISM="false",
               TORCH_HOME=temp, TORCHINDUCTOR_CACHE_DIR=temp, TRITON_CACHE_DIR=temp, TEMP=temp, TMP=temp, TMPDIR=temp)
    require(log.is_absolute() and log.parent.parent == reference.parent.parent
            and log.name == "stdout.log" and not log.exists(), "fresh private sibling experiment required")
    converter["guard"](plan, log, deadline=deadline)

    def suspended(api, job, info, kid_deadline):
        evidence = parent.suspended_authentication(api, info, deadline, kid_deadline, converter["SID"],
                                                   plan["python"], owner.image_proof)
        require(converter["windows_owner"](process=info.process) == token
                and owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1), "child identity or Job differs")
        return dict(evidence, token=token)

    command = [sys.executable, "-I", "-B", "-S", "-X", "pycache_prefix=" + plan["cache_prefix"],
               str(source), str(reference)]
    if capture is not None:
        command.extend(["--capture", str(capture), "--capture-sha256", capture_sha])
    if dense is not None:
        command.extend(["--dense",str(dense),"--dense-sha256",dense_sha])
    if gpu_treatment:
        command.append("--gpu-treatment")
    if panel_mask:
        command.extend(["--panel-mask",hex(panel_mask)])
    with open(os.devnull, "rb") as stdin, log.open("xb") as stdout:
        owned = owner.run_windows_owned_training(command, ROOT, stdout, deadline - time.monotonic(), env,
                                                stdin, 4 << 30, before_resume=suspended,
                                                absolute_deadline=deadline, cleanup_seconds=5)
    require(parent.settled(owned) and 0 < owned["peak_host_committed_bytes"] <= 4 << 30, "owned CPU control failed")
    converter["guard"](plan, log, True, deadline=deadline)
    require(log.stat().st_size < 65536 and digest(source) == source_sha, "CPU control evidence changed")
    result = json.loads(log.read_bytes())
    return {"source_sha256": source_sha, "command": command, "host_limit_bytes": 4 << 30,
            "elapsed_seconds": time.monotonic() - (deadline - 120), "owned": owned, "observation": result}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("--owned-log", type=Path)
    parser.add_argument("--capture", type=Path)
    parser.add_argument("--capture-sha256")
    parser.add_argument("--dense",type=Path)
    parser.add_argument("--dense-sha256")
    parser.add_argument("--gpu-treatment", action="store_true")
    parser.add_argument("--panel-mask", type=lambda value:int(value,0), default=0)
    args = parser.parse_args()
    require((args.capture is None) == (args.capture_sha256 is None), "pair native capture path and SHA-256")
    result = (run_owned(args.reference, args.owned_log, args.capture, args.capture_sha256, args.gpu_treatment,args.dense,args.dense_sha256,args.panel_mask) if args.owned_log
              else execute(args.reference, args.capture, args.capture_sha256, args.gpu_treatment,args.dense,args.dense_sha256,args.panel_mask))
    print(json.dumps(result, allow_nan=False))
