# SPDX-License-Identifier: Apache-2.0
"""Compare independent CPU attention paths on captured Compact layer-zero tensors."""

import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import struct
import sys


ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
RUNTIME = CAMPAIGN / "compact-fp16-present-conversion-plan-r2/compact-runtime.json"
RUNTIME_SHA = "a557be764ebcf8a2b777403ac349f2d3ecadfabb5d65e16b8c47026239644c95"
PYTHON_SHA = "d932e5e2f324d57f392e8fd063dcf6d0185be8a664c57c6d24e7762ed02c28ca"
VERIFIER_SHA = "0f410a57014ce09a4703bd52d50e96fcc2f853605e1352b48d641f52daa3cdd6"
SEALED_SHA = "4ab73f93188955889b3c026ceadf9283ec133ac3c1a7a70f1a8ea1cd50af41a5"
CAPTURE_SCHEMA = "airk.private.compact-fp16-layer0-taps/1"
SEALED_SCHEMA = "airk.private.compact-fp16-bounded-t32-component/1"
T32_DIGEST = "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335"
TOKENS = 32
HEAD_WIDTH = 128
TAPS = (
    ("/airk_g5/attention/0/q16", 2048),
    ("/airk_g5/attention/0/k16", 1024),
    ("/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16", 1024),
    ("/airk_g5/attention/0/raw16", 2048),
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def scores(result, schema):
    require(isinstance(result, dict) and result.get("schema") == schema
            and result.get("native_forward_returned") is result.get("forward_synchronized") is True
            and type(result.get("numeric_forward_attempts")) is int
            and result["numeric_forward_attempts"] == 1,
            "expected a single synchronized native T32 forward")
    rows = result.get("rows")
    require(isinstance(rows, list) and len(rows) == 1 and isinstance(rows[0], dict),
            "expected one native score row")
    row = rows[0]
    require(row.get("name") == "diagnostic-all-live-32"
            and type(row.get("tokens")) is int and row["tokens"] == TOKENS
            and row.get("input_digest") == T32_DIGEST,
            "frozen native T32 row differs")
    values = row.get("raw_scores")
    require(isinstance(values, list) and len(values) == 8
            and all(type(value) in (int, float) and math.isfinite(value) for value in values),
            "expected eight finite native scores")
    return struct.pack("<8d", *values)


def validated_taps(capture, sealed):
    capture_scores = scores(capture, CAPTURE_SCHEMA)
    require(capture.get("complete") is True and capture.get("error") is None,
            "complete layer-zero capture required")
    require(capture_scores == scores(sealed, SEALED_SCHEMA),
            "captured raw scores differ from sealed native T32 scores")
    taps = capture.get("layer0_taps")
    require(isinstance(taps, list) and len(taps) == len(TAPS), "expected exactly four layer-zero taps")
    for tap, (name, width) in zip(taps, TAPS):
        require(isinstance(tap, dict) and set(tap) == {"name", "width", "bits"}
                and tap["name"] == name and type(tap["width"]) is int and tap["width"] == width,
                "layer-zero tap name, order or width differs")
        words = tap["bits"]
        require(isinstance(words, list) and len(words) == TOKENS * width
                and all(type(word) is int and 0 <= word <= 0xFFFF and word & 0x7C00 != 0x7C00
                        for word in words),
                "complete finite binary16 layer-zero bits required")
    return taps


def head_tensor(torch, tap):
    # The native [batch, token, width] rows flatten consecutive heads, then lanes.
    bits = torch.tensor(tap["bits"], dtype=torch.uint16, device="cpu")
    heads = tap["width"] // HEAD_WIDTH
    return bits.view(torch.float16).reshape(1, TOKENS, heads, HEAD_WIDTH).transpose(1, 2).contiguous()


def compare(torch, replayed, native):
    require(replayed.dtype == torch.float16 and replayed.device.type == "cpu"
            and replayed.shape == native.shape and torch.isfinite(replayed).all().item(),
            "finite replayed half tensor with native geometry required")
    different = replayed.view(torch.int16) != native.view(torch.int16)
    return {
        "max_abs_error": (replayed.float() - native.float()).abs().amax().item(),
        "exact_bit_mismatches": different.count_nonzero().item(),
    }


def replay(torch, reference, taps):
    with torch.inference_mode():
        q, k, v, native = [head_tensor(torch, tap) for tap in taps]
        attention = reference.causal_half_attention
        mask = [1] * TOKENS
        dense = attention(torch, q, k, v, mask)
        flash = attention(torch, q, k, v, mask, context_flash_single_tile=True)
        return {
            "tap_elements_compared": native.numel(),
            "dense_fp32_softmax": compare(torch, dense, native),
            "single_tile_flash": compare(torch, flash, native),
        }


def no_deadline():
    pass


def file_pin(path, check=no_deadline):
    check()
    with path.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest}


