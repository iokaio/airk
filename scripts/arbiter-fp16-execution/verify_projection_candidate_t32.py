# SPDX-License-Identifier: Apache-2.0
"""Produce owned independent CPU references for staged Compact projection variants."""

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
STAGE = CAMPAIGN / "compact-fp16-qkv-fp32-staged-r1/stage.json"
STAGE_SHA = "87ec206416eda607ff1585809d085e37fc4f8b99a5fc029308b5c78218cefcc2"
OUTPUT_STAGE = CAMPAIGN / "compact-fp16-qkvo-fp32-staged-r1/stage.json"
OUTPUT_STAGE_SHA = "dbf3a746b8ec66d81ba68798988cdc4068d99841c538cbb2963a37125bd3fb5b"
OUTPUT_GRAPH_VERIFIER_SHA = "5d9c604db15a9ce452836e981560b2addfda504fc64c4f30f68bc443d19f3f1e"
BASE_ARGUMENTS = {
    "conversion_plan": CAMPAIGN / "compact-fp16-present-conversion-plan-r2/compact.json",
    "conversion_plan_sha256": "b43ddbceb00a82b9ae5cc700bb3e6b633d1ed966cc028592c6e0ab5fe836e715",
    "conversion_receipt": CAMPAIGN / "compact-fp16-present-conversion-owned-r2/execution.json",
    "conversion_receipt_sha256": "547777e60cd994968849c08b5dc158432097ece6b09da37e5008fa7c34b1cc56",
}
BASE_VERIFIER_SHA = "50d5c7237cd057a1d08ea79794bbdda0f1e141438a2d22580a760e418f9e3d75"
GRAPH_VERIFIER_SHA = "160facf30e6bb1b0d5881d26a7c705c3c66ad160f74f0014d7ae5647e6927cf9"
SCHEMA = "airk.private.compact-fp32-qkv-t32-reference/1"
OWNED_SCHEMA = "airk.private.owned-compact-fp32-qkv-t32-reference/1"
MARKER = b"AIRK_COMPACT_FP32_QKV_T32_REFERENCE "
SOURCE_ORDER_SCHEMA = "airk.private.compact-fp32-qkv-source-order-t32-reference/1"
SOURCE_ORDER_OWNED_SCHEMA = "airk.private.owned-compact-fp32-qkv-source-order-t32-reference/1"
SOURCE_ORDER_MARKER = b"AIRK_COMPACT_FP32_QKV_SOURCE_ORDER_T32_REFERENCE "
SOURCE_ORDER_LEAF = "compact-qkv-fp32-source-order-t32-reference.json"
OUTPUT_SCHEMA = "airk.private.compact-fp32-qkvo-t32-reference/1"
OUTPUT_OWNED_SCHEMA = "airk.private.owned-compact-fp32-qkvo-t32-reference/1"
OUTPUT_MARKER = b"AIRK_COMPACT_FP32_QKVO_T32_REFERENCE "
OUTPUT_LEAF = "compact-qkvo-fp32-t32-reference.json"
WIDER_SCHEMA = "airk.private.compact-fp32-qkv-wider-reference/1"
WIDER_OWNED_SCHEMA = "airk.private.owned-compact-fp32-qkv-wider-reference/1"
WIDER_MARKER = b"AIRK_COMPACT_FP32_QKV_WIDER_REFERENCE "
WIDER_LEAF = "compact-qkv-fp32-wider-reference.json"
WIDER_CPU_THREADS = 8
FLASH_BLOCK = 64
WIDER_TOKENS = (128, 1024, 8192)
RMS_SHAPES = {(1, 32, 2048): 57, (1, 32, 16, 128): 28, (1, 32, 8, 128): 28}
MAX_LOG = 64 << 20
MAX_PROOF = 16 << 20
ROUTES = tuple(f"layers.{layer}.self_attn.{projection}_proj.weight"
               for layer in range(28) for projection in ("q", "k", "v"))
OUTPUT_ROUTES = tuple(f"layers.{layer}.self_attn.{projection}_proj.weight"
                      for layer in range(28) for projection in ("q", "k", "v", "o"))


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load_base():
    path = HERE / "verify_compact.py"
    raw = path.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    require(digest == BASE_VERIFIER_SHA, "unchanged maintained Compact verifier required")
    spec = importlib.util.spec_from_file_location("compact_projection_base_verifier", path)
    legacy = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = legacy
    spec.loader.exec_module(legacy)
    return legacy, {"path": str(path), "bytes": len(raw), "sha256": digest}


