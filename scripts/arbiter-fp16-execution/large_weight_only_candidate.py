# SPDX-License-Identifier: Apache-2.0
"""Remove activation/result half roundbacks at exactly the Large model's 252 learned body GEMMs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import sys
import time

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
CAMPAIGN=ROOT/"artifacts/v190/final-plan/exec-20261116T052942Z"
PARENT="e6a6227446662e33c47ea006e1c0f6e2710b6434245bb23f4205ace8299d794c"
SCORE="de071de7d3441cc8f00a5b67a033e3e6040ab41649bc2991990281370ae9dbe8"
PANEL="99936c0bb69c7cef9e61f11dd4ad2a78f181b63d2dafdf130088c4b3789734ee"
GRAPH="a9badf775c6b4e2be5dd7ffa015c37d7aeace0c56ec943f55bc1be23e0854157"
SITES={"self_attn/q_proj":("input_layernorm/Mul_1_output_0",2560,4096),
       "self_attn/k_proj":("input_layernorm/Mul_1_output_0",2560,1024),
       "self_attn/v_proj":("input_layernorm/Mul_1_output_0",2560,1024),
       "self_attn/o_proj":("self_attn/Reshape_7_output_0",4096,2560),
       "mlp/gate_proj":("post_attention_layernorm/Mul_1_output_0",2560,9728),
       "mlp/up_proj":("post_attention_layernorm/Mul_1_output_0",2560,9728),
       "mlp/down_proj":("mlp/Mul_output_0",9728,2560)}

def require(ok, message):
    if not ok:
        raise ValueError(message)

def path_nodes(onnx, layer, site, storage):
    root=f"/scorer/layers.{layer}/"
    name=root+site+"/MatMul"
    stem=("/airk_large_qkv_fp32" if site in ("self_attn/q_proj","self_attn/k_proj","self_attn/v_proj") else "/airk_large_body_fp32")+name
    g5="/airk_g5/weight/"+name
    act=root+SITES[site][0]
    def cast(node, source, dest, dtype):
        return onnx.helper.make_node("Cast",[source],[dest],name=node,to=dtype)
    def matmul(left,out):
        return onnx.helper.make_node("MatMul",[left,stem+"/weight32"],[out],name=name)
    w=cast(stem+"/weight32",storage,stem+"/weight32",1)
    old=[cast(g5+"/input16",act,g5+"/input16",10),cast(stem+"/input32",g5+"/input16",stem+"/input32",1),
         w,matmul(stem+"/input32",stem+"/output32"),cast(stem+"/output16",stem+"/output32",g5+"/output16",10),
         cast(g5+"/output32",g5+"/output16",name+"_output_0",1)]
    return old,[w,matmul(act,name+"_output_0")]

def transform(onnx, source):
    require(hashlib.sha256(source.SerializeToString()).hexdigest()==GRAPH,"wrong compensated Large graph")
    nodes={n.name:n for n in source.graph.node}
    parameters={p.name:p for p in source.graph.initializer}
    require(len(nodes)==len(source.graph.node),"duplicate original node names")
    edits={}
    reverse={}
    weights=set()
    for layer in range(36):
        for site,(_,rows,cols) in SITES.items():
            name=f"/scorer/layers.{layer}/{site}/MatMul"
            cast=nodes[nodes[name].input[1]]
            storage=cast.input[0]
            w=parameters[storage]
            ext={v.key:v.value for v in w.external_data}
            require(w.data_type==10 and tuple(w.dims)==(rows,cols) and ext["location"]=="weights.bin"
                    and int(ext["length"])==2*rows*cols and storage not in weights,"original half initializer changed")
            weights.add(storage)
            old,new=path_nodes(onnx,layer,site,storage)
            require(all(nodes.get(n.name)==n for n in old),"original projection boundary differs: "+name)
            for item in old:
                require(item.name not in edits,"overlapping learned projection edges")
                edits[item.name]=None
                reverse[item.name]=item
            for item in new:
                edits[item.name]=item
    def empty(model):
        out=onnx.ModelProto()
        out.CopyFrom(model)
        del out.graph.node[:]
        return out
    result=empty(source)
    for item in source.graph.node:
        replacement=edits.get(item.name,item)
        if replacement is not None:
            result.graph.node.append(replacement)
    recovered=empty(result)
    iterator=iter(result.graph.node)
    for name in nodes:  # The predecessor supplies order only. Reconstitute the removed casts from the route specification.
        if name not in edits:
            actual=next(iterator)
            require(actual.name==name,"unrelated source node moved")
            recovered.graph.node.append(actual)
        else:
            if edits[name] is not None:
                require(next(iterator)==edits[name],"direct learned projection differs")
            recovered.graph.node.append(reverse[name])
    require(len(edits)==1512 and len(source.graph.node)-len(result.graph.node)==1008 and next(iterator,None) is None
            and recovered.SerializeToString()==source.SerializeToString(),"Large inverse changed nonprojection bytes")
    return result

def pin(path):
    path=Path(path)
    with path.open("rb") as f:
        return {"path":str(path),"bytes":path.stat().st_size,"sha256":hashlib.file_digest(f,"sha256").hexdigest()}

def read(path, expected):
    require(pin(path)["sha256"]==expected,"changed "+str(path))
    return json.loads(Path(path).read_bytes())

def stage(out):
    require(out.parent==CAMPAIGN and out.is_dir() and not any(out.iterdir()) and sys.flags.isolated and sys.flags.no_site
            and sys.dont_write_bytecode and os.environ.get("CUDA_VISIBLE_DEVICES")=="","fresh isolated CPU-only stage required")
    prior=CAMPAIGN/"large-cutlass-body-stage-r1/stage.json"
    previous=read(prior,PARENT)
    ep=CAMPAIGN/"windows-large-compensated-score-t32-r1/execution.json"
    evidence=read(ep,SCORE)
    pp=CAMPAIGN/"large-compensated-layer0-roundback-panel-r1/execution.json"
    panel=read(pp,PANEL)
    require(evidence["complete"] and evidence["owned"]["exit_code"]==0 and not evidence["numerical"]["release_numerical_case_pass"]
            and panel["observation"]["actual_final_graph_census"]["large"]["immediate_half_result"]==252,"wrong Large predecessor")
    ref=read(previous["original_reference"]["path"],previous["original_reference"]["sha256"])
    plan=read(ref["conversion_plan"]["path"],ref["conversion_plan"]["sha256"])
    runtime=read(plan["runtime_manifest"]["path"],plan["runtime_manifest"]["sha256"])
    owner_path=ROOT/"scripts/arbiter-fp16-execution.py"
    verifier_path=HERE/"verify_compact.py"
    require(pin(sys.executable)==plan["python"] and pin(owner_path)==plan["wrapper"] and
            pin(verifier_path)["sha256"]=="50d5c7237cd057a1d08ea79794bbdda0f1e141438a2d22580a760e418f9e3d75"
            and pin(previous["graph"]["path"])==previous["graph"] and pin(previous["weights"]["path"])==previous["weights"],"original executable or model assets changed")
    owner=runpy.run_path(str(owner_path))
    verifier=runpy.run_path(str(verifier_path))
    dst=out/"model.onnx"
    owner["guard"](plan,dst,deadline=time.monotonic()+120)
    sys.addaudithook(lambda event,args:owner["output_access"](event,args,out))
    before=set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import onnx
    model=transform(onnx,onnx.load_model(previous["graph"]["path"],load_external_data=False))
    data=model.SerializeToString()
    with dst.open("xb") as f:
        f.write(data)
    owner["windows_owner"](path=dst)
    require(dst.read_bytes()==data,"changed serialized model")
    verifier["loaded_runtime"](runtime["files"],lambda p,_:pin(p),lambda:None,(sys.modules[k] for k in sys.modules.keys()-before))
    return {"schema":"airk.private.large-direct-fp32-stage/1","qualification":False,"prior_stage":pin(prior),"original_score":pin(ep),
            "panel":pin(pp),"original_graph":previous["original_graph"],"original_reference":previous["original_reference"],
            "weights":previous["weights"],"graph":pin(dst),"source":pin(Path(__file__).resolve()),"new_weight_bytes":0,
            "verification":{"projection_count":252,"mha_count":36,"query_heads":32,"kv_heads":8,"group_repeat":4,"head_width":128,
                            "direct_fp32_inputs":252,"direct_fp32_results":252,"removed_casts":1008,"byte_exact_mha_inverse":True}}
if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output",type=Path)
    print(json.dumps(stage(parser.parse_args().output),allow_nan=False))
