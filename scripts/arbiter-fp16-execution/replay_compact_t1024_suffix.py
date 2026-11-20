# SPDX-License-Identifier: Apache-2.0
"""Replay the Compact CPU suffix after the captured T1024 layer-zero attention output."""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
REPLAY_SHA = "1c344a187ea32694789571dcf22721945bd9302eeb4f76cd26ab2e3a9a383da0"
REFERENCE_SHA = "f0d0e01fa28e7a876f1c57bc9894dff7e2a915ac8bf8150885b2aac2fdff2f14"
CAPTURE_SHA = "6725b3ac6d793601a100ff7d6e812837d58b4aa33ec2b11af00d1936b2d15e03"
MARKER = "AIRK_SUFFIX_DIAGNOSTIC "


def require(test, message):
    if not test:
        raise ValueError(message)


def module(name, digest):
    path = HERE / name
    require(hashlib.sha256(path.read_bytes()).hexdigest() == digest, "replay source pin differs")
    spec = importlib.util.spec_from_file_location(path.stem, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def score_suffix(wider, half, independent, row, captured, check):
    original = wider.wider_flash_attention
    calls = 0

    def attention(torch, q, k, v, mask, scale, clock):
        nonlocal calls
        calls += 1
        if calls == 1:
            require(q.shape == captured.shape and q.dtype == captured.dtype and q.device == captured.device,
                    "layer-zero capture tensor differs from native layout")
            return captured
        return original(torch, q, k, v, mask, scale, clock)

    wider.wider_flash_attention = attention
    try:
        scores = wider.wider_projection_scores(half, independent, row, check)
    finally:
        wider.wider_flash_attention = original
    require(calls == 28, "expected one substituted attention and 27 ordinary attention calls")
    return scores


def worker(replay, wider, deadline):
    def check():
        require(time.monotonic() < deadline, "suffix replay deadline expired")

    legacy = replay.read_only_bootstrap()
    proof, proof_pin = legacy.read_pinned_json(CAMPAIGN / "compact-fp16-qkv-fp32-wider-reference-r1" /
                                              wider.WIDER_LEAF, REFERENCE_SHA, check)
    capture, capture_pin = replay.read_t1024_capture(CAMPAIGN / "compact-fp16-qkv-fp32-layer0-t1024-r1" /
                                                   "native-result.json", CAPTURE_SHA)
    native, native_pin = legacy.read_pinned_json(CAMPAIGN / "compact-fp16-qkv-fp32-native-development-r1" /
                                               "native-result.json", replay.T1024_SEALED_SHA, check)
    taps = replay.validated_t1024_taps(capture, native)
    reference = proof["references"][1]
    require(reference["input"]["input_digest"] == replay.T1024_DIGEST, "input pin differs")
    runtime, _ = legacy.read_pinned_json(replay.RUNTIME, replay.RUNTIME_SHA, check)
    for file_pin in proof["publisher_files"].values():
        require(replay.file_pin(Path(file_pin["path"]), check) == file_pin, "publisher pin differs")
    half = module("../arbiter-fp16-reference.py", legacy.REFERENCES[legacy.REFERENCE_OWNER])
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import torch
    torch.set_num_threads(wider.WIDER_CPU_THREADS)
    torch.set_num_interop_threads(1)
    model, _ = half.from_authenticated_original(torch, "compact", ROOT / legacy.PUBLISHER_OWNER,
                                              legacy.PUBLISHER_ROOT, "cpu")
    with torch.inference_mode():
        cut = replay.head_tensor(torch, taps[3], replay.T1024_TOKENS)
        del capture, taps
        scores = score_suffix(wider, half, model, reference["input"], cut, check)
    require(not torch.cuda.is_initialized() and legacy.finite_scores(scores), "invalid CPU-only score vector")
    pins = dict(runtime["files"])
    for pin in proof["reference_sources"] + [replay.file_pin(Path(__file__).resolve())]:
        pins[pin["path"]] = pin
    imports = [sys.modules[name] for name in sys.modules.keys() - before]
    legacy.loaded_runtime(pins, replay.file_pin, check, imports)
    baseline = reference["matching_qkv_fp32"]
    target = native["rows"][1]["raw_scores"]
    def error(a, b):
        return max(abs(x-y) for x, y in zip(a, b, strict=True))
    return {"schema": "airk.private.compact-t1024-layer0-suffix/1", "capture": capture_pin,
            "native": native_pin, "reference": proof_pin, "baseline_vs_native": error(baseline, target),
            "suffix_vs_native": error(scores, target), "suffix_vs_baseline": error(scores, baseline),
            "scores": scores, "independent_reference": False, "qualification": False}


def run_owned(replay, wider, out):
    legacy, _ = wider.load_base()
    deadline = time.monotonic() + legacy.MAX_WALL_SECONDS
    def check():
        require(time.monotonic() < deadline, "original wall expired")
    plan, _ = legacy.read_pinned_json(wider.BASE_ARGUMENTS["conversion_plan"],
                                      wider.BASE_ARGUMENTS["conversion_plan_sha256"], check)
    wrapper = Path(plan["wrapper"]["path"])
    require(wrapper == ROOT / "scripts/arbiter-fp16-execution.py" and replay.file_pin(wrapper) == plan["wrapper"],
            "conversion owner differs")
    conversion = legacy.load_module(wrapper, "suffix_conversion_owner")
    token = conversion.windows_owner()
    require(replay.file_pin(Path(sys.executable)) == plan["python"] and token["owner_sid"] == plan["human_sid"],
            "launcher identity differs")
    helper_root = ROOT / "scripts/cuda-native-execution/helpers"
    for name, digest in conversion.HELPERS.items():
        require(replay.file_pin(helper_root / name)["sha256"] == digest, "owned job helper changed")
    sys.path.insert(0, str(helper_root))
    parent = legacy.load_module(helper_root / "reference_parent_no_site_r2.py", "suffix_parent")
    owner = legacy.load_module(helper_root / "reference_owned.py", "suffix_job")
    require(out.parent == CAMPAIGN and out == out.resolve() and not out.exists(), "fresh campaign output required")
    cache = out / "temp"
    for path in (out, cache):
        conversion.guard(plan, path, deadline=deadline)
        path.mkdir()
        conversion.guard(plan, path, True, deadline=deadline)
    log, receipt = out / "worker.log", out / "execution.json"
    for path in (log, receipt):
        conversion.guard(plan, path, deadline=deadline)
    source = replay.file_pin(Path(__file__).resolve())
    env = {key: val for key, val in os.environ.items()
           if not key.upper().startswith(("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_", "TORCH_", "HF_", "TRITON_"))}
    env.update(CUDA_VISIBLE_DEVICES="", HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1", TOKENIZERS_PARALLELISM="false",
               TORCH_HOME=str(cache), TORCHINDUCTOR_CACHE_DIR=str(cache), TRITON_CACHE_DIR=str(cache),
               TEMP=str(cache), TMP=str(cache), TMPDIR=str(cache))
    argv = [sys.executable, "-I", "-B", "-S", "-X", "pycache_prefix="+plan["cache_prefix"], source["path"],
            "--worker-deadline", repr(deadline), "--source-sha256", source["sha256"]]
    def suspended(api, job, info, child_deadline):
        observed = parent.suspended_authentication(api, info, deadline, child_deadline, conversion.SID,
                                                  plan["python"], owner.image_proof)
        require(conversion.windows_owner(process=info.process) == token
                and owner.job_snapshot(api, job) == ([info.pid], 1, 1), "unexpected suspended token or process")
        return observed
    with open(os.devnull, "rb") as stdin, log.open("xb") as output:
        conversion.guard(plan, log, True, deadline=deadline)
        owned = owner.run_windows_owned_training(argv, ROOT, output, deadline-time.monotonic(), env, stdin, 32 << 30,
                                               before_resume=suspended, absolute_deadline=deadline, cleanup_seconds=5)
    conversion.guard(plan, log, True, deadline=time.monotonic()+10)
    require(log.stat().st_size <= 16 << 20, "unexpected log extent")
    markers = [line[len(MARKER):] for line in log.read_text(encoding="utf8").splitlines() if line.startswith(MARKER)]
    complete = parent.settled(owned) and len(markers) == 1 and replay.file_pin(Path(__file__).resolve()) == source
    data = {"complete": complete, "owned": owned, "source": source, "command": argv,
            "result": json.loads(markers[0]) if complete else None}
    with receipt.open("x", encoding="utf8", newline="\n") as stream:
        json.dump(data, stream, allow_nan=False)
        stream.write("\n")
    conversion.guard(plan, receipt, True, deadline=time.monotonic()+10)
    require(complete, "suffix worker failed, inspect retained log and receipt")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--output", type=Path)
    group.add_argument("--worker-deadline", type=float)
    parser.add_argument("--source-sha256")
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "checked Windows Python -I -B -S required")
    replay = module("replay_layer0.py", REPLAY_SHA)
    wider = module("verify_projection_candidate_t32.py", replay.WIDER_REFERENCE_SHA)
    if args.output:
        run_owned(replay, wider, args.output)
    else:
        require(replay.file_pin(Path(__file__).resolve())["sha256"] == args.source_sha256
                and 0 < args.worker_deadline-time.monotonic() <= 600, "owned source or deadline differs")
        print(MARKER+json.dumps(worker(replay, wider, args.worker_deadline), allow_nan=False))
