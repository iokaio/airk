# SPDX-License-Identifier: Apache-2.0
"""Replay the governed Large layer 17 Flash and dense operators on their actual CUDA operands."""
import math
from pathlib import Path

GRAPH = "0d00e2fabdb5fe54474041fb1f2787502349b78ce40436b955cd4433633a4462"
NATIVE = "6eb43a120a45669915d99cdf57f71143228a6feeaaa1d0c444816ccb0f280790"
PROFILE = "33e1df2f103006d18e52995d27fd14f5224bfeca4e74e791b71d2f5399d30f4e"


def metrics(np, actual, expected):
    actual = actual.astype(np.float64)
    diff = actual - expected.astype(np.float64)
    if not np.isfinite(diff).all():
        raise ValueError("nonfinite operator difference")
    return {"elements":diff.size, "different":int(np.count_nonzero(diff)),
            "max_abs":float(np.abs(diff).max()), "rms":float(np.sqrt(np.mean(diff**2)))}


def replay(np, torch, half, exact, read_tensor, load, campaign, inputs, baseline):
    source = load({"path":campaign/"windows-large-protected-qkv-t32-r1/native.json","sha256":NATIVE})
    rows = load({"path":campaign/"windows-large-protected-qkv-graph-r1/native.json","sha256":GRAPH})["graph_metadata"]["rows"]
    values = {r["value"]:r for r in rows if r["kind"]=="value" and r["graph"]==0}
    nodes = {r["name"]:r for r in rows if r["kind"]=="node" and r["graph"]==0}
    a = "/airk_g5/attention/17/"
    layer = "/scorer/layers.17/"
    stored = "/airk_g5/weight/" + layer
    def cuda_ports(name, ports):
        node = nodes[name]
        if node["ep"] != "CUDAExecutionProvider":
            raise ValueError("unmeasured CUDA layer17 operand producer")
        for role, slot, expected in ports:
            edge, = [r for r in rows if r["kind"]=="edge" and r["graph"]==0 and r["node"]==node["node"]
                     and r["role"]==role and r["slot"]==slot]
            value = values[edge["value"]]
            if value["element_type"] != 10 or value["name"] != expected:
                raise ValueError(f"layer17 actual input/output binding differs: {expected}")
    cuda_ports(a+"efficient", [(0,0,a+"q16"),(0,1,a+"k16"),(0,2,stored+"self_attn/v_proj/MatMul/output16"),
                              (2,0,a+"raw16")])
    cuda_ports(layer+"self_attn/o_proj/MatMul",[(0,0,a+"raw16"),(0,1,"onnx::MatMul_11102"),
                                               (2,0,stored+"self_attn/o_proj/MatMul/output16")])
    cuda_ports(layer+"mlp/down_proj/MatMul",[(0,0,stored+"mlp/down_proj/MatMul/input16"),(0,1,"onnx::MatMul_11105"),
                                              (2,0,stored+"mlp/down_proj/MatMul/output16")])
    def operand_set(mode, pin):
        owned = load(pin)
        if (owned["mode"] != mode or not owned["complete"] or owned["whole_phase_expired"]
                or owned["owned"]["exit_code"] != 0 or owned["owned"]["owned_job_final_active_processes"] != 0):
            raise ValueError("completed owned original layer17 capture required")
        doc = load(owned["native"])
        if (doc["profile"] != PROFILE or doc["reviewed_graph"]["sha256"] != GRAPH or
                doc["rows"][0]["raw_scores"] != source["rows"][0]["raw_scores"]):
            raise ValueError("Large protected graph or original scores changed")
        return {tap["name"]:np.array(tap["bits"],dtype="<u2").view("<f2").reshape(tap["tokens"],tap["width"])
                for tap in doc["layer17_taps"]}
    att = operand_set("qkv-attention17",inputs[0])
    dense = operand_set("qkv-dense17",inputs[1])
    def heads(name, count):
        return torch.from_numpy(att[name].reshape(1,32,count,128)).transpose(1,2).contiguous()
    q,k,v = heads("q",32),heads("k",8),heads("v",8)
    if not np.array_equal(att["attention"][-1],dense["o_input"][0]):
        raise ValueError("separate runs disagree on actual attention-to-dense operand")
    with torch.inference_mode():
        h = half["causal_half_attention"](torch,q,k,v,[1]*32,context_flash_single_tile=True)
        cpu = h.transpose(1,2).reshape(32,4096).contiguous().numpy()
        scores = (q.double() @ k.repeat_interleave(4,dim=1).double().transpose(-1,-2))/math.sqrt(128)
        scores.masked_fill_(torch.ones((32,32),dtype=torch.bool).triu(1),float("-inf"))
        fp64 = (scores.softmax(-1) @ v.repeat_interleave(4,dim=1).double()).transpose(1,2).reshape(32,4096).numpy()
    products = {}
    for tag, parameter in (("o","layers.17.self_attn.o_proj.weight"),("down","layers.17.mlp.down_proj.weight")):
        weight = read_tensor(parameter)
        left = dense[tag+"_input"][0]
        observed = dense[tag+"_output"][0]
        if weight.shape != (left.size,observed.size) or weight.dtype != np.float16:
            raise ValueError("dense operator input/weight geometry differs")
        a = exact["half_array_units"](np,left.tobytes())
        result = np.empty(observed.size,dtype="<u2")
        for start in range(0, observed.size, 64):
            block = weight[:,start:start+64].copy()
            ints = exact["half_array_units"](np,block.tobytes()).reshape(block.shape)
            bound = int(np.abs(a).max()) * int(np.abs(ints).max()) * a.size
            if bound >= 2**63:
                raise ValueError("exact accumulation needs more than int64")
            result[start:start+block.shape[1]] = [exact["rounded_half_word"](int(x)) for x in (a @ ints)]
        result=result.view("<f2")
        products[tag]={"parameter":parameter,"original_expected_sha256":next(
            p["independent_expected_sha256"] for p in baseline["verification"]["parameters"] if p["parameter"]==parameter),
            "native_vs_exact_once_rounded":metrics(np,observed,result)}
    return {"schema":"airk.private.large-qkv-block17-operators/1","candidate_profile":PROFILE,
            "captures":inputs,"causal_half_schedule_all_tokens":metrics(np,att["attention"],cpu),
            "causal_half_schedule_decision":metrics(np,att["attention"][-1],cpu[-1]),
            "native_vs_fp64_once_rounded_decision":metrics(np,att["attention"][-1],fp64[-1].astype(np.float16)),
            "reference_vs_fp64_once_rounded_decision":metrics(np,cpu[-1],fp64[-1].astype(np.float16)),
            "dense":products}
