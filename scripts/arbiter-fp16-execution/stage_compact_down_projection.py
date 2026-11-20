# SPDX-License-Identifier: Apache-2.0
"""Stage a distinct Compact Q/K/V + FP32 MLP down-projection candidate."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import time


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
BASE_STAGE = "87ec206416eda607ff1585809d085e37fc4f8b99a5fc029308b5c78218cefcc2"
BASE_GRAPH = "e621cf6bffad1f0a27e3cc1ecba5b4379949295498f6585963620d71cfe4ff14"
PREFIX = "/airk_compact_mlp_down_fp32/"
ROUTES = tuple(f"/scorer/layers.{i}/mlp/down_proj/MatMul" for i in range(28))


def require(condition, message):
    if not condition:
        raise ValueError(message)


def nodes_for(onnx, name, weight):
    stem = "/airk_g5/weight/" + name
    layer = name.split("/")[2]
    prefix = PREFIX + layer + "/"
    out = name + "_output_0"
    original = [onnx.helper.make_node("MatMul",[stem+"/input16",weight],[stem+"/output16"],name=name),
                onnx.helper.make_node("Cast",[stem+"/output16"],[out],name=stem+"/output32",to=onnx.TensorProto.FLOAT)]
    protected = [onnx.helper.make_node("Cast",[stem+"/input16"],[prefix+"input32"],name=prefix+"input32",to=onnx.TensorProto.FLOAT),
                 onnx.helper.make_node("Cast",[weight],[prefix+"weight32"],name=prefix+"weight32",to=onnx.TensorProto.FLOAT),
                 onnx.helper.make_node("MatMul",[prefix+"input32",prefix+"weight32"],[out],name=name)]
    return original, protected


def transform(onnx, baseline):
    graph=baseline.graph
    positions={node.name:i for i,node in enumerate(graph.node)}
    tensors={tensor.name:tensor for tensor in graph.initializer}
    uses={}
    for node in graph.node:
        for value in node.input:
            uses.setdefault(value,[]).append(node.name)
    require(len(positions)==len(graph.node) and not any(PREFIX in s for n in graph.node
            for s in [n.name,*n.input,*n.output]),"ambiguous baseline or occupied namespace")
    edited={}
    for name in ROUTES:
        i=positions[name]
        matmul=graph.node[i]
        weight=matmul.input[1]
        original,protected=nodes_for(onnx,name,weight)
        require(matmul==original[0] and graph.node[i+1]==original[1],"original down MatMul or output cast differs")
        source="/scorer/"+name.split("/")[2]+"/mlp/Mul_output_0"
        cast=onnx.helper.make_node("Cast",[source],[matmul.input[0]],name=matmul.input[0],to=onnx.TensorProto.FLOAT16)
        require(graph.node[positions[matmul.input[0]]]==cast and uses[matmul.input[0]]==[name]
                and uses[matmul.output[0]]==[original[1].name] and uses[weight]==[name],
                "original activation or rounding boundary differs")
        t=tensors[weight]
        storage={field.key:field.value for field in t.external_data}
        require(t.data_type==onnx.TensorProto.FLOAT16 and t.data_location==onnx.TensorProto.EXTERNAL
                and list(t.dims)==[6144,2048] and storage["location"]=="weights.bin"
                and int(storage["length"])==6144*2048*2, "original down projection sidecar differs")
        edited[i]=protected
    candidate=onnx.ModelProto()
    candidate.CopyFrom(baseline)
    del candidate.graph.node[:]
    skip=set(i+1 for i in edited)
    for i,node in enumerate(graph.node):
        if i not in skip:
            candidate.graph.node.extend(edited.get(i,[node]))
    return candidate


def verify(onnx, baseline, candidate):
    # Recover the original Q/K/V graph by removing only the 28 new cast pairs
    # and reinstating each literal old output rounding boundary.
    restored=onnx.ModelProto()
    restored.CopyFrom(candidate)
    del restored.graph.node[:]
    i=0
    seen=[]
    changed=[]
    nodes=candidate.graph.node
    while i<len(nodes):
        node=nodes[i]
        if not node.name.startswith(PREFIX):
            restored.graph.node.append(node)
            i+=1
            continue
        require(i+2<len(nodes),"partial down route")
        weight=nodes[i+1].input[0]
        name=nodes[i+2].name
        require(name in ROUTES,"unreviewed FP32 down route")
        original,protected=nodes_for(onnx,name,weight)
        require(list(nodes[i:i+3])==protected,"invalid protected operands, output boundary or node")
        restored.graph.node.extend(original)
        seen.append(name)
        changed.append(weight)
        i+=3
    require(tuple(seen)==ROUTES and len(set(changed))==28 and restored.SerializeToString()==baseline.SerializeToString(),
            "inverse graph differs outside protected down projections")
    return {"prior_qkv_routes":84,"mlp_down_routes":list(ROUTES),"projection_count":112,
            "new_input_and_weight_casts":56,"removed_half_output_casts":28,"retained_sidecar":True,
            "source_graph_recovered_byte_for_byte":True}


def stage(out):
    deadline=time.monotonic()+120
    def check():
        require(time.monotonic()<deadline,"original staging deadline expired")
    # Reuse the independently frozen original conversion inputs and guard.
    suffix=HERE/"replay_compact_t1024_suffix.py"
    spec=importlib.util.spec_from_file_location("compact_down_source",suffix)
    owner=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    replay=owner.module("replay_layer0.py",owner.REPLAY_SHA)
    wider=owner.module("verify_projection_candidate_t32.py",replay.WIDER_REFERENCE_SHA)
    legacy,_=wider.load_base()
    plan,plan_pin=legacy.read_pinned_json(wider.BASE_ARGUMENTS["conversion_plan"],wider.BASE_ARGUMENTS["conversion_plan_sha256"],check)
    wrapper=Path(plan["wrapper"]["path"])
    if replay.file_pin(wrapper)!=plan["wrapper"]:
        wrapper=owner.CAMPAIGN/"compact-conversion-owner-311d13cc"/"arbiter-fp16-execution.py"
    require(replay.file_pin(wrapper)["sha256"]==plan["wrapper"]["sha256"],"wrong original guard/owner")
    converter=legacy.load_module(wrapper,"down_conversion")
    token=converter.windows_owner()
    require(token["owner_sid"]==plan["human_sid"] and replay.file_pin(Path(sys.executable))==plan["python"]
            and sys.pycache_prefix==plan["cache_prefix"] and not Path(plan["cache_prefix"]).exists()
            and out.parent==owner.CAMPAIGN and out==out.resolve() and not out.exists(),"wrong owner, Python or destination")
    previous,prev_pin=legacy.read_pinned_json(owner.CAMPAIGN/"compact-fp16-qkv-fp32-staged-r1"/"stage.json",BASE_STAGE,check)
    graph,weights=previous["candidate_graph"],previous["candidate_weights"]
    require(previous["complete"] and previous["graph_proof"]["projection_count"]==84 and graph["sha256"]==BASE_GRAPH,
            "wrong original Q/K/V graph")
    for pin in (graph,weights):
        converter.windows_owner(pin["path"])
        require(replay.file_pin(Path(pin["path"]),check)==pin,"changed original candidate")
    runtime,runtime_pin=legacy.read_pinned_json(replay.RUNTIME,replay.RUNTIME_SHA,check)
    converter.guard(plan,out,deadline=deadline)
    out.mkdir()
    converter.guard(plan,out,True,deadline=deadline)
    result=out/"stage.json"
    model=out/"model.onnx"
    sidecar=out/"weights.bin"
    for p in (result,model,sidecar):
        converter.guard(plan,p,deadline=deadline)
    sys.addaudithook(lambda event,values:converter.output_access(event,values,out))
    before=set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import onnx
    imported=legacy.loaded_runtime(runtime["files"],replay.file_pin,check,(sys.modules[n] for n in sys.modules.keys()-before))
    base=onnx.load_model(graph["path"],load_external_data=False)
    changed=transform(onnx,base)
    proof=verify(onnx,base,changed)
    with model.open("xb") as dst:
        dst.write(changed.SerializeToString())
    with Path(weights["path"]).open("rb") as source,sidecar.open("xb") as dst:
        for block in iter(lambda:source.read(16<<20),b""):
            check()
            dst.write(block)
    require(verify(onnx,base,onnx.load_model(model,load_external_data=False))==proof,"persisted graph differs")
    onnx.checker.check_model(os.fspath(model))
    for p in (model,sidecar):
        converter.windows_owner(p)
    candidate,external=replay.file_pin(model,check),replay.file_pin(sidecar,check)
    require(external["sha256"]==weights["sha256"] and external["bytes"]==weights["bytes"]
            and candidate["sha256"]!=graph["sha256"],"candidate graph or storage identity differs")
    for pin in (graph,weights,prev_pin,runtime_pin,plan_pin):
        require(replay.file_pin(Path(pin["path"]),check)==pin,"staging inputs changed")
    source_pin=replay.file_pin(Path(__file__).resolve(),check)
    fact={"schema":"airk.private.compact-fp32-qkv-down-projection-candidate/1","complete":True,
          "qualification":False,"product_admission":False,"native_load_attempts":0,"native_forward_attempts":0,
          "baseline_graph":previous["baseline_graph"],"baseline_weights":previous["baseline_weights"],
          "qkv_stage":prev_pin,"qkv_graph":graph,"candidate_graph":candidate,"candidate_weights":external,
          "graph_proof":proof,"staging_source":source_pin,"runtime":runtime_pin,"verified_imports":len(imported),
          "execution_token":token,"command":sys.orig_argv,"elapsed_seconds":120-(deadline-time.monotonic())}
    with result.open("x",encoding="utf8",newline="\n") as dst:
        json.dump(fact,dst,allow_nan=False)
        dst.write("\n")
    converter.windows_owner(result)
    check()
    print(json.dumps({"stage":replay.file_pin(result,check),"candidate":candidate}))


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",required=True,type=Path)
    args=parser.parse_args()
    require(sys.platform=="win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,"checked Windows Python required")
    stage(args.output)
