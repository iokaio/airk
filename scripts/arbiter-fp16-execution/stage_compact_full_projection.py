# SPDX-License-Identifier: Apache-2.0
"""Stock FP32 dense projection arithmetic with unchanged compressed storage and FP16 GQA."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import sys
import time


HERE = Path(__file__).resolve().parent
PREFIX = "/airk_compact_full_projection/"
SITES = {
    "self_attn/k_proj": ("input_layernorm/Mul_1_output_0", (2048,1024)),
    "self_attn/q_proj": ("input_layernorm/Mul_1_output_0", (2048,2048)),
    "self_attn/v_proj": ("input_layernorm/Mul_1_output_0", (2048,1024)),
    "self_attn/o_proj": ("self_attn/Reshape_7_output_0", (2048,2048)),
    "mlp/gate_proj": ("post_attention_layernorm/Mul_1_output_0", (2048,6144)),
    "mlp/up_proj": ("post_attention_layernorm/Mul_1_output_0", (2048,6144)),
    "mlp/down_proj": ("mlp/Mul_output_0", (6144,2048)),
}
NAMES={f"/scorer/layers.{i}/{site}/MatMul": (i,site) for i in range(28) for site in SITES}


def require(ok,message):
    if not ok:
        raise ValueError(message)


def replacement(onnx,name,weight):
    layer,site=NAMES[name]
    original="/airk_g5/weight/"+name
    root=f"/scorer/layers.{layer}/"
    activation=root+SITES[site][0]
    weight32=PREFIX+f"layers.{layer}/"+site+"/weight32"
    node=onnx.helper.make_node
    source=[node("Cast",[activation],[original+"/input16"],name=original+"/input16",to=onnx.TensorProto.FLOAT16),
            node("MatMul",[original+"/input16",weight],[original+"/output16"],name=name),
            node("Cast",[original+"/output16"],[name+"_output_0"],name=original+"/output32",to=onnx.TensorProto.FLOAT)]
    proposed=[node("Cast",[weight],[weight32],name=weight32,to=onnx.TensorProto.FLOAT),
              node("MatMul",[activation,weight32],[name+"_output_0"],name=name)]
    return source,proposed


def transform(onnx,baseline):
    nodes=baseline.graph.node
    weights={value.name:value for value in baseline.graph.initializer}
    locations={node.name:i for i,node in enumerate(nodes)}
    positions={}
    touched=set()
    seen_weights=set()
    uses={}
    for node in nodes:
        for value in node.input:
            uses.setdefault(value,[]).append(node.name)
    require(len(locations)==len(nodes) and not any(PREFIX in field for node in nodes for field in [node.name,*node.input,*node.output]),
            "ambiguous source nodes or occupied full projection namespace")
    for name,(layer,site) in NAMES.items():
        matmul=nodes[locations[name]]
        weight=matmul.input[1]
        old,new=replacement(onnx,name,weight)
        indices=[locations[n.name] for n in old]
        require(indices[0]<indices[1]<indices[2] and [nodes[i] for i in indices]==old,
                "original projection cast ordering or arithmetic differs: "+name)
        tensor=weights[weight]
        fields={field.key:field.value for field in tensor.external_data}
        dims=SITES[site][1]
        require(tuple(tensor.dims)==dims and tensor.data_type==onnx.TensorProto.FLOAT16
                and tensor.data_location==onnx.TensorProto.EXTERNAL and fields["location"]=="weights.bin"
                and int(fields["length"])==2*dims[0]*dims[1] and weight not in seen_weights
                and uses[weight]==[name] and uses[old[0].output[0]]==[name] and uses[old[1].output[0]]==[old[2].name],
                "original projection storage ownership or cast consumers differ: "+name)
        seen_weights.add(weight)
        positions[indices[0]]=[new[0]]
        positions[indices[1]]=[new[1]]
        touched.update(indices)
    result=onnx.ModelProto()
    result.CopyFrom(baseline)
    del result.graph.node[:]
    for i,node in enumerate(nodes):
        if i in positions:
            result.graph.node.extend(positions[i])
        elif i not in touched:
            result.graph.node.append(node)
    return result


def verify(onnx,baseline,candidate):
    restored=onnx.ModelProto()
    restored.CopyFrom(candidate)
    del restored.graph.node[:]
    nodes=candidate.graph.node
    by_name={node.name:node for node in nodes}
    require(len(by_name)==len(nodes),"duplicate candidate graph nodes")
    routes={}
    for name,(layer,site) in NAMES.items():
        weight32=PREFIX+f"layers.{layer}/"+site+"/weight32"
        cast=by_name.get(weight32)
        require(cast is not None and len(cast.input)==1 and name in by_name,"missing selected cast or matrix")
        old,new=replacement(onnx,name,cast.input[0])
        require(cast==new[0] and by_name[name]==new[1],"new route or FP32 arithmetic differs: "+name)
        for original,changed in zip(old,[new[0],new[1],None],strict=True):
            routes[original.name]=(original,changed)
    index=0
    for expected in baseline.graph.node:
        if expected.name in routes:
            prior,changed=routes[expected.name]
            if changed is not None:
                require(index<len(nodes) and nodes[index]==changed,"changed original graph ordering")
                index+=1
            restored.graph.node.append(prior)
        else:
            require(index<len(nodes) and nodes[index]==expected,"unrelated source node changed")
            restored.graph.node.append(nodes[index])
            index+=1
    require(index==len(nodes) and restored.SerializeToString()==baseline.SerializeToString(),
            "inverse does not recover the full original scientific FP16 graph")
    return {"projection_count":196,"fp32_weight_upcasts":196,"removed_activation_and_output_half_boundaries":392,
            "attention_nodes":28,"retained_sidecar":True,"source_graph_recovered_byte_for_byte":True}


def stage(out):
    deadline=time.monotonic()+120
    def check():
        require(time.monotonic()<deadline,"original staging deadline expired")
    path=HERE/"replay_compact_t1024_suffix.py"
    spec=importlib.util.spec_from_file_location("compact_checked_stage_inputs",path)
    owner=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    replay=owner.module("replay_layer0.py",owner.REPLAY_SHA)
    wider=owner.module("verify_projection_candidate_t32.py",replay.WIDER_REFERENCE_SHA)
    legacy,_=wider.load_base()
    plan,plan_pin=legacy.read_pinned_json(wider.BASE_ARGUMENTS["conversion_plan"],wider.BASE_ARGUMENTS["conversion_plan_sha256"],check)
    wrapper=Path(plan["wrapper"]["path"])
    if replay.file_pin(wrapper)!=plan["wrapper"]:
        wrapper=owner.CAMPAIGN/"compact-conversion-owner-311d13cc"/"arbiter-fp16-execution.py"
    require(replay.file_pin(wrapper)["sha256"]==plan["wrapper"]["sha256"],"checked conversion guard changed")
    converter=legacy.load_module(wrapper,"full_projection_stage_guard")
    token=converter.windows_owner()
    require(token["owner_sid"]==plan["human_sid"] and replay.file_pin(Path(sys.executable))==plan["python"]
            and sys.pycache_prefix==plan["cache_prefix"] and not Path(plan["cache_prefix"]).exists()
            and out.parent==owner.CAMPAIGN and out==out.resolve() and not out.exists(),"wrong owner, Python or output")
    previous,prev_pin=legacy.read_pinned_json(owner.CAMPAIGN/"compact-fp16-qkv-fp32-staged-r1"/"stage.json",
        "87ec206416eda607ff1585809d085e37fc4f8b99a5fc029308b5c78218cefcc2",check)
    graph,weights=previous["baseline_graph"],previous["baseline_weights"]
    require(previous["complete"] and graph["sha256"]=="971cf2d22d8bb7c874f3e5f5ce014bf2c494d1fb7245126d27d722223ac1577f",
            "wrong original FP16 model")
    for pin in (graph,weights):
        converter.windows_owner(pin["path"])
        require(replay.file_pin(Path(pin["path"]),check)==pin,"original graph or sidecar changed")
    runtime,runtime_pin=legacy.read_pinned_json(replay.RUNTIME,replay.RUNTIME_SHA,check)
    converter.guard(plan,out,deadline=deadline)
    out.mkdir()
    converter.guard(plan,out,True,deadline=deadline)
    dst_model,dst_sidecar,receipt=[out/name for name in ("model.onnx","weights.bin","stage.json")]
    for field in (dst_model,dst_sidecar,receipt):
        converter.guard(plan,field,deadline=deadline)
    sys.addaudithook(lambda event,args:converter.output_access(event,args,out))
    before=set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import onnx
    imports=legacy.loaded_runtime(runtime["files"],replay.file_pin,check,(sys.modules[n] for n in sys.modules.keys()-before))
    model=onnx.load_model(graph["path"],load_external_data=False)
    candidate=transform(onnx,model)
    proof=verify(onnx,model,candidate)
    with dst_model.open("xb") as dst:
        dst.write(candidate.SerializeToString())
    with Path(weights["path"]).open("rb") as src,dst_sidecar.open("xb") as dst:
        for chunk in iter(lambda:src.read(16<<20),b""):
            check()
            dst.write(chunk)
    require(verify(onnx,model,onnx.load_model(dst_model,load_external_data=False))==proof,"persisted topology differs")
    onnx.checker.check_model(os.fspath(dst_model))
    for field in (dst_model,dst_sidecar):
        converter.windows_owner(field)
    stored=replay.file_pin(dst_sidecar,check)
    require(stored["sha256"]==weights["sha256"] and stored["bytes"]==weights["bytes"],"stored half weight identity changed")
    for pin in (graph,weights,prev_pin,runtime_pin,plan_pin):
        require(replay.file_pin(Path(pin["path"]),check)==pin,"staging source changed")
    result={"schema":"airk.private.compact-full-projection-candidate/1","complete":True,"qualification":False,
            "product_admission":False,"native_load_attempts":0,"native_forward_attempts":0,
            "baseline_graph":graph,"baseline_weights":weights,"candidate_graph":replay.file_pin(dst_model,check),
            "candidate_weights":stored,"graph_proof":proof,"source":replay.file_pin(Path(__file__).resolve(),check),
            "conversion_plan":plan_pin,"runtime":runtime_pin,"source_receipt":prev_pin,"verified_imports":len(imports),
            "execution_token":token,"command":sys.orig_argv,"elapsed_seconds":120-(deadline-time.monotonic())}
    with receipt.open("x",encoding="utf8",newline="\n") as dst:
        json.dump(result,dst,allow_nan=False)
        dst.write("\n")
    converter.windows_owner(receipt)
    print(json.dumps({"stage":replay.file_pin(receipt,check),"candidate":result["candidate_graph"]}))


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",required=True,type=Path)
    args=parser.parse_args()
    require(sys.platform=="win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,"checked Windows Python required")
    stage(args.output)
