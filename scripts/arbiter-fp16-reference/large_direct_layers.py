# SPDX-License-Identifier: Apache-2.0
"""Compute an exact contiguous Large Direct layer range for bounded offline comparisons."""
from pathlib import Path
import runpy
import time

HERE = Path(__file__).resolve().parent
DIRECT = "0658ac65db0fb9aef920265d45dd61dd2494ea58d75aa365dfb191a2ac31230f"


def calculate(model, row, begin, end, previous, check, observe=None):
    """Return the next FP32 residual and, only at layer 36, the original sparse scores."""
    import hashlib
    source = HERE / "large_weight_only_reference.py"
    if hashlib.sha256(source.read_bytes()).hexdigest() != DIRECT:
        raise ValueError("reviewed direct Large recipe changed")
    owner = runpy.run_path(str(source))
    shared_path = HERE / "large_cutlass_reference.py"
    if hashlib.sha256(shared_path.read_bytes()).hexdigest() != owner["MHA_SOURCE"]:
        raise ValueError("reviewed Large grouped attention recipe changed")
    shared = runpy.run_path(str(shared_path))
    attention = shared["verified"](HERE.parent / "arbiter-fp16-execution/compact_cutlass_reference.py",
                                   owner["FAST_SOURCE"])["attention"]
    routes = shared["verified"](HERE / "large_body_reference.py", owner["BODY_SOURCE"])["ROUTES"]
    if not 0 <= begin < end <= model.layers == 36 or (previous is None) != (begin == 0):
        raise ValueError("invalid contiguous layer interval or missing checkpoint")
    if (model.hidden, model.qheads, model.kvheads, model.dim) != (2560, 32, 8, 128):
        raise ValueError("original Large grouped geometry required")
    t = model.torch
    ns = model.forward.__func__.__globals__
    ids, mask, positions, candidates, options = ns["validate_native"](t, model.config, False, row)
    ordered = iter(f"layers.{i}.{site}.weight" for i in range(begin, end) for site in routes)
    t.set_float32_matmul_precision("highest")

    def linear(value, name):
        check()
        weight = model.weights[name]
        if name != next(ordered, None) or value.dtype != t.float32 or weight.dtype != t.float16:
            raise ValueError("changed projection order, activation or weight precision")
        if value.device.type != "cpu" or weight.device.type != "cpu":
            raise ValueError("CPU reference requires original CPU operands")
        return t.nn.functional.linear(value, weight.float())

    n = len(ids)
    with t.inference_mode(), t.autocast(device_type="cpu", enabled=False):
        if previous is None:
            hidden, position = model._embed_inputs(ids, positions)
        else:
            hidden = previous
            position = t.tensor([positions], dtype=t.int64, device="cpu")
        if hidden.dtype != t.float32 or hidden.device.type != "cpu" or tuple(hidden.shape) != (1, n, 2560):
            raise ValueError("checkpoint must be an original FP32 residual")
        for i in range(begin, end):
            started = time.monotonic()
            root = f"layers.{i}."
            normal = model._attention_norm(hidden, root)
            q = linear(normal, root+"self_attn.q_proj.weight").reshape(1, n, 32, 128)
            k = linear(normal, root+"self_attn.k_proj.weight").reshape(1, n, 8, 128)
            v = linear(normal, root+"self_attn.v_proj.weight").reshape(1, n, 8, 128).transpose(1, 2)
            q = ns["rms"](t, q, model.weights[root+"self_attn.q_norm.weight"], model.eps).transpose(1, 2)
            k = ns["rms"](t, k, model.weights[root+"self_attn.k_norm.weight"], model.eps).transpose(1, 2)
            q, k = ns["rotary"](t, q, k, position, model.config["rope_theta"])
            result = shared["causal"](t, q, k, v, mask, check, attention)
            result = result.float().transpose(1, 2).reshape(1, n, 32*128)
            hidden = hidden + linear(result, root+"self_attn.o_proj.weight")
            normal = ns["rms"](t, hidden, model.weights[root+"post_attention_layernorm.weight"], model.eps)
            gate = linear(normal, root+"mlp.gate_proj.weight")
            up = linear(normal, root+"mlp.up_proj.weight")
            activated = gate*t.sigmoid(gate)
            hidden = hidden + linear(activated*up, root+"mlp.down_proj.weight")
            ns["checked_finite"](t, hidden, "FP32 residual")
            if observe is not None:
                observe(i, hidden, time.monotonic()-started)
            check()
        if next(ordered, None) is not None:
            raise ValueError("incomplete original layer interval")
        if end != 36:
            return hidden, None
        final = ns["rms"](t, hidden, model.weights["norm.weight"], model.eps)
        decision = final[:, row["decision_position"], :]
        rows = t.nn.functional.embedding(t.tensor([candidates], dtype=t.int64, device="cpu"),
                                          model.weights["embed_tokens.weight"]).float()
        scores = (rows*decision[:, None, :]).sum(-1).masked_fill(~t.tensor([options], dtype=t.bool, device="cpu"), -1e4)
        ns["checked_finite"](t, scores, "FP32 scores")
        return hidden, scores