def reference_identity(source_order, output_projection, wider=False):
    require(sum((source_order, output_projection, wider)) <= 1, "select only one Compact reference recipe")
    if wider:
        return WIDER_SCHEMA, WIDER_OWNED_SCHEMA, WIDER_MARKER, WIDER_LEAF, "matching_qkv_fp32"
    if output_projection:
        return OUTPUT_SCHEMA, OUTPUT_OWNED_SCHEMA, OUTPUT_MARKER, OUTPUT_LEAF, "matching_qkvo_fp32"
    if source_order:
        return SOURCE_ORDER_SCHEMA, SOURCE_ORDER_OWNED_SCHEMA, SOURCE_ORDER_MARKER, SOURCE_ORDER_LEAF, "matching_qkv_fp32"
    return SCHEMA, OWNED_SCHEMA, MARKER, "compact-qkv-fp32-t32-reference.json", "matching_qkv_fp32"


def reference_inputs(legacy, check, *, wider):
    if wider:
        return legacy.wider_references(check)
    row, publisher_files = legacy.exact_reference(check)
    return [row], publisher_files


def matching_references(legacy, result, expected, field, *, wider):
    key = "references" if wider else "reference"
    require(("reference" if wider else "references") not in result,
            "a single exact Compact reference payload required")
    rows = result[key] if wider else [result[key]]
    require(type(rows) is list and len(rows) == len(expected) == (3 if wider else 1),
            "exact ordered Compact reference row count required")
    for actual, source in zip(rows, expected):
        require(type(actual) is dict and actual.keys() == source.keys() | {field}
                and all(actual[key] == value for key, value in source.items())
                and legacy.finite_scores(actual[field]),
                "exact Compact reference row lineage and eight finite matching scores required")


def read_stage(legacy, conversion, plan_pin, token, check, *, output_projection=False):
    stage_path, stage_sha = (OUTPUT_STAGE, OUTPUT_STAGE_SHA) if output_projection else (STAGE, STAGE_SHA)
    stage, stage_pin = legacy.read_pinned_json(stage_path, stage_sha, check)
    conversion.windows_owner(stage_path)
    variant = "qkvo" if output_projection else "qkv"
    projections = 112 if output_projection else 84
    require(stage["schema"] == f"airk.private.compact-fp32-{variant}-projection-candidate/1"
            and stage["complete"] is True and stage["qualification"] is stage["product_admission"] is False
            and stage["native_load_attempts"] == stage["native_forward_attempts"] == 0
            and stage["conversion_plan"] == plan_pin and stage["execution_token"] == token
            and stage["graph_proof"] == {"projection_count": projections, "added_cast_count": 3 * projections,
                                         "baseline_node_count": 4538, "candidate_node_count": 4538 + 3 * projections},
            "reviewed staged projection receipt differs")
    candidate = {key: stage["candidate_" + key] for key in ("graph", "weights")}
    require(candidate["graph"]["path"] == str(stage_path.with_name("model.onnx"))
            and candidate["weights"]["path"] == str(stage_path.with_name("weights.bin"))
            and candidate["graph"]["sha256"] != stage["baseline_graph"]["sha256"]
            and all(candidate["weights"][key] == stage["baseline_weights"][key] for key in ("sha256", "bytes")),
            "staged graph or immutable FP16 storage differs")
    verifier_path = HERE / "verify_projection_candidate_graph.py"
    staged_verifier = next((pin for pin in stage["projection_sources"] if pin["path"] == str(verifier_path)), None)
    expected_verifier = OUTPUT_GRAPH_VERIFIER_SHA if output_projection else GRAPH_VERIFIER_SHA
    require(staged_verifier is not None and staged_verifier["sha256"] == expected_verifier,
            "exact staged independent projection graph verifier required")
    conversion.windows_owner(verifier_path)
    source = conversion.pin(verifier_path, check)
    allowed_sources = {expected_verifier} if output_projection else {GRAPH_VERIFIER_SHA, OUTPUT_GRAPH_VERIFIER_SHA}
    require(source["sha256"] in allowed_sources, "exact maintained projection graph verifier required")
    return stage, stage_pin, candidate, source


