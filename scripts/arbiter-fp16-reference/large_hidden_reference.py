# SPDX-License-Identifier: Apache-2.0
"""Record four decision-token intermediates during the separately frozen Large Q/K/V reference."""
import math
import struct

EARLY = "/scorer/layers.0/input_layernorm/Mul_1_output_0"
MID = "/scorer/layers.17/input_layernorm/Mul_1_output_0"
RESIDUAL = "/scorer/layers.35/Add_1_output_0"
FINAL = "/scorer/norm/Mul_1_output_0"
NAMES = (EARLY, MID, RESIDUAL, FINAL)


def capture(model, row, check, score):
    torch = model.torch
    if row["decision_position"] != 31 or row["candidate_positions"] != list(range(32, 40)):
        raise ValueError("original T32 decision and eight candidate token IDs required")
    observed = {}
    original_attention = model._attention_norm
    namespace = model.forward.__func__.__globals__
    original_rms = namespace["rms"]

    def encode(values):
        flat = values.detach().float().contiguous().view(-1)
        if flat.numel() != 2560 or not torch.isfinite(flat).all():
            raise ValueError("one finite FP32 Large hidden vector required")
        return struct.pack("<2560f", *flat.tolist()).hex()

    def record(name, values):
        if name in observed:
            raise ValueError("repeated hidden boundary")
        observed[name] = encode(values[0, 31])

    def attention(hidden, root):
        value = original_attention(hidden, root)
        if root in ("layers.0.", "layers.17."):
            record(EARLY if root == "layers.0." else MID, value)
        return value

    def rms(t, value, scale, eps):
        result = original_rms(t, value, scale, eps)
        if scale is model.weights["norm.weight"]:
            record(RESIDUAL, value)
            record(FINAL, result)
        return result

    model._attention_norm = attention
    namespace["rms"] = rms
    try:
        scores = score(model, row, check)
    finally:
        del model._attention_norm
        namespace["rms"] = original_rms
    if tuple(observed) != NAMES:
        raise ValueError("incomplete or reordered decision-token boundaries")
    embedding = model.weights["embed_tokens.weight"]
    return scores, {"fp32_le_hex": observed, "scale": encode(model.weights["norm.weight"]),
                    "epsilon": model.eps, "candidate_ids": row["candidate_positions"],
                    "candidate_embeddings": [encode(embedding[i]) for i in row["candidate_positions"]]}


def compare(native, reference, original_scores):
    if native["rows"][0]["raw_scores"] != original_scores or not native["complete"]:
        raise ValueError("native capture changed the untreated candidate scores")
    taps = native["hidden_taps"]
    if tuple(item["name"] for item in taps) != NAMES:
        raise ValueError("distinct four native hidden boundaries required")
    def unpack(v):
        return struct.unpack("<2560f", bytes.fromhex(v))
    actual = {}
    per_boundary = {}
    for tap in taps:
        if tap["width"] != 2560 or tap["token"] != 31 or len(tap["bits"]) != 5120:
            raise ValueError("malformed native FP32 row")
        name = tap["name"]
        values = struct.unpack("<2560f", struct.pack("<5120H", *tap["bits"]))
        expected = unpack(reference["fp32_le_hex"][name])
        diff = [a-b for a,b in zip(values, expected, strict=True)]
        if not all(math.isfinite(d) for d in diff):
            raise ValueError("nonfinite reference comparison")
        per_boundary[name] = {"max_abs":max(map(abs,diff)), "rms":math.sqrt(math.fsum(d*d for d in diff)/len(diff)),
                              "different":sum(d!=0 for d in diff)}
        actual[name] = values
    scale = unpack(reference["scale"])
    residual = actual[RESIDUAL]
    inv = 1/math.sqrt(math.fsum(x*x for x in residual)/len(residual) + reference["epsilon"])
    norm_error = max(abs(x*inv*s-y) for x,s,y in zip(residual, scale, actual[FINAL], strict=True))
    exact = [math.fsum(x*w for x,w in zip(actual[FINAL],unpack(vector),strict=True))
             for vector in reference["candidate_embeddings"]]
    return {"boundaries": per_boundary, "native_residual_fp64_norm_max_abs":norm_error,
            "fp64_head_from_native_decision":exact,
            "native_readout_max_abs":max(abs(a-b) for a,b in zip(exact,original_scores,strict=True))}
