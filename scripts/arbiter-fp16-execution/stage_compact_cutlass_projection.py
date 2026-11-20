# SPDX-License-Identifier: Apache-2.0
"""Lower one pinned Compact full-projection graph to grouped causal FP32 CUTLASS attention."""
import hashlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
STAGE_SHA = "6486768c16820bc09f11c2dfbbbdd1d0ddf896e3a10b2315ec9f5d798ae9add1"
GRAPH_SHA = "fe94664a4addf797feaa54235e07f484f4e72051a894a154e5aff7f2c1c11ea6"
WEIGHT_SHA = "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3"
PREFIX = "/airk_compact_cutlass/"
SHAPES = {"split_kv":[0,0,8,1,128], "repeat_group":[1,1,1,2,1], "flat_heads":[0,0,2048]}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def original_and_replacements(onnx, layer):
    source = f"/airk_g5/attention/{layer}/"
    renamed = f"{PREFIX}{layer}/"
    native = f"/scorer/layers.{layer}/self_attn/"
    node = onnx.helper.make_node
    cast = lambda name, value, dest, dtype: node("Cast",[value],[dest],name=name,to=dtype)
    q, k, v = source+"q_flat", source+"k_flat", native+"v_proj/MatMul_output_0"
    old = [cast(source+"v16",v,source+"v16",10),cast(source+"q16",q,source+"q16",10),
           cast(source+"k16",k,source+"k16",10),
           node("GroupQueryAttention",[source+"q16",source+"k16",source+"v16","","","/airk_g5/seqlens32","/airk_g5/T32"],
                [source+"raw16",source+"present_key",source+"present_value"],name=source+"efficient",domain="com.microsoft",
                causal=1,do_rotary=0,kv_num_heads=8,local_window_size=-1,num_heads=16,scale=128**-0.5),
           cast(source+"output32",source+"raw16",native+"Reshape_7_output_0",1)]
    new = []
    for key, value in (("k",k),("v",v)):
        # [B,T,8,1,128] -> [B,T,8,2,128] preserves H0,H0,H1,H1,... grouping.
        group = renamed+key
        new.extend([node("Reshape",[value,PREFIX+"split_kv"],[group+"_split"],name=group+"_split"),
                    node("Tile",[group+"_split",PREFIX+"repeat_group"],[group+"_paired"],name=group+"_paired"),
                    node("Reshape",[group+"_paired",PREFIX+"flat_heads"],[group+"_flat"],name=group+"_flat")])
    new.append(node("MultiHeadAttention",[q,renamed+"k_flat",renamed+"v_flat"],[native+"Reshape_7_output_0"],
                    domain="com.microsoft",name=renamed+"efficient",num_heads=16,unidirectional=1,scale=128**-0.5))
    return old, new


def transform(onnx, source):
    require(hashlib.sha256(source.SerializeToString()).hexdigest()==GRAPH_SHA,
            "reviewed all-projection Compact graph required")
    before = {node.name:node for node in source.graph.node}
    require(len(before)==len(source.graph.node), "duplicate original nodes")
    changes = {}
    for layer in range(28):
        old, new = original_and_replacements(onnx,layer)
        for node in old:
            require(before.get(node.name) == node, "original protected attention boundaries differ: "+node.name)
            changes[node.name] = []
        changes[old[-2].name] = new
    result = onnx.ModelProto()
    result.CopyFrom(source)
    del result.graph.node[:]
    for node in source.graph.node:
        result.graph.node.extend(changes.get(node.name,[node]))
    for name,dims in SHAPES.items():
        result.graph.initializer.append(onnx.helper.make_tensor(PREFIX+name,onnx.TensorProto.INT64,[len(dims)],dims))
    verify(onnx,source,result)
    return result


def verify(onnx, original, candidate):
    """Recover the original graph bytes while checking every inserted node, original producer and output."""
    inversion = {}
    dropped = set()
    for layer in range(28):
        old, new = original_and_replacements(onnx, layer)
        for node in old:
            inversion[node.name] = new if node.name == old[-2].name else []
        for node in old[:-1]:
            dropped.update(node.output)
    actual = iter(candidate.graph.node)
    for node in original.graph.node:
        for expected in inversion.get(node.name,[node]):
            require(next(actual,None)==expected, "candidate topology or arithmetic differs at "+expected.name)
    require(next(actual,None) is None, "unexpected candidate nodes")
    require(not any(x in dropped for n in candidate.graph.node for x in n.input),
            "original half-attention or present output still consumed")
    expected_initializers = [onnx.helper.make_tensor(PREFIX+k,onnx.TensorProto.INT64,[len(v)],v) for k,v in SHAPES.items()]
    require(list(candidate.graph.initializer[-3:])==expected_initializers, "group geometry differs")
    inverse = onnx.ModelProto()
    inverse.CopyFrom(candidate)
    del inverse.graph.node[:]
    inverse.graph.node.extend(original.graph.node)
    del inverse.graph.initializer[-3:]
    require(inverse.SerializeToString()==original.SerializeToString(), "graph ABI, storage, metadata or scopes changed")
    return {"causal_fp32_mha":28,"half_boundary_casts_removed":112,"fp32_projection_count":196,
            "grouped_kv_heads":8,"query_heads":16,"head_dim":128,"original_graph_recovered":True,
            "dense_attention_score_tensor":False}