def projection_scores(independent, row, check, *, routes=ROUTES, context_flash_single_tile=True):
    torch = independent.torch
    require((independent.layers, independent.hidden, independent.qheads, independent.kvheads, independent.dim)
            == (28, 2048, 16, 8, 128)
            and torch.device(independent.device).type == "cpu"
            and all(name in independent.weights for name in routes), "exact Compact FP16 publisher geometry required")
    torch.set_float32_matmul_precision("highest")
    expected = set(routes)
    observed = []
    original_linear = independent._linear_half

    def linear_half(value, name):
        if name not in expected:
            return original_linear(value, name)
        check()
        weight = independent.weights[name]
        require(value.dtype == weight.dtype == torch.float16 and value.device.type == weight.device.type == "cpu",
                "projection variant requires independent CPU half operands")
        output = torch.nn.functional.linear(value.float(), weight.float()).half()
        require(torch.isfinite(output).all().item(), "nonfinite independently projected half output")
        observed.append(name)
        return output

    independent._linear_half = linear_half
    try:
        scores = independent.forward(row, context_flash_single_tile=context_flash_single_tile)
    finally:
        del independent._linear_half
    require(tuple(observed) == routes, f"exact {len(routes)} independent projection calls in model order required")
    require(scores.dtype == torch.float32 and scores.device.type == "cpu" and tuple(scores.shape) == (1, 8)
            and not torch.cuda.is_initialized(), "CPU-only FP32-ABI Compact reference vector required")
    return scores[0].tolist()


def reverse_flash_result(torch, scores, values, scale_log2):
    # SM86 causal D128 Flash scans key tiles from right to left. Their
    # exponent tiles narrow before PV, while the denominator stays FP32.
    tiles = scores.unflatten(-1, (-1, FLASH_BLOCK))
    maxima = tiles.amax(dim=-1).flip(-1).cummax(dim=-1).values.flip(-1)
    scaled_max = maxima * scale_log2
    exponent = tiles.double().mul_(scale_log2).sub_(scaled_max.unsqueeze(-1).double()).float().exp2_()

    # Collapse the successive rescalings to one factor per tile. CPU
    # reductions and exponent evaluation approximate the CUDA operations.
    scale = ((maxima - maxima[..., :1]) * scale_log2).exp2_()
    denominator = (exponent.sum(dim=-1) * scale).sum(dim=-1, keepdim=True)
    mass = exponent.half().float().mul_(scale.unsqueeze(-1)).flatten(-2)
    return (torch.matmul(mass, values) * denominator.reciprocal()).half()


def wider_flash_attention(torch, query, key, value, mask, scale_log2, check):
    width = len(mask)
    require(width in WIDER_TOKENS and mask == [1] * width and tuple(query.shape) == (1, 16, width, 128)
            and tuple(key.shape) == tuple(value.shape) == (1, 8, width, 128)
            and all(tensor.dtype == torch.float16 and tensor.device.type == "cpu"
                    and torch.isfinite(tensor).all().item() for tensor in (query, key, value)),
            "exact all-live wider Compact CPU GQA geometry required")
    keys = key.repeat_interleave(2, dim=1).float()
    values = value.repeat_interleave(2, dim=1).float()
    output = torch.empty_like(query)
    positions = torch.arange(width, device=query.device)
    for first in range(0, width, FLASH_BLOCK):
        check()
        last = first + FLASH_BLOCK
        scores = torch.matmul(query[:, :, first:last, :].float(), keys[:, :, :last, :].transpose(-1, -2))
        allowed = positions[:last].unsqueeze(0) <= positions[first:last].unsqueeze(1)
        scores.masked_fill_(~allowed, -math.inf)
        output[:, :, first:last, :] = reverse_flash_result(torch, scores, values[:, :, :last, :], scale_log2)
    require(torch.isfinite(output).all().item(), "nonfinite wider Compact attention output")
    return output


def wider_projection_scores(half, independent, row, check):
    attention_calls = 0
    previous_attention = half.causal_half_attention
    scale_log2 = half.f32(half.f32(128 ** -0.5) * math.log2(math.e))

    def attention(torch, query, key, value, mask, *, context_flash_single_tile):
        nonlocal attention_calls
        require(context_flash_single_tile is False, "wider projections require the multi-tile attention recipe")
        output = wider_flash_attention(torch, query, key, value, mask, scale_log2, check)
        attention_calls += 1
        return output

    half.causal_half_attention = attention
    try:
        scores = projection_scores(independent, row, check, context_flash_single_tile=False)
    finally:
        half.causal_half_attention = previous_attention
    require(attention_calls == 28, "exact 28 wider Compact multi-tile attention calls required")
    return scores


def fp32_square_fma(torch, accumulator, value):
    # A binary32 square is exact in binary64. TwoSum retains the lost low
    # part of adding the binary32 accumulator before the final rounding.
    term = value.double().square()
    acc = accumulator.double()
    wide = term + acc
    carried = wide - term
    remainder = (term - (wide - carried)) + (acc - carried)
    rounded = wide.float()
    rounded_wide = rounded.double()
    away = ((rounded_wide < wide) & (remainder > 0)) | ((rounded_wide > wide) & (remainder < 0))
    step = torch.where(remainder > 0, torch.inf, -torch.inf).to(rounded.dtype)
    neighbor = torch.nextafter(rounded, step)
    midpoint = (rounded_wide + neighbor.double()) * 0.5
    return torch.where(away & (wide == midpoint), neighbor, rounded)