def read_only_bootstrap():
    prefix = Path(sys.pycache_prefix or "")
    cache = Path(os.environ.get("TORCHINDUCTOR_CACHE_DIR", ""))
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and prefix.is_absolute() and not prefix.exists() and os.environ.get("CUDA_VISIBLE_DEVICES") == "",
            "isolated Python, absent bytecode prefix and disabled CUDA required")
    require(cache.is_absolute() and cache.is_relative_to(CAMPAIGN) and cache.is_dir()
            and cache.resolve(strict=True) == cache
            and not any(path.is_symlink() or path.is_junction() for path in (cache, *cache.parents)),
            "existing audited Torch cache directory required")
    verifier_path = Path(__file__).with_name("verify_compact.py")
    require(file_pin(verifier_path)["sha256"] == VERIFIER_SHA
            and file_pin(Path(sys.executable))["sha256"] == PYTHON_SHA,
            "pinned Compact verifier or Python interpreter differs")
    spec = importlib.util.spec_from_file_location("compact_replay_verifier", verifier_path)
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    sys.addaudithook(lambda event, arguments: verifier.read_only(event, arguments, cache))
    return verifier


def run(args):
    verifier = read_only_bootstrap()
    require(args.sealed_result_sha256 == SEALED_SHA, "exact prior sealed CUDA result pin required")
    capture, capture_pin = verifier.read_pinned_json(args.native_result, args.native_result_sha256, no_deadline)
    sealed, sealed_pin = verifier.read_pinned_json(args.sealed_result, SEALED_SHA, no_deadline)
    taps = validated_taps(capture, sealed)
    runtime, runtime_pin = verifier.read_pinned_json(RUNTIME, RUNTIME_SHA, no_deadline)
    source = ROOT / verifier.REFERENCE_OWNER
    reference_pin = file_pin(source)
    require(reference_pin["sha256"] == verifier.REFERENCES[verifier.REFERENCE_OWNER],
            "pinned independent reference source differs")
    reference = verifier.load_module(source, "compact_layer0_half_reference")
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import torch

    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    reference.freeze_reference_math(torch)
    comparisons = replay(torch, reference, taps)
    modules = (sys.modules[name] for name in sys.modules.keys() - before)
    # Torch may register the exact entry point again under its multiprocessing alias.
    replay_source = file_pin(Path(__file__).resolve())
    sources = {**runtime["files"], replay_source["path"]: replay_source}
    imported = verifier.loaded_runtime(sources, file_pin, no_deadline, modules)
    require(not torch.cuda.is_initialized(), "CPU replay initialized CUDA")
    return {
        "schema": "airk.private.compact-fp16-layer0-replay/1",
        "input_digest": T32_DIGEST,
        "raw_scores_equal_to_sealed": True,
        "capture": capture_pin,
        "sealed_result": sealed_pin,
        "runtime_manifest": runtime_pin,
        "reference": reference_pin,
        "replay_source": replay_source,
        "imported_runtime_modules": len(imported),
        **comparisons,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ("native-result", "native-result-sha256", "sealed-result", "sealed-result-sha256"):
        parser.add_argument("--" + option, required=True)
    args = parser.parse_args()
    try:
        result = run(args)
    except (OSError, ValueError, KeyError, OverflowError) as error:
        parser.exit(1, f"Layer-zero replay refused: {error}\n")
    print(json.dumps(result, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