def stage(out):
    import json
    import os
    import runpy
    import sys
    import time

    stop=time.monotonic()+120
    def check():
        require(time.monotonic()<stop,"staging deadline expired")
    owner=runpy.run_path(str(HERE/"replay_compact_t1024_suffix.py"))
    replay=owner["module"]("replay_layer0.py",owner["REPLAY_SHA"])
    wider=owner["module"]("verify_projection_candidate_t32.py",replay.WIDER_REFERENCE_SHA)
    legacy,_=wider.load_base()
    plan,plan_pin=legacy.read_pinned_json(wider.BASE_ARGUMENTS["conversion_plan"],wider.BASE_ARGUMENTS["conversion_plan_sha256"],check)
    wrapper=Path(plan["wrapper"]["path"])
    if replay.file_pin(wrapper)!=plan["wrapper"]:
        wrapper=owner["CAMPAIGN"]/"compact-conversion-owner-311d13cc"/"arbiter-fp16-execution.py"
    require(replay.file_pin(wrapper)["sha256"]==plan["wrapper"]["sha256"],"original Windows guard changed")
    converter=legacy.load_module(wrapper,"cutlass_conversion_guard")
    token=converter.windows_owner()
    require(token["user_sid"]==token["owner_sid"]==plan["human_sid"] and replay.file_pin(Path(sys.executable))==plan["python"]
            and sys.pycache_prefix==plan["cache_prefix"] and not Path(plan["cache_prefix"]).exists()
            and out.parent==owner["CAMPAIGN"] and out==out.resolve() and not out.exists(),"wrong Windows owner, Python or output")
    previous,previous_pin=legacy.read_pinned_json(owner["CAMPAIGN"]/"compact-fp16-full-projection-staged-r1"/"stage.json",STAGE_SHA,check)
    operand,operand_pin=legacy.read_pinned_json(owner["CAMPAIGN"]/"compact-cutlass-fp32-original-output-reconciled-r2.json",
                                            "28f0b7458c3b76dbf315dac85680eb5d5b49bac3a6da84e417e0ebe8b1c9c90b",check)
    require(operand["complete"] and operand["head_values_compared"]==2048 and operand["retrospective_stdout_pin"]
            and previous["complete"] and previous["candidate_graph"]["sha256"]==GRAPH_SHA
            and previous["candidate_weights"]["sha256"]==WEIGHT_SHA,"reviewed operation, graph or storage differs")
    graph,weights=previous["candidate_graph"],previous["candidate_weights"]
    for pin in (graph,weights):
        converter.windows_owner(pin["path"])
        require(replay.file_pin(Path(pin["path"]),check)==pin,"all-projection original asset changed")
    runtime,runtime_pin=legacy.read_pinned_json(replay.RUNTIME,replay.RUNTIME_SHA,check)
    before=set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import onnx
    imports=legacy.loaded_runtime(runtime["files"],replay.file_pin,check,(sys.modules[n] for n in sys.modules.keys()-before))
    original=onnx.load_model(graph["path"],load_external_data=False)
    candidate=transform(onnx,original)
    dst_model,dst_weights,receipt=(out/name for name in ("model.onnx","weights.bin","stage.json"))
    converter.guard(plan,out,deadline=stop)
    out.mkdir()
    converter.guard(plan,out,True,deadline=stop)
    for path in (dst_model,dst_weights,receipt):
        converter.guard(plan,path,deadline=stop)
    with dst_model.open("xb") as stream:
        stream.write(candidate.SerializeToString())
    with Path(weights["path"]).open("rb") as source,dst_weights.open("xb") as target:
        for chunk in iter(lambda:source.read(16<<20),b""):
            check()
            target.write(chunk)
    onnx.checker.check_model(os.fspath(dst_model))
    proof=verify(onnx,original,onnx.load_model(dst_model,load_external_data=False))
    for path in (dst_model,dst_weights):
        converter.guard(plan,path,True,deadline=stop)
    dst_graph_pin=replay.file_pin(dst_model,check)
    dst_weight_pin=replay.file_pin(dst_weights,check)
    require(dst_graph_pin["sha256"]=="147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9"
            and dst_weight_pin["sha256"]==weights["sha256"] and dst_weight_pin["bytes"]==weights["bytes"],
            "serialized graph or original copied half storage differs")
    for pin in (graph,weights,previous_pin,operand_pin,runtime_pin,plan_pin):
        require(replay.file_pin(Path(pin["path"]),check)==pin,"original artifacts changed during stage")
    result={"schema":"airk.private.compact-cutlass-protected-staged/1","complete":True,"qualification":False,
            "product_admission":False,"native_load_attempts":0,"native_forward_attempts":0,"operand_only":operand_pin,
            "baseline_graph":graph,"baseline_weights":weights,"candidate_graph":dst_graph_pin,"candidate_weights":dst_weight_pin,
            "graph_proof":proof,"source":replay.file_pin(Path(__file__).resolve(),check),"conversion_plan":plan_pin,
            "runtime":runtime_pin,"baseline_stage":previous_pin,"verified_imports":len(imports),"execution_token":token,
            "command":sys.orig_argv,"elapsed_seconds":120-(stop-time.monotonic())}
    with receipt.open("x",encoding="utf8",newline="\n") as stream:
        json.dump(result,stream,allow_nan=False)
        stream.write("\n")
    converter.guard(plan,receipt,True,deadline=stop)
    print(json.dumps({"stage":replay.file_pin(receipt,check),"candidate":dst_graph_pin}))


if __name__=="__main__":
    import argparse
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",type=Path,required=True)
    stage(parser.parse_args().output)
