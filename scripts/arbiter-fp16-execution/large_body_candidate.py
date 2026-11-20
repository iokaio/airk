# SPDX-License-Identifier: Apache-2.0
"""Protect all 252 Large body products with FP32 accumulation and retain their half boundaries."""
import argparse
import json
import os
import runpy
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
QKV_STAGE = "0ae64faa17aecc33642c3245b3bf55b6f44a961de50128114ecf5ad8442e295c"
MEASUREMENT = "146a2679193462058ac34b01b7c770204c75b2580a6889005425607d2807bcae"
SIZES = {"self_attn/o_proj":(4096,2560), "mlp/gate_proj":(2560,9728),
         "mlp/up_proj":(2560,9728), "mlp/down_proj":(9728,2560)}
SITES = {f"/scorer/layers.{layer}/{site}/MatMul":sizes for layer in range(36) for site,sizes in SIZES.items()}
PREFIX = "/airk_large_body_fp32"

def check(ok, message):
    if not ok:
        raise ValueError(message)

def lowered(onnx, original):
    stem = PREFIX + original.name
    cast = onnx.helper.make_node
    left, weight = original.input
    out, = original.output
    return [cast("Cast",[left],[stem+"/input32"],name=stem+"/input32",to=1),
            cast("Cast",[weight],[stem+"/weight32"],name=stem+"/weight32",to=1),
            cast("MatMul",[stem+"/input32",stem+"/weight32"],[stem+"/output32"],name=original.name),
            cast("Cast",[stem+"/output32"],[out],name=stem+"/output16",to=10)]

def transform(onnx, source):
    result = onnx.ModelProto()
    result.CopyFrom(source)
    del result.graph.node[:]
    tensors = {v.name:v for v in source.graph.initializer}
    names = set()
    weights = set()
    for n in source.graph.node:
        check(not any(PREFIX in v for v in (n.name,*n.input,*n.output)), "occupied body graph namespace")
        if n.name not in SITES:
            result.graph.node.append(n)
            continue
        check(n.name not in names and not n.domain and n.op_type == "MatMul" and not n.attribute
              and len(n.input)==2 and len(n.output)==1, "original Large body MatMul differs")
        t = tensors[n.input[1]]
        ext = {v.key:v.value for v in t.external_data}
        a,b = SITES[n.name]
        check(t.name not in weights and t.data_type == 10 and t.data_location == onnx.TensorProto.EXTERNAL
              and tuple(t.dims)==(a,b) and ext["location"]=="weights.bin" and int(ext["length"])==2*a*b,
              "original Large half projection storage differs")
        weights.add(t.name)
        names.add(n.name)
        result.graph.node.extend(lowered(onnx,n))
    check(names==SITES.keys() and len(weights)==144, "incomplete or aliased Large body projections")
    return result

def verify(onnx, baseline, candidate):
    recovered = onnx.ModelProto()
    recovered.CopyFrom(candidate)
    del recovered.graph.node[:]
    nodes = iter(candidate.graph.node)
    for orig in baseline.graph.node:
        current = next(nodes)
        if orig.name not in SITES:
            recovered.graph.node.append(current)
            continue
        weight, mm, narrow = next(nodes),next(nodes),next(nodes)
        check([current,weight,mm,narrow]==lowered(onnx,orig), "wrong typed widening or narrowing")
        old = recovered.graph.node.add()
        old.CopyFrom(mm)
        old.input[:] = [current.input[0],weight.input[0]]
        old.output[:] = narrow.output
    check(next(nodes,None) is None and recovered.SerializeToString()==baseline.SerializeToString(),
          "candidate cannot reconstruct the entire protected QKV graph exactly")
    return {"projection_count":252,"previously_protected":108,"additional_projections":144,
            "new_cast_count":432,"retained_half_boundaries":True,"unchanged_flash_layers":36,
            "byte_exact_qkv_inverse":True}

def stage(out):
    import hashlib
    def pin(path):
        with Path(path).open("rb") as f:
            return {"path":str(path),"bytes":Path(path).stat().st_size,"sha256":hashlib.file_digest(f,"sha256").hexdigest()}
    def read(path,sha):
        check(pin(path)["sha256"]==sha,"changed "+str(path))
        return json.loads(Path(path).read_bytes())
    check(out.parent==CAMPAIGN and out.is_dir() and not any(out.iterdir()) and sys.flags.isolated
          and sys.flags.no_site and sys.dont_write_bytecode and os.environ.get("CUDA_VISIBLE_DEVICES")=="",
          "fresh owned campaign output and offline isolated CPU interpreter required")
    prior = CAMPAIGN/"large-fp32-qkv-stage-r1/stage.json"
    measured = CAMPAIGN/"large-fp32-qkv-layer17-same-operand-r1/execution.json"
    qkv = read(prior,QKV_STAGE)
    replay = read(measured,MEASUREMENT)
    check(replay["owned"]["exit_code"] == replay["owned"]["owned_job_final_active_processes"] == 0
          and qkv["verification"]["projection_count"] == 108,"settled Large evidence required")
    ref = read(qkv["original_reference"]["path"],qkv["original_reference"]["sha256"])
    plan = read(ref["conversion_plan"]["path"],ref["conversion_plan"]["sha256"])
    runtime = read(plan["runtime_manifest"]["path"],plan["runtime_manifest"]["sha256"])
    check(pin(sys.executable)==plan["python"] and pin(qkv["weights"]["path"])==qkv["weights"]
          and pin(qkv["graph"]["path"])==qkv["graph"],"exact original interpreter, graph and weights required")
    owner_path=ROOT/"scripts/arbiter-fp16-execution.py"
    verifier_path=HERE/"verify_compact.py"
    check(pin(owner_path)==plan["wrapper"] and pin(verifier_path)["sha256"]==
          "50d5c7237cd057a1d08ea79794bbdda0f1e141438a2d22580a760e418f9e3d75","ownership or import verifier differs")
    owner=runpy.run_path(str(owner_path))
    verifier=runpy.run_path(str(verifier_path))
    graph=out/"model.onnx"
    owner["guard"](plan,graph,deadline=time.monotonic()+120)
    sys.addaudithook(lambda event,args: owner["output_access"](event,args,out))
    before=set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import onnx
    original=onnx.load_model(qkv["graph"]["path"],load_external_data=False)
    candidate=transform(onnx,original)
    details=verify(onnx,original,candidate)
    with graph.open("xb") as f:
        f.write(candidate.SerializeToString())
    owner["windows_owner"](path=graph)
    check(verify(onnx,original,onnx.load_model(graph,load_external_data=False))==details,"serialized graph differs")
    imports=verifier["loaded_runtime"](runtime["files"],lambda p,_:pin(p),lambda:None,
                                      (sys.modules[k] for k in sys.modules.keys()-before))
    return {"schema":"airk.private.large-body-fp32-engineering-stage/1","qualification":False,
            "prior_stage":pin(prior),"treatment_evidence":pin(measured),"original_graph":qkv["original_graph"],
            "original_reference":qkv["original_reference"],"graph":pin(graph),"weights":qkv["weights"],
            "new_weight_bytes":0,"verification":details,"imported_runtime_modules":len(imports),
            "logical_external_name":"weights.bin","source":pin(Path(__file__).resolve())}

if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output",type=Path)
    print(json.dumps(stage(parser.parse_args().output),allow_nan=False))
