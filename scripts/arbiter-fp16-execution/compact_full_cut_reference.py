# SPDX-License-Identifier: Apache-2.0
"""Reproduce the original full-projection reference and isolate error on captured CUDA operands."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import time

HERE = Path(__file__).resolve().parent
FULL_SOURCE = "5b12a98cee8525323ac16076fe82e3b3895471c534cdc48947f76f9785813595"
FLASH_SOURCE = "24d551de98080eb338f602a725161ed35a95456646860575e1bf16a28d3284cb"
REFERENCE_SHA = "0995832e12ef5a84af11562187281c86b96304693053272b3b1fbd091026afc7"
ORIGINAL_SCORE = "735c77839099adf75426cf3ef9884ad69a99b60bb73e5fdf6247b7f321b12ded"
CUT_CLOSURES = {
    "head": ("compact-fp16-full-head-cut-r1", "9f9d69fe57a84bb8c74870d49a6eed144f71fae99e0b73fad13d822c961c53ca"),
    "attention": ("compact-fp16-full-attention-cut-r1", "2e59784add6b697286297e3b67cee693fcf7f882f8ab8e71d4bb03f1d27e3867"),
    "dense": ("compact-fp16-full-dense-cut-r1", "75a6107fe865e854bedd46e3c4e1cd0d3d8ec786b47062e8334f1166c3e492df"),
}


def module(path, name, digest):
    if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
        raise ValueError(f"source pin changed: {path}")
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def exact_head(closure_path, expected):
    """Audit the sparse dot products with bounded reads of seven half embedding rows."""
    import math
    import struct

    def digest(path):
        with Path(path).open("rb") as file:
            return hashlib.file_digest(file, "sha256").hexdigest()

    if digest(closure_path) != expected:
        raise ValueError("head owner receipt changed")
    closure = json.loads(Path(closure_path).read_bytes())
    owned = closure["owned"]
    if not (closure["complete"] and owned["exit_code"] == 0 and owned["owned_descendants_settled"]
            and owned["owned_job_final_active_processes"] == 0):
        raise ValueError("complete naturally settled head capture required")
    native_pin = closure["outputs"]["native"]
    if digest(native_pin["path"]) != native_pin["sha256"]:
        raise ValueError("head native source changed")
    native = json.loads(Path(native_pin["path"]).read_bytes())
    source = Path(closure_path).parent.parent
    original_path = source / "compact-fp16-full-engineering-score-r1/native-result.json"
    if digest(original_path) != ORIGINAL_SCORE:
        raise ValueError("sealed raw score source changed")
    original = json.loads(original_path.read_bytes())["rows"][0]
    score, = native["rows"]
    if not (native["complete"] and native["full_cut_group"] == "head" and native["forward_synchronized"]
            and score["compiled"] == original["compiled"] and score["raw_scores"] == original["raw_scores"]):
        raise ValueError("head diagnostic did not preserve the original input and score words")
    pin = closure["projection_assets"]["candidate_weights"]
    weight_path = Path(pin["path"])
    if (pin["sha256"] != "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3"
            or pin["bytes"] != 3441397760 or weight_path.stat().st_size != pin["bytes"]
            or digest(weight_path) != pin["sha256"]):
        raise ValueError("embedding sidecar changed")
    graph = closure["projection_assets"]["candidate_graph"]
    if graph["sha256"] != "fe94664a4addf797feaa54235e07f484f4e72051a894a154e5aff7f2c1c11ea6" or digest(graph["path"]) != graph["sha256"]:
        raise ValueError("validated full projection graph changed")
    final, = [r for r in native["full_cut_rows"] if r["name"] == "/scorer/norm/Mul_1_output_0"]
    if final["tokens_retained"] != 1 or final["width"] != 2048 or len(final["words"]) != 4096:
        raise ValueError("original final FP32 decision row absent")
    hidden = struct.unpack("<2048f", struct.pack("<4096H", *final["words"]))
    # The pinned graph declares scorer.text.embed_tokens.weight at byte offset zero,
    # shape [151936,2048], FLOAT16, and gathers by the candidate vocabulary IDs.
    candidates = score["compiled"]["candidate_positions"]
    if candidates != list(range(32, 39)):
        raise ValueError("original seven candidate IDs required")
    exact = []
    with weight_path.open("rb") as file:
        for token in candidates:
            file.seek(token * 2048 * 2)
            embedding = struct.unpack("<2048e", file.read(4096))
            exact.append(math.fsum(x * y for x, y in zip(embedding, hidden, strict=True)))
    return {"schema": "airk.private.compact-full-t1014-independent-head/1",
            "owned_capture_sha256": expected, "native": native_pin, "graph": graph,
            "embedding_sidecar": pin, "candidate_vocabulary_ids": candidates, "fp64_head": exact,
            "observed_scores": score["raw_scores"], "matching_scores": score["matching_fp16"],
            "max_head_abs": max(abs(x-y) for x, y in zip(exact, score["raw_scores"], strict=True)),
            "max_inherited_state_abs": max(abs(x-y) for x, y in zip(exact, score["matching_fp16"], strict=True))}


def exact_dense_attention(closure_path, expected, *, include_fp64=False):
    """Recompute the decision row in FP64 from native operands, with original GQA and causal support."""
    import math
    import struct

    def pinned(path, sha):
        data = Path(path).read_bytes()
        if hashlib.sha256(data).hexdigest() != sha:
            raise ValueError("attention source bytes changed")
        return json.loads(data)

    owner = pinned(closure_path, expected)
    owned = owner["owned"]
    if not (owner["complete"] and owned["owned_descendants_settled"] and owned["exit_code"] == 0
            and owned["owned_job_final_active_processes"] == 0 and owner["full_cut_group"] == "attention"):
        raise ValueError("naturally settled attention capture required")
    native_pin = owner["outputs"]["native"]
    native = pinned(native_pin["path"], native_pin["sha256"])
    score, = native["rows"]
    original = pinned(Path(closure_path).parent.parent / "compact-fp16-full-engineering-score-r1/native-result.json", ORIGINAL_SCORE)
    if not (native["complete"] and native["forward_synchronized"] and native["full_cut_group"] == "attention"
            and score["compiled"] == original["rows"][0]["compiled"] and score["raw_scores"] == original["rows"][0]["raw_scores"]
            and score["compiled"]["decision_position"] == 1013 and score["compiled"]["attention_mask"] == [1] * 1014):
        raise ValueError("original complete decision query and all causal keys required")
    def tensor(short, tokens, width):
        name = f"/airk_g5/attention/13/{short}"
        item, = [r for r in native["full_cut_rows"] if r["name"] == name]
        count = tokens * width
        if item["tokens_retained"] != tokens or item["width"] != width or len(item["words"]) != count:
            raise ValueError("invalid native attention operand geometry")
        return struct.unpack(f"<{count}e", struct.pack(f"<{count}H", *item["words"]))

    q, k, v, raw = (tensor(n, t, w) for n, t, w in (("q16",1,2048), ("k16",1014,1024),
                                                       ("v16",1014,1024), ("raw16",1,2048)))
    exact = []
    for head in range(16):
        query = q[128 * head:128 * (head + 1)]
        offset = 128 * (head // 2)
        keys = range(offset, len(k), 1024)
        logits = [math.sumprod(query, k[i:i + 128]) / math.sqrt(128) for i in keys]
        maximum = max(logits)
        numerators = [math.exp(x - maximum) for x in logits]
        denom = math.fsum(numerators)
        for i in range(128):
            exact.append(math.sumprod(numerators, v[offset + i::1024]) / denom)
    rounded = struct.unpack("<2048e", struct.pack("<2048e", *exact))
    errors = [a - b for a, b in zip(raw, rounded, strict=True)]
    return {"schema": "airk.private.compact-full-t1014-local-dense-attention/1", "owner_sha256": expected,
            "native": native_pin, "layer": 13, "decision": 1013, "native_half_words": len(raw),
            "rounded_dense_mismatches": sum(x != 0 for x in errors), "rounded_dense_max_abs": max(map(abs, errors)),
            "rounded_dense_rms": math.sqrt(math.sumprod(errors, errors) / len(errors)),
            **({"fp64_decision_values": exact} if include_fp64 else {})}


def exact_dense_projections(closure_path, expected):
    """Accumulate two real layer-13 matrix-vector products from original external half rows."""
    import math
    import struct

    def digest(path):
        with Path(path).open("rb") as file:
            return hashlib.file_digest(file, "sha256").hexdigest()

    if digest(closure_path) != expected:
        raise ValueError("dense closure differs")
    owner = json.loads(Path(closure_path).read_bytes())
    job = owner["owned"]
    if not (owner["complete"] and owner["full_cut_group"] == "dense" and job["exit_code"] == 0
            and job["owned_descendants_settled"] and job["owned_job_final_active_processes"] == 0):
        raise ValueError("closed dense group required")
    native_pin = owner["outputs"]["native"]
    graph = owner["projection_assets"]["candidate_graph"]
    weights = owner["projection_assets"]["candidate_weights"]
    if (graph["sha256"] != "fe94664a4addf797feaa54235e07f484f4e72051a894a154e5aff7f2c1c11ea6"
            or weights["sha256"] != "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3"
            or any(digest(p["path"]) != p["sha256"] for p in (graph, weights, native_pin))):
        raise ValueError("dense graph, original weights or synced output changed")
    native = json.loads(Path(native_pin["path"]).read_bytes())
    original_path = Path(closure_path).parent.parent / "compact-fp16-full-engineering-score-r1/native-result.json"
    if digest(original_path) != ORIGINAL_SCORE:
        raise ValueError("original CUDA score changed")
    original = json.loads(original_path.read_bytes())["rows"][0]
    score, = native["rows"]
    if not (native["forward_synchronized"] and native["full_cut_group"] == "dense" and native["complete"]
            and score["compiled"] == original["compiled"] and score["raw_scores"] == original["raw_scores"]):
        raise ValueError("dense run changed original inputs or scores")

    def tensor(name, width, dtype):
        item, = [r for r in native["full_cut_rows"] if r["name"] == name]
        if item["width"] != width or item["tokens_retained"] != 1:
            raise ValueError("incorrect dense operand shape")
        return struct.unpack(f"<{width}{dtype}", struct.pack(f"<{len(item['words'])}H", *item["words"]))

    # Original authenticated ONNX MatMul initializers, float16 [input,2048].
    # The two external_data ranges are taken from the distinct weight Cast inputs.
    layer = "/scorer/layers.13/"
    matrices = (("self_attn/o_proj", "onnx::MatMul_8662", 1948225536, 2048, "/airk_g5/attention/13/raw16", "e"),
                ("mlp/down_proj", "onnx::MatMul_8665", 2006945792, 6144, layer + "mlp/Mul_output_0", "f"))
    results = {}
    with Path(weights["path"]).open("rb") as file:
        for site, initializer, offset, width, source, dtype in matrices:
            file.seek(offset)
            actual_inputs = tensor(source, width, dtype)
            observed = tensor(layer + site + "/MatMul_output_0", 2048, "f")
            reference = [0.] * 2048
            for x in actual_inputs:
                weight = struct.unpack("<2048e", file.read(4096))
                reference = [total + x * w for total, w in zip(reference, weight, strict=True)]
            differences = [a - b for a, b in zip(observed, reference, strict=True)]
            results[site] = {"initializer": initializer, "offset": offset, "matmul_input_width": width,
                             "max_abs": max(map(abs, differences)), "rms": math.sqrt(math.sumprod(differences, differences) / 2048),
                             "relative_l2": math.sqrt(math.sumprod(differences, differences) / math.sumprod(reference, reference))}
    return {"schema": "airk.private.compact-full-t1014-local-dense-projections/1", "owner_sha256": expected,
            "native": native_pin, "graph": graph, "weight": weights, "decision": 1013, "results": results}


def differences(torch, observed, expected):
    observed, expected = observed.double().flatten(), expected.double().flatten()
    if observed.shape != expected.shape or not torch.isfinite(observed).all() or not torch.isfinite(expected).all():
        raise ValueError("cut comparison shape or finite-value check failed")
    delta = observed - expected
    return {"elements": delta.numel(), "max_abs": delta.abs().max().item(),
            "rms": delta.square().mean().sqrt().item(),
            "relative_l2": (delta.norm() / expected.norm().clamp_min(1e-30)).item()}


def rows(torch, cuts):
    result = {}
    for record in (r for cut in cuts for r in cut["full_cut_rows"]):
        name, bits, width = record["name"], record["words"], record["width"]
        if any(type(x) is not int or not 0 <= x <= 65535 for x in bits):
            raise ValueError("duplicate cut value or invalid native word")
        dtype = torch.float32 if name.startswith("/scorer/") else torch.float16
        values = torch.tensor(bits, dtype=torch.uint16).view(dtype)
        length = 1014 if name in ("/airk_g5/attention/13/k16", "/airk_g5/attention/13/v16") else 1
        if record["tokens_retained"] != length or values.numel() != width * length:
            raise ValueError("unexpected full-cut geometry")
        values = values.view(length, width)
        if name in result:
            if name != "/airk_g5/attention/13/raw16" or not torch.equal(result[name].view(torch.int16), values.view(torch.int16)):
                raise ValueError("shared attention output changed across independent cut sessions")
        result[name] = values
    if len(result) != 11 or any(len(cut["full_cut_rows"]) != 4 for cut in cuts):
        raise ValueError("three complete four-output sessions with one common attention output required")
    return result


def compare_model(base, full, flash, half, wider, model, row, cuts, expected_scores, check):
    torch = model.torch
    decision = row["decision_position"]
    actual = rows(torch, cuts)
    comparisons = {}
    local_products = {}
    root_name = lambda n: f"/scorer/layers.{n}/"
    att_name = lambda n: f"/airk_g5/attention/{n}/"
    layer = 0
    routes = []
    scale = flash.operand_scale()
    previous_attention = half.causal_half_attention
    previous_rms = half.rms

    def record(name, value):
        comparisons[name] = differences(torch, actual[name][-1], value)

    def linear(value, name):
        check()
        routes.append(name)
        weight = model.weights[name]
        product = torch.nn.functional.linear(value, weight.float())
        _, index, *site_parts, _ = name.split(".")
        number = int(index)
        site = "/".join(site_parts)
        if number in (0, 13, 27) and site == "self_attn/q_proj":
            record(root_name(number) + "input_layernorm/Mul_1_output_0", value[0, decision])
        if number == 13 and site in ("self_attn/o_proj", "mlp/down_proj"):
            name = root_name(number) + site + "/MatMul_output_0"
            if site == "self_attn/o_proj":
                source = att_name(number) + "raw16"
            else:
                source = root_name(number) + "mlp/Mul_output_0"
                record(source, value[0, decision])
            record(name, product[0, decision])
            exact = weight.double() @ actual[source][-1].double()
            local_products[name] = differences(torch, actual[name][-1], exact)
        return product

    def attention(torch, q, k, v, mask, *, context_flash_single_tile):
        nonlocal layer
        if context_flash_single_tile:
            raise ValueError("unexpected single-tile request")
        result = base.ragged_flash(torch, wider, q, k, v, mask, scale, check)
        if layer == 13:
            for suffix, value in (("q16", q), ("k16", k), ("v16", v), ("raw16", result)):
                record(att_name(layer) + suffix, value[0, :, decision, :].reshape(-1))
        layer += 1
        return result

    def rms(torch, hidden, weight, eps):
        result = previous_rms(torch, hidden, weight, eps)
        if weight is model.weights["norm.weight"]:
            record("/scorer/norm/Mul_1_output_0", result[0, decision])
        return result

    try:
        model.linear = linear
        half.causal_half_attention = attention
        half.rms = rms
        scores = model.forward(row, context_flash_single_tile=False)[0].tolist()
    finally:
        del model.linear
        half.causal_half_attention = previous_attention
        half.rms = previous_rms
    if layer != 28 or tuple(routes) != full.ROUTES or scores != expected_scores or set(comparisons) != set(actual):
        raise ValueError("independent full CPU score, route count or tap roster changed")

    # Fill the last 64-wide query tile; only the observed decision query is used.
    # Padding columns past position 1013 are always causally masked for this row.
    start = decision // 64 * 64
    query = torch.zeros(1, 16, 64, 128)
    query[0, :, decision - start, :] = actual[att_name(13) + "q16"][-1].view(16, 128)
    def operands(name):
        value = actual[att_name(13) + name].view(1, 1014, 8, 128).transpose(1, 2)
        return torch.nn.functional.pad(value.repeat_interleave(2, dim=1).float(), (0, 0, 0, 10))
    replay = flash.flash_block(torch, query, operands("k16"), operands("v16"), start, check)
    independent_attention = differences(torch, actual[att_name(13) + "raw16"][-1], replay[0, :, decision - start].flatten())
    head = model.weights["embed_tokens.weight"][row["candidate_positions"]].double()
    same_state = head @ actual["/scorer/norm/Mul_1_output_0"][-1].double()
    native_scores = torch.tensor(cuts[0]["rows"][0]["raw_scores"], dtype=torch.float64)
    return {"cpu_score_words_reproduced": True, "boundaries": comparisons, "same_input_fp64_dots": local_products,
            "same_native_operands_sm86_attention": independent_attention, "head_from_native_final_norm": same_state.tolist(),
            "head_only": differences(torch, native_scores, same_state)}


def worker(args, full, base, replay, wider):
    def check():
        full.require(time.monotonic() < args.worker_deadline, "CPU cut comparison deadline expired")
    placement = None
    if args.cal_pid is not None:
        affinity = module(HERE.parent / "arbiter/analysis/development_margins/observe_cal_thread_affinity.py", "affinity",
                          "7f37dcc3f9be734a964f561d9afc68b54ede19d036094b7fb7dab697f7d3f57f")
        placement = affinity.place_optional_worker(args.cal_pid, 2)
        full.require(len(placement["logical_cpus"]) == 2, "expected two selected CPU cores")
    legacy = replay.read_only_bootstrap()
    case, census, capture, _, row = base.inputs(legacy, args.case, args.census_sha256, args.ordinal, check)
    full.require(case == "ENGINEERING_FIXED" and set(CUT_CLOSURES) == {"head", "attention", "dense"}, "settled original engineering cut required")
    reference, ref_pin = legacy.read_pinned_json(base.CAMPAIGN / "compact-engineering-full-projection-reference-r1/execution.json", REFERENCE_SHA, check)
    original, score_pin = legacy.read_pinned_json(base.CAMPAIGN / "compact-fp16-full-engineering-score-r1/native-result.json", ORIGINAL_SCORE, check)
    cuts, cut_pins = [], []
    for group, (leaf, sha) in CUT_CLOSURES.items():
        directory = base.CAMPAIGN / leaf
        owner, owner_pin = legacy.read_pinned_json(directory / "execution.json", sha, check)
        full.require(owner["complete"] and owner["owned"]["exit_code"] == 0 and owner["owned"]["owned_descendants_settled"]
                     and owner["owned"]["owned_job_final_active_processes"] == 0 and owner["full_cut_group"] == group,
                     "original four-output group Job did not close")
        native, cut_pin = legacy.read_pinned_json(directory / "native-result.json", owner["outputs"]["native"]["sha256"], check)
        full.require(native["complete"] and native["forward_synchronized"] and native["rows"][0]["compiled"] == row
                     and native["full_cut_group"] == group and original["rows"][0]["raw_scores"] == native["rows"][0]["raw_scores"]
                     and reference["result"]["compiled"] == row, "original native/CPU score input differs")
        cuts.append(native)
        cut_pins.append({"group":group, "owner":owner_pin, "native":cut_pin})
    source, source_pin = legacy.read_pinned_json(base.CAMPAIGN / "compact-fp16-qkv-fp32-wider-reference-r1" / wider.WIDER_LEAF,
                                                 base.owner.REFERENCE_SHA, check)
    for pin in source["publisher_files"].values():
        full.require(replay.file_pin(Path(pin["path"]), check) == pin, "publisher weights changed")
    runtime, runtime_pin = legacy.read_pinned_json(replay.RUNTIME, replay.RUNTIME_SHA, check)
    flash = module(HERE.parent / "arbiter-fp16-reference" / "large_flash.py", "sm86", FLASH_SOURCE)
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import torch
    torch.set_num_threads(2)
    full.require(torch.get_num_threads() == 2, "independent cut job requires two CPU threads")
    torch.set_num_interop_threads(1)
    half = base.owner.module("../arbiter-fp16-reference.py", legacy.REFERENCES[legacy.REFERENCE_OWNER])
    publisher = base.owner.module("../" + legacy.PUBLISHER_OWNER.removeprefix("scripts/"), legacy.REFERENCES[legacy.PUBLISHER_OWNER])
    half.freeze_reference_math(torch)
    weights, config = publisher.load_authenticated(legacy.PUBLISHER_ROOT, "Qwen/Qwen3-1.7B")
    model = half.HalfReference(torch, config, weights.text.state_dict(), False, "cpu")
    del weights
    with torch.inference_mode():
        result = compare_model(base, full, flash, half, wider, model, row, cuts, reference["result"]["matching_fp16"], check)
    check()
    full.require(not torch.cuda.is_initialized(), "unexpected GPU initialization")
    imports = dict(runtime["files"])
    for pin in [*source["reference_sources"], replay.file_pin(Path(__file__).resolve()),
                replay.file_pin(HERE / "compact_full_projection_reference.py"), replay.file_pin(HERE / "representative_compact_reference.py")]:
        imports[pin["path"]] = pin
    legacy.loaded_runtime(imports, replay.file_pin, check, [sys.modules[n] for n in sys.modules.keys() - before])
    return {"schema": "airk.private.compact-full-fixed-cut-comparison/1", "case": case, "capture": capture,
            "cpu_placement": placement, "census": census, "source": source_pin, "runtime": runtime_pin, "original": score_pin,
            "cuts": cut_pins, "independent_reference": ref_pin, **result, "qualification": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--output", type=Path)
    group.add_argument("--worker-deadline", type=float)
    parser.add_argument("--source-sha256")
    parser.add_argument("--case", required=True)
    parser.add_argument("--census-sha256", required=True)
    parser.add_argument("--ordinal", type=int, required=True)
    parser.add_argument("--cal-pid", type=int)
    args = parser.parse_args()
    full = module(HERE / "compact_full_projection_reference.py", "full_projection", FULL_SOURCE)
    base = full.load_base()
    replay = base.owner.module("replay_layer0.py", base.owner.REPLAY_SHA)
    wider = base.owner.module("verify_projection_candidate_t32.py", replay.WIDER_REFERENCE_SHA)
    full.require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
                 "checked Windows worker required")
    if args.output:
        base.owner.run_owned(replay, wider, args.output, representative=(args.case, args.census_sha256, args.ordinal),
                             full_cuts=True, full_cuts_cal_pid=args.cal_pid)
    else:
        full.require(replay.file_pin(Path(__file__).resolve())["sha256"] == args.source_sha256
                     and 0 < args.worker_deadline - time.monotonic() <= 600, "source or deadline changed")
        print(base.owner.MARKER + json.dumps(worker(args, full, base, replay, wider), allow_nan=False))