def source_order_square_mean(torch, value):
    width = value.shape[-1]
    require(value.dtype == torch.float32 and value.device.type == "cpu" and width in (128, 2048),
            "source-order RMS requires CPU binary32 rows of width 128 or 2048")
    lanes = value.reshape(*value.shape[:-1], width // 4, 4)
    sums = torch.zeros_like(lanes[..., 0])
    for word in lanes.unbind(-1):
        sums = fp32_square_fma(torch, sums, word)

    # Each warp merges lanes 0/16, 0/8, ..., with identical per-group
    # pairing for every other participating lane. Then warp zero merges
    # the warp partials in the same shuffle-down order.
    sums = sums.reshape(*value.shape[:-1], width // 128, 32)
    for stride in (16, 8, 4, 2, 1):
        sums = sums[..., :stride] + sums[..., stride:2 * stride]
    sums = sums.squeeze(-1)
    for stride in (8, 4, 2, 1) if width == 2048 else ():
        sums = sums[..., :stride] + sums[..., stride:2 * stride]
    require(torch.isfinite(sums).all().item(), "source-order RMS square accumulation overflow")
    return sums / width


def source_order_sigmoid(torch, gate):
    positive = gate >= 0
    unfolded = 1.0 / (1.0 + torch.exp(torch.where(positive, -gate, gate)))
    return torch.where(positive, unfolded, 1.0 - unfolded)


def source_order_recipe(enabled, output_projection=False, wider=False):
    if wider:
        return {
            "qkv_fp32_routes": list(ROUTES),
            "context_flash_single_tile": False,
            "torch_cpu_threads": WIDER_CPU_THREADS,
            "wider_attention": {"algorithm": "causal-reverse-tile-fp16-exponent",
                                "query_block": FLASH_BLOCK, "key_block": FLASH_BLOCK, "split_kv": False},
        }
    recipe = ({"qkvo_fp32_routes": list(OUTPUT_ROUTES), "context_flash_single_tile": True} if output_projection
              else {"qkv_fp32_routes": list(ROUTES), "context_flash_single_tile": True})
    if enabled:
        recipe["source_order"] = {
            "rms_square_accumulation": "fp32-round-to-nearest-even-fused-square-add",
            "rms_lane_width": 4, "rms_warp_size": 32,
            "rms_output_order": "scale-times-cpu-rsqrt-then-input", "quickgelu": "sign-branch-cpu-exp",
        }
    return recipe


def observed_source_order(norm_calls, gate_calls):
    return {"rms": [{"shape": list(shape), "calls": count} for shape, count in norm_calls.items()],
            "quickgelu_calls": gate_calls}


def source_order_scores(half, independent, row, check):
    torch = independent.torch
    norm_calls = dict.fromkeys(RMS_SHAPES, 0)
    gate_calls = 0

    class ScopedTorch:
        def __getattr__(self, name):
            return getattr(torch, name)

        def sigmoid(self, gate):
            nonlocal gate_calls
            require(tuple(gate.shape) == (1, 32, 6144) and gate.dtype == torch.float32
                    and gate.device.type == "cpu", "exact Compact QuickGelu FP32 input required")
            gate_calls += 1
            return source_order_sigmoid(torch, gate)

    scoped_torch = ScopedTorch()

    def rms(api, value, scale, eps):
        shape = tuple(value.shape)
        require(api is scoped_torch and shape in norm_calls and tuple(scale.shape) == (shape[-1],)
                and scale.dtype == torch.float32 and scale.device.type == "cpu" and eps == 1e-6,
                "exact Compact source-order RMS input and scale required")
        norm_calls[shape] += 1
        inverse = torch.rsqrt(source_order_square_mean(torch, value) + eps)
        return (scale * inverse) * value

    prior_rms = half.rms
    independent.torch = scoped_torch
    half.rms = rms
    try:
        scores = projection_scores(independent, row, check)
    finally:
        half.rms = prior_rms
        independent.torch = torch
    require(norm_calls == RMS_SHAPES and gate_calls == 28, "exact 113 RMSNorm and 28 QuickGelu calls required")
    return scores, observed_source_order(norm_calls, gate_calls)


def worker(deadline, expected_source, *, source_order=False, output_projection=False, wider=False):
    started = time.monotonic()
    schema, _, _, _, matching_field = reference_identity(source_order, output_projection, wider)

    def check():
        if time.monotonic() >= deadline:
            raise TimeoutError("original Compact projection reference deadline expired")

    legacy, legacy_pin = load_base()
    require(0 < deadline - started <= legacy.MAX_WALL_SECONDS and os.environ.get("CUDA_VISIBLE_DEVICES") == "",
            "original bounded CPU-only projection reference required")
    cache = Path(os.environ.get("TORCHINDUCTOR_CACHE_DIR", ""))
    require(cache.is_absolute() and cache.is_relative_to(CAMPAIGN) and cache.resolve(strict=True) == cache
            and cache.is_dir() and not cache.is_symlink() and not cache.is_junction(),
            "existing audited Torch cache directory required")
    sys.addaudithook(lambda event, values: legacy.read_only(event, values, cache))
    conversion, plan, plan_pin, prior_pin, proofs, token = legacy.authenticated_conversion(
        argparse.Namespace(**BASE_ARGUMENTS), check)
    conversion.windows_owner(cache)
    source = conversion.pin(Path(__file__).resolve(), check)
    require(source["sha256"] == expected_source and conversion.pin(sys.executable, check) == plan["python"]
            and sys.pycache_prefix == plan["cache_prefix"] and not Path(plan["cache_prefix"]).exists(),
            "actual reviewed worker, interpreter or absent bytecode prefix differs")
    stage, stage_pin, candidate, graph_source = read_stage(
        legacy, conversion, plan_pin, token, check, output_projection=output_projection)
    baseline = {key: proofs[key] for key in ("graph", "weights")}
    require(baseline == {key: stage["baseline_" + key] for key in baseline},
            "stage does not bind the original authenticated conversion")
    for pin in (*candidate.values(), graph_source):
        conversion.windows_owner(Path(pin["path"]))
        conversion.pinned(pin, check)

    original, selected = conversion.source_facts(plan["source_plan"], "compact", check)
    require(plan["source_graph"] == selected["original_graph"] and plan["source_graph"]["sha256"] == legacy.ORIGINAL_GRAPH
            and plan["source_closure"] == selected["source_closure"] and plan["python"] == original["python"]
            and plan["expected_weights_bytes"] == candidate["weights"]["bytes"] == selected["expected_weights_bin_bytes"]
            and plan["expected_attention_count"] == len(selected["expected_attention_nodes"]) == 28,
            "original Compact source identity differs")
    original_graph = Path(plan["source_graph"]["path"])
    original_files, closure_pin = legacy.read_pinned_json(plan["source_closure"]["path"], plan["source_closure"]["sha256"], check)
    require(closure_pin == plan["source_closure"], "original Compact closure length differs")
    references, publisher_files = reference_inputs(legacy, check, wider=wider)
    publisher_root = legacy.PUBLISHER_ROOT / "candidate-assets/Qwen--Qwen3-1.7B"
    for name, pin in publisher_files.items():
        require(Path(name).name == name and pin["path"] == str(publisher_root / name), "publisher source escaped its path")
        conversion.pinned(pin, check)

    reference_sources = [source, legacy_pin, graph_source]
    for relative, digest in legacy.REFERENCES.items():
        pin = conversion.pin(ROOT / relative, check)
        require(pin["sha256"] == digest, "independent Compact reference source changed")
        reference_sources.append(pin)
    runtime, runtime_pin = legacy.read_pinned_json(
        plan["runtime_manifest"]["path"], plan["runtime_manifest"]["sha256"], check)
    import_pins = {row["path"]: row for row in plan["input_files"]}
    python_home = Path(plan["python"]["path"]).parent
    require(runtime_pin == plan["runtime_manifest"] and import_pins.get(runtime_pin["path"]) == runtime_pin
            and runtime["roots"] == runtime["import_paths"]
                == [str(python_home / "Lib"), str(python_home / "DLLs"), original["python_site"]]
            and all(path == row["path"] and import_pins.get(path) == row for path, row in runtime["files"].items()),
            "reviewed projection reference runtime closure differs")
    sys.path.extend(runtime["import_paths"])
    import onnx
    import torch

    half = legacy.load_module(ROOT / legacy.REFERENCE_OWNER, "compact_projection_independent_half")
    graph_verifier = legacy.load_module(Path(graph_source["path"]), "compact_projection_graph_verifier")
    import_pins.update({pin["path"]: pin for pin in reference_sources})
    legacy.loaded_runtime(import_pins, conversion.pin, check)
    graph = Path(baseline["graph"]["path"])
    typed_graph = graph_verifier.verify_projection_candidate_graph(
        onnx.load_model(graph, load_external_data=False),
        onnx.load_model(candidate["graph"]["path"], load_external_data=False), onnx,
        **({"output_projection": True} if output_projection else {}))
    require(typed_graph == stage["graph_proof"], "actual staged graph differs from reviewed projection proof")

    torch.set_num_threads(WIDER_CPU_THREADS if wider else 2)
    torch.set_num_interop_threads(1)
    independent, publisher = half.from_authenticated_original(
        torch, "compact", ROOT / legacy.PUBLISHER_OWNER, legacy.PUBLISHER_ROOT, "cpu")
    weights = Path(baseline["weights"]["path"])
    baseline_verification = half.verify_actual_half_parameters(
        independent, graph, onnx, {weights.name: weights}, candidate_sha256=baseline["graph"]["sha256"],
        original_graph=original_graph, original_files=original_files)
    check()
    order_observation = {}
    for reference in references:
        if wider:
            width = len(reference["input"]["input_ids"])
            print(f"AIRK_COMPACT_WIDER_CPU_START {width}", flush=True)
        if source_order:
            scores, observed = source_order_scores(half, independent, reference["input"], check)
            order_observation["source_order_observation"] = observed
        elif wider:
            scores = wider_projection_scores(half, independent, reference["input"], check)
        else:
            routes = OUTPUT_ROUTES if output_projection else ROUTES
            scores = projection_scores(independent, reference["input"], check, routes=routes)
        check()
        reference[matching_field] = scores
        require(legacy.finite_scores(scores), "eight finite independent Compact scores required")
        if wider:
            print(f"AIRK_COMPACT_WIDER_CPU_RETURN {width}", flush=True)
    reference_pins = {reference[key]["path"]: reference[key]
                      for reference in references for key in ("fixture", "producer")}
    for pin in (plan_pin, prior_pin, stage_pin, plan["source_graph"], closure_pin, runtime_pin,
                *baseline.values(), *candidate.values(), *publisher_files.values(), *reference_sources,
                *reference_pins.values()):
        conversion.pinned(pin, check)
    for name, pin in original_files.items():
        conversion.pinned(dict(pin, path=str(original_graph.parent / name)), check)
    loaded = legacy.loaded_runtime(import_pins, conversion.pin, check)
    return {
        "schema": schema, "complete": True, "model": legacy.MODEL,
        "candidate": candidate, "baseline_candidate": baseline, "stage_receipt": stage_pin,
        "conversion_plan": plan_pin, "conversion_receipt": prior_pin,
        "original_graph": plan["source_graph"], "original_closure": closure_pin,
        "publisher": publisher, "publisher_files": publisher_files,
        "verification": {"baseline": baseline_verification, "projection_graph": typed_graph},
        **({"references": references} if wider else {"reference": references[0]}),
        "reference_sources": reference_sources, "loaded_runtime": loaded,
        "recipe": source_order_recipe(source_order, output_projection, wider), **order_observation,
        "worker": source, "pid": os.getpid(), "execution_token": token,
        "original_deadline_monotonic": deadline, "reference_device": "cpu",
        "model_inference": True, "torch_forward_invocations": len(references),
        "native_execution": False, "qualification": False, "product_admission": False,
    }


def run_owned(out, *, source_order=False, output_projection=False, wider=False):
    started = time.monotonic()
    schema, owned_schema, marker, leaf, matching_field = reference_identity(source_order, output_projection, wider)
    legacy, legacy_pin = load_base()
    deadline = started + legacy.MAX_WALL_SECONDS

    def check():
        if time.monotonic() >= deadline:
            raise TimeoutError("original Compact projection reference deadline expired")

    plan, plan_pin = legacy.read_pinned_json(BASE_ARGUMENTS["conversion_plan"], BASE_ARGUMENTS["conversion_plan_sha256"], check)
    prior, prior_pin = legacy.read_pinned_json(
        BASE_ARGUMENTS["conversion_receipt"], BASE_ARGUMENTS["conversion_receipt_sha256"], check)
    wrapper = ROOT / "scripts/arbiter-fp16-execution.py"
    raw = wrapper.read_bytes()
    require(plan["wrapper"] == {"path": str(wrapper), "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()},
            "reviewed conversion wrapper differs")
    conversion = legacy.load_module(wrapper, "compact_projection_conversion_parent")
    token = conversion.windows_owner()
    require(plan["schema"] == conversion.SCHEMA and plan["model"] == "compact"
            and plan["human_sid"] == conversion.SID and plan["repository_root"] == str(ROOT)
            and prior["complete"] is True and prior["plan"] == plan_pin
            and conversion.pin(sys.executable, check) == plan["python"], "reviewed conversion and interpreter required")
    stage, stage_pin, candidate, graph_source = read_stage(
        legacy, conversion, plan_pin, token, check, output_projection=output_projection)
    helper_root = ROOT / "scripts/cuda-native-execution/helpers"
    for helper_leaf, digest in conversion.HELPERS.items():
        pin = plan["helpers"][helper_leaf]
        require(pin["path"] == str(helper_root / helper_leaf) and pin["sha256"] == digest, "reviewed owned helper differs")
        conversion.pinned(pin, check)
    sys.path.insert(0, str(helper_root))
    parent = legacy.load_module(helper_root / "reference_parent_no_site_r2.py", "compact_projection_parent")
    owner = legacy.load_module(helper_root / "reference_owned.py", "compact_projection_job")
    require(Path(parent.worker.__file__) == helper_root / "reference_worker.py", "owned reference dependency shadowed")
    require(out.is_absolute() and out.parent == CAMPAIGN and out.resolve(strict=False) == out and not out.exists(),
            "fresh checked direct campaign child required")
    temporary = out / "temp"
    for directory in (out, temporary):
        conversion.guard(plan, directory, deadline=deadline)
        directory.mkdir()
        conversion.guard(plan, directory, True, deadline=deadline)
    log, proof, receipt_path = (out / name for name in ("stdout-stderr.log", leaf, "execution.json"))
    for path in (log, proof, receipt_path):
        conversion.guard(plan, path, deadline=deadline)
    source = conversion.pin(Path(__file__).resolve(), check)
    argv = [plan["python"]["path"], "-I", "-B", "-S", "-X", "pycache_prefix=" + plan["cache_prefix"],
            source["path"], "--worker", "--deadline-monotonic", repr(deadline), "--worker-sha256", source["sha256"],
            "--stage-receipt", stage_pin["path"], "--stage-receipt-sha256", stage_pin["sha256"]]
    if source_order:
        argv.append("--source-order-rms-quickgelu")
    if output_projection:
        argv.append("--output-projection")
    if wider:
        argv.append("--wider")
    environment = {key: value for key, value in os.environ.items()
                   if not key.upper().startswith(("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_", "TORCH_", "TRITON_",
                                                  "HF_", "HUGGINGFACE_"))}
    environment.update(CUDA_VISIBLE_DEVICES="", HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1",
                       TOKENIZERS_PARALLELISM="false", TORCH_HOME=str(temporary),
                       TORCHINDUCTOR_CACHE_DIR=str(temporary), TRITON_CACHE_DIR=str(temporary),
                       TEMP=str(temporary), TMP=str(temporary), TMPDIR=str(temporary))
    host = 32 << 30
    receipt = {"schema": owned_schema,
               "complete": False, "failure": None, "qualification": False,
               "product_admission": False, "original_deadline_monotonic": deadline, "host_bytes": host,
               "command": argv, "execution_token": token, "worker": source,
               "conversion_plan": plan_pin, "conversion_receipt": prior_pin, "stage_receipt": stage_pin}

    def suspended(api, job, info, child_deadline):
        observed = parent.suspended_authentication(api, info, deadline, child_deadline, conversion.SID,
                                                   plan["python"], owner.image_proof)
        require(conversion.windows_owner(process=info.process) == token
                and owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1), "exact suspended owned process required")
        return dict(observed, token=token)

    try:
        with open(os.devnull, "rb") as stdin, log.open("xb") as stdout:
            conversion.guard(plan, log, True, deadline=deadline)
            receipt["owned"] = owner.run_windows_owned_training(
                argv, ROOT, stdout, deadline - time.monotonic(), environment, stdin, host,
                before_resume=suspended, absolute_deadline=deadline, cleanup_seconds=5)
        owned = receipt["owned"]
        require(parent.settled(owned) and type(owned["peak_host_committed_bytes"]) is int
                and 0 < owned["peak_host_committed_bytes"] <= host,
                f"CPU projection reference failed or did not settle within bounds, exit={owned['exit_code']}")
        conversion.guard(plan, log, True, deadline=deadline)
        require(0 < log.stat().st_size <= MAX_LOG, "bounded projection reference stdout required")
        with log.open("rb") as stream:
            lines = [line[len(marker):] for line in stream if line.startswith(marker)]
        require(len(lines) == 1 and 0 < len(lines[0]) <= MAX_PROOF, "exact bounded projection worker proof required")
        result = json.loads(lines[0])
        expected_references, _ = reference_inputs(legacy, check, wider=wider)
        matching_references(legacy, result, expected_references, matching_field, wider=wider)
        require(result["schema"] == schema
                and result["complete"] is result["model_inference"] is True
                and result["native_execution"] is result["qualification"] is result["product_admission"] is False
                and result["pid"] == owned["owned_leader_pid"] and result["execution_token"] == token
                and result["original_deadline_monotonic"] == deadline and result["reference_device"] == "cpu"
                and result["worker"] == source and result["stage_receipt"] == stage_pin and result["candidate"] == candidate
                and result["conversion_plan"] == plan_pin and result["conversion_receipt"] == prior_pin
                and result["baseline_candidate"] == {key: stage["baseline_" + key] for key in candidate}
                and result["verification"]["projection_graph"] == stage["graph_proof"]
                and result["recipe"] == source_order_recipe(source_order, output_projection, wider)
                and result.get("source_order_observation") == (
                    observed_source_order(RMS_SHAPES, 28) if source_order else None)
                and type(result["torch_forward_invocations"]) is int
                and result["torch_forward_invocations"] == len(expected_references),
                "independent projection observation differs from the exact owned request")
        for pin in (plan_pin, prior_pin, stage_pin, source, legacy_pin, graph_source):
            conversion.pinned(pin, check)
        require(not any(temporary.iterdir())
                and {path.name for path in out.iterdir()} == {"temp", log.name}, "unexpected owned reference output")
        with proof.open("xb") as output:
            output.write(lines[0])
        conversion.guard(plan, proof, True, deadline=deadline)
        receipt["proof"] = conversion.pin(proof, check)
        receipt["stdout"] = conversion.pin(log, check)
        receipt["complete"] = True
    except BaseException as error:
        receipt["failure"] = {"type": type(error).__name__, "message": str(error)}
        if hasattr(error, "owned_cleanup"):
            receipt["owned_cleanup"] = error.owned_cleanup
    finally:
        retention = time.monotonic() + 10
        try:
            for path in out.rglob("*"):
                conversion.guard(plan, path, True, deadline=retention)
            if log.is_file() and "stdout" not in receipt and 0 < log.stat().st_size <= MAX_LOG:
                receipt["stdout"] = conversion.pin(log)
        except (OSError, ValueError, TimeoutError) as error:
            receipt["complete"] = False
            receipt["retention_error"] = str(error)
            receipt["failure"] = receipt["failure"] or {"type": type(error).__name__, "message": str(error)}
        parent.final_phase_observation(receipt, deadline, started)
        conversion.guard(plan, receipt_path, deadline=retention)
        with receipt_path.open("x", encoding="utf-8", newline="\n") as output:
            json.dump(receipt, output, allow_nan=False)
            output.write("\n")
        conversion.guard(plan, receipt_path, True, deadline=retention)
    require(receipt["complete"], str(receipt["failure"]))
    return receipt["proof"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage-receipt", type=Path, required=True)
    parser.add_argument("--stage-receipt-sha256", required=True)
    recipe = parser.add_mutually_exclusive_group()
    recipe.add_argument("--source-order-rms-quickgelu", action="store_true")
    recipe.add_argument("--output-projection", action="store_true")
    recipe.add_argument("--wider", action="store_true")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--owned-output", type=Path)
    mode.add_argument("--worker", action="store_true")
    parser.add_argument("--worker-sha256")
    parser.add_argument("--deadline-monotonic", type=float)
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use isolated Windows Python -I -B -S")
    stage_path, stage_sha = (OUTPUT_STAGE, OUTPUT_STAGE_SHA) if args.output_projection else (STAGE, STAGE_SHA)
    require(stage_sha and args.stage_receipt == stage_path and args.stage_receipt_sha256 == stage_sha,
            "exact reviewed staged projection receipt required")
    source_order, output_projection = args.source_order_rms_quickgelu, args.output_projection
    if args.owned_output is not None:
        require(args.worker_sha256 is None and args.deadline_monotonic is None, "worker args require owned launch")
        proof = run_owned(args.owned_output, source_order=source_order, output_projection=output_projection,
                          wider=args.wider)
        print(json.dumps({"proof": proof, "qualification": False}))
        return
    require(args.worker_sha256 and args.deadline_monotonic and math.isfinite(args.deadline_monotonic),
            "owned worker source and finite original deadline required")
    result = worker(args.deadline_monotonic, args.worker_sha256, source_order=source_order,
                    output_projection=output_projection, wider=args.wider)
    _, _, marker, _, _ = reference_identity(source_order, output_projection, args.wider)
    encoded = json.dumps(result, allow_nan=False, separators=(",", ":")).encode("utf-8")
    require(len(encoded) <= MAX_PROOF and time.monotonic() < args.deadline_monotonic,
            "original projection proof size or deadline exceeded")
    print((marker + encoded).decode("utf-8"), flush=True)


if __name__ == "__main__":
    main()
