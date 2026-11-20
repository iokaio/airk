# SPDX-License-Identifier: Apache-2.0
"""Lower the authenticated 36-layer Large graph to causal FP32 MHA with four query heads per KV head."""
import argparse
import hashlib
import json
import os
import runpy
import sys
import time
from pathlib import Path

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
CAMPAIGN=ROOT/"artifacts/v190/final-plan/exec-20261116T052942Z"
PARENT="a179d41c0dd4fe89ea489e7325365b63aa4825ffd0ac8dc52e17b46fcc3c973b"
GRAPH="636cea20b5fa4f58251589d01a7ef2e3df87f7f4b57498d2765b0d56f6b87d2d"
SCORE="ecc918a0b955d510ba543d0604b470a3781d534b001f0df719ce9bd4f9e5670b"
PREFIX="/airk_large_cutlass/"
SHAPES={"split_kv":[0,0,8,1,128],"repeat_group":[1,1,1,4,1],"flat_heads":[0,0,4096]}

def require(ok,message):
    if not ok:
        raise ValueError(message)

def replacements(onnx,layer):
    n=onnx.helper.make_node
    a=f"/airk_g5/attention/{layer}/"
    out=f"/scorer/layers.{layer}/self_attn/"
    q,k,v=a+"q_flat",a+"k_flat",out+"v_proj/MatMul_output_0"
    old=[n("Cast",[v],[a+"v16"],name=a+"v16",to=10),
         n("Cast",[q],[a+"q16"],name=a+"q16",to=10),
         n("Cast",[k],[a+"k16"],name=a+"k16",to=10),
         n("GroupQueryAttention",[a+"q16",a+"k16",a+"v16","","","/airk_g5/seqlens32","/airk_g5/T32"],
           [a+"raw16",a+"present_key",a+"present_value"],name=a+"efficient",domain="com.microsoft",
           causal=1,do_rotary=0,kv_num_heads=8,local_window_size=-1,num_heads=32,scale=128**-0.5),
         n("Cast",[a+"raw16"],[out+"Reshape_7_output_0"],name=a+"output32",to=1)]
    pre=PREFIX+str(layer)+"/"
    new=[]
    for s,x in (("k",k),("v",v)):
        head=pre+s
        new.extend([n("Reshape",[x,PREFIX+"split_kv"],[head+"_split"],name=head+"_split"),
                    n("Tile",[head+"_split",PREFIX+"repeat_group"],[head+"_group4"],name=head+"_group4"),
                    n("Reshape",[head+"_group4",PREFIX+"flat_heads"],[head+"_flat"],name=head+"_flat")])
    new.append(n("MultiHeadAttention",[q,pre+"k_flat",pre+"v_flat"],[out+"Reshape_7_output_0"],
                 name=pre+"efficient",domain="com.microsoft",num_heads=32,unidirectional=1,scale=128**-0.5))
    return old,new

def transform(onnx,source):
    require(hashlib.sha256(source.SerializeToString()).hexdigest()==GRAPH,"wrong original Large graph")
    original={n.name:n for n in source.graph.node}
    require(len(original)==len(source.graph.node),"ambiguous original node names")
    edits={}
    removed=set()
    for layer in range(36):
        old,new=replacements(onnx,layer)
        for n in old:
            require(original[n.name]==n,"original typed attention wiring differs: "+n.name)
            edits[n.name]=[]
        for n in old[:-1]:
            removed.update(n.output)
        edits[old[-2].name]=new
    candidate=onnx.ModelProto()
    candidate.CopyFrom(source)
    del candidate.graph.node[:]
    for n in source.graph.node:
        candidate.graph.node.extend(edits.get(n.name,[n]))
    added=[]
    for name,values in SHAPES.items():
        require(all(t.name!=PREFIX+name for t in source.graph.initializer),"occupied shape initializer")
        t=onnx.helper.make_tensor(PREFIX+name,onnx.TensorProto.INT64,[len(values)],values)
        added.append(t)
        candidate.graph.initializer.append(t)
    require(not any(v in removed for n in candidate.graph.node for v in n.input),"removed half operand still consumed")
    recovered=onnx.ModelProto()
    recovered.CopyFrom(candidate)
    require(list(recovered.graph.initializer[-3:])==added,"grouped head ordering or shape differs")
    del recovered.graph.initializer[-3:]
    del recovered.graph.node[:]
    actual=iter(candidate.graph.node)
    for n in source.graph.node:
        for expected in edits.get(n.name,[n]):
            require(next(actual,None)==expected,"modified Large projection, geometry or MHA attributes")
        recovered.graph.node.append(n)
    require(next(actual,None) is None and recovered.SerializeToString()==source.SerializeToString(),"inverse graph failed")
    return candidate

