# SPDX-License-Identifier: Apache-2.0
"""Independent CPU recipe for 36 layers of seven FP32-accumulated half products and original causal Flash."""
import json
from pathlib import Path

STAGE = "a179d41c0dd4fe89ea489e7325365b63aa4825ffd0ac8dc52e17b46fcc3c973b"
GRAPH = "636cea20b5fa4f58251589d01a7ef2e3df87f7f4b57498d2765b0d56f6b87d2d"
ROUTES = ("self_attn.q_proj","self_attn.k_proj","self_attn.v_proj","self_attn.o_proj",
          "mlp.gate_proj","mlp.up_proj","mlp.down_proj")

def stage(campaign, pin, check):
    path=campaign/"large-fp32-body-stage-r1/stage.json"
    record=pin(path,check)
    if record["sha256"]!=STAGE:
        raise ValueError("Large body descriptor digest differs")
    d=json.loads(path.read_bytes())
    if (d["graph"]["sha256"]!=GRAPH or Path(d["graph"]["path"])!=path.with_name("model.onnx")
            or pin(path.with_name("model.onnx"),check)!=d["graph"] or d["verification"]["projection_count"]!=252
            or not d["verification"]["byte_exact_qkv_inverse"] or d["new_weight_bytes"]!=0):
        raise ValueError("Large body geometry or graph custody differs")
    return record,d

def score(independent, row, check):
    torch=independent.torch
    if (independent.layers,independent.hidden,independent.qheads,independent.kvheads,independent.dim)!=(36,2560,32,8,128):
        raise ValueError("Large original publisher geometry required")
    names=iter(f"layers.{layer}.{site}.weight" for layer in range(36) for site in ROUTES)
    torch.set_float32_matmul_precision("highest")
    def product(value,name):
        if name!=next(names,None):
            raise ValueError("Large body model order or product roster differs")
        weight=independent.weights[name]
        if (value.device.type!="cpu" or weight.device.type!="cpu" or value.dtype!=torch.float16
                or weight.dtype!=torch.float16):
            raise ValueError("independent original CPU half operands required")
        check()
        return torch.nn.functional.linear(value.float(),weight.float()).half()
    independent._linear_half=product
    try:
        scores=independent.forward(row,context_flash_single_tile=True)
    finally:
        del independent._linear_half
    if next(names,None) is not None:
        raise ValueError("Large body forward did not complete its 252 products")
    return scores
