# SPDX-License-Identifier: Apache-2.0
"""Measure the source-derived sequential SM86 schedule on frozen Compact T1024 operands."""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent


def load(path, name, expected):
    if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise ValueError(f"source pin changed: {path}")
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    replay = load(HERE / "replay_layer0.py", "compact_layer0",
                  "1c344a187ea32694789571dcf22721945bd9302eeb4f76cd26ab2e3a9a383da0")
    verifier = replay.read_only_bootstrap()
    flash_path = HERE.parent / "arbiter-fp16-reference" / "large_flash.py"
    schedule = load(flash_path, "sm86_flash", "24d551de98080eb338f602a725161ed35a95456646860575e1bf16a28d3284cb")
    capture, capture_pin = replay.read_t1024_capture(
        replay.CAMPAIGN / "compact-fp16-qkv-fp32-layer0-t1024-r1/native-result.json",
        "6725b3ac6d793601a100ff7d6e812837d58b4aa33ec2b11af00d1936b2d15e03")
    sealed, sealed_pin = verifier.read_pinned_json(
        replay.CAMPAIGN / "compact-fp16-qkv-fp32-native-development-r1/native-result.json",
        replay.T1024_SEALED_SHA, replay.no_deadline)
    taps = replay.validated_t1024_taps(capture, sealed)
    runtime, runtime_pin = verifier.read_pinned_json(replay.RUNTIME, replay.RUNTIME_SHA, replay.no_deadline)
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import torch

    torch.set_num_threads(4)
    torch.set_num_interop_threads(1)
    torch.set_float32_matmul_precision("highest")
    with torch.inference_mode():
        query, key, value, native = [replay.head_tensor(torch, tap, 1024) for tap in taps]
        key = key.repeat_interleave(2, dim=1).float()
        value = value.repeat_interleave(2, dim=1).float()
        output = torch.empty_like(query)
        for start in range(0, 1024, 64):
            target = slice(start, start + 64)
            output[..., target, :] = schedule.flash_block(
                torch, query[..., target, :].float(), key, value, start, replay.no_deadline)
        comparison = replay.compare(torch, output, native, wider_statistics=True)
    result_source = replay.file_pin(Path(__file__).resolve())
    sources = {**runtime["files"], result_source["path"]: result_source}
    modules = (sys.modules[name] for name in sys.modules.keys() - before)
    imports = verifier.loaded_runtime(sources, replay.file_pin, replay.no_deadline, modules)
    for pin in (capture_pin, sealed_pin, runtime_pin):
        replay.require(replay.file_pin(Path(pin["path"])) == pin, "input changed during replay")
    replay.require(not torch.cuda.is_initialized(), "unexpected CUDA initialization")
    print(json.dumps({"schema": "airk.private.compact-sm86-t1024-replay/1", "source": result_source,
                      "capture": capture_pin, "sealed": sealed_pin, "runtime": runtime_pin,
                      "schedule": replay.file_pin(flash_path), "imports": len(imports),
                      "raw_scores_equal_to_sealed": True, "tap_elements": native.numel(),
                      "sequential_sm86": comparison, "native_forwards": 0, "qualification": False},
                     allow_nan=False))


if __name__ == "__main__":
    main()