def stage(out):
    def pin(p):
        p=Path(p)
        with p.open("rb") as f:
            return {"path":str(p),"bytes":p.stat().st_size,"sha256":hashlib.file_digest(f,"sha256").hexdigest()}
    def read(p,sha):
        require(pin(p)["sha256"]==sha,"changed "+str(p))
        return json.loads(Path(p).read_bytes())
    require(out.parent==CAMPAIGN and out.is_dir() and not any(out.iterdir()) and sys.flags.isolated
            and sys.flags.no_site and sys.dont_write_bytecode and os.environ.get("CUDA_VISIBLE_DEVICES")=="",
            "fresh isolated CPU-only campaign output required")
    prior=CAMPAIGN/"large-fp32-body-stage-r1/stage.json"
    original=read(prior,PARENT)
    evidence=CAMPAIGN/"windows-large-body-score-t32-r1/execution.json"
    original_score=read(evidence,SCORE)
    require(original_score["complete"] and not original_score["release_matching_raw_pass"]
            and original_score["owned"]["owned_job_final_active_processes"]==0
            and original["verification"]["projection_count"]==252,"expected authentic Large predecessor")
    ref=read(original["original_reference"]["path"],original["original_reference"]["sha256"])
    plan=read(ref["conversion_plan"]["path"],ref["conversion_plan"]["sha256"])
    runtime=read(plan["runtime_manifest"]["path"],plan["runtime_manifest"]["sha256"])
    owner_path=ROOT/"scripts/arbiter-fp16-execution.py"
    verifier_path=HERE/"verify_compact.py"
    require(pin(sys.executable)==plan["python"] and pin(owner_path)==plan["wrapper"] and
            pin(verifier_path)["sha256"]=="50d5c7237cd057a1d08ea79794bbdda0f1e141438a2d22580a760e418f9e3d75"
            and pin(original["weights"]["path"])==original["weights"] and pin(original["graph"]["path"])==original["graph"],
            "original interpreter, source or weight closure differs")
    owner=runpy.run_path(str(owner_path))
    verifier=runpy.run_path(str(verifier_path))
    dst=out/"model.onnx"
    owner["guard"](plan,dst,deadline=time.monotonic()+120)
    sys.addaudithook(lambda event,args:owner["output_access"](event,args,out))
    before=set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import onnx
    baseline=onnx.load_model(original["graph"]["path"],load_external_data=False)
    data=transform(onnx,baseline).SerializeToString()
    with dst.open("xb") as f:
        f.write(data)
    owner["windows_owner"](path=dst)
    require(dst.read_bytes()==data,"serialized Large graph differs")
    imports=verifier["loaded_runtime"](runtime["files"],lambda p,_:pin(p),lambda:None,
                                      (sys.modules[k] for k in sys.modules.keys()-before))
    return {"schema":"airk.private.large-fp32-causal-mha-stage/1","qualification":False,"prior_stage":pin(prior),
            "original_score":pin(evidence),"original_graph":original["original_graph"],"original_reference":original["original_reference"],
            "graph":pin(dst),"weights":original["weights"],"source":pin(Path(__file__).resolve()),
            "verification":{"projection_count":252,"mha_count":36,"query_heads":32,"kv_heads":8,"group_repeat":4,
                            "head_width":128,"byte_exact_body_inverse":True},"new_weight_bytes":0,"imported_modules":len(imports)}

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("output",type=Path)
    print(json.dumps(stage(p.parse_args().output),allow_nan=False))
