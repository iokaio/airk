# SPDX-License-Identifier: Apache-2.0
"""Stage and independently invert the 108 Large protected Q/K/V projection substitutions."""

import argparse
import hashlib
import json
import os
import runpy
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
TREATMENT = "c5e99be11646cc665075ee098d24a539664b1e282caaa7806360fb9a86b75988"
REFERENCE = "c214278ab06a24053d0a309b294e459ec7ba717e99f4e39edd3fedaa26c878bf"
GRAPH = "c00371ad9407c25bd52999d87327115025b7c6f55a4e585645ca112d858de5bd"
WEIGHTS = "5b053f02795e72ff88123b28f89d3f2c89da055e9ed2228bc60809dd0a2b4270"
STEM = "/airk_large_qkv_fp32"
ROUTES = {f"/scorer/layers.{layer}/self_attn/{which}_proj/MatMul": width
          for layer in range(36) for which, width in (("q",4096),("k",1024),("v",1024))}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def pin(path):
    with path.open("rb") as file:
        return {"path": str(path), "bytes": path.stat().st_size, "sha256": hashlib.file_digest(file,"sha256").hexdigest()}


def read(path, sha):
    require(pin(path)["sha256"] == sha, "digest changed: " + path.name)
    return json.loads(path.read_bytes())


def rewrite(onnx, baseline):
    result = onnx.ModelProto()
    result.CopyFrom(baseline)
    nodes = result.graph.node
    del nodes[:]
    tensors = {t.name: t for t in baseline.graph.initializer}
    seen = set()
    for node in baseline.graph.node:
        if node.name not in ROUTES:
            nodes.append(node)
            continue
        require(node.name not in seen and node.domain == "" and node.op_type == "MatMul"
                and len(node.input) == 2 and len(node.output) == 1 and not node.attribute,
                "original Large projection owner differs")
        tensor = tensors[node.input[1]]
        ext = {v.key:v.value for v in tensor.external_data}
        require(tensor.data_type == onnx.TensorProto.FLOAT16 and list(tensor.dims) == [2560, ROUTES[node.name]]
                and ext["location"] == "weights.bin" and int(ext["length"]) == 2*2560*ROUTES[node.name],
                "original Large projection span differs")
        seen.add(node.name)
        stem = STEM + node.name
        for original, suffix in zip(node.input, ("/input32", "/weight32"), strict=True):
            out = stem + suffix
            nodes.append(onnx.helper.make_node("Cast", [original], [out], name=out, to=onnx.TensorProto.FLOAT))
        mm = nodes.add()
        mm.CopyFrom(node)
        mm.input[:] = [stem + "/input32", stem + "/weight32"]
        mm.output[:] = [stem + "/output32"]
        nodes.append(onnx.helper.make_node("Cast", list(mm.output), list(node.output),
                                          name=stem + "/output16", to=onnx.TensorProto.FLOAT16))
    require(seen == ROUTES.keys(), "Large projection roster incomplete")
    return result


def verify(onnx, baseline, candidate):
    inverse = onnx.ModelProto()
    inverse.CopyFrom(candidate)
    del inverse.graph.node[:]
    source = iter(candidate.graph.node)
    actual = []
    for old in baseline.graph.node:
        first = next(source)
        if old.name not in ROUTES:
            inverse.graph.node.append(first)
            continue
        weight, mm, narrow = next(source), next(source), next(source)
        prefix = STEM + old.name
        for node, src, dst, name, dtype in (
                (first, old.input[0], prefix+"/input32", prefix+"/input32", onnx.TensorProto.FLOAT),
                (weight, old.input[1], prefix+"/weight32", prefix+"/weight32", onnx.TensorProto.FLOAT),
                (narrow, prefix+"/output32", old.output[0], prefix+"/output16", onnx.TensorProto.FLOAT16)):
            expected = onnx.helper.make_node("Cast",[src],[dst],name=name,to=dtype)
            require(node == expected,"unexpected protected Cast")
        require(list(mm.input) == [first.output[0], weight.output[0]] and list(mm.output)==list(narrow.input),
                "protected MatMul boundaries differ")
        recovered = inverse.graph.node.add()
        recovered.CopyFrom(mm)
        recovered.input[:] = [first.input[0],weight.input[0]]
        recovered.output[:] = list(narrow.output)
        actual.append(mm.name)
    require(next(source,None) is None and len(actual)==108 and set(actual)==ROUTES.keys()
            and inverse.SerializeToString() == baseline.SerializeToString(), "exact inverse original graph differs")
    return {"projection_count": len(actual), "new_cast_count": 3*len(actual),
            "original_node_count": len(baseline.graph.node), "candidate_node_count": len(candidate.graph.node),
            "byte_exact_inverse": True}


def stage(output):
    require(output.parent == CAMPAIGN and output.is_dir() and not any(output.iterdir()), "fresh campaign stage required")
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode and os.environ.get("CUDA_VISIBLE_DEVICES") == "",
            "CPU-only isolated Python required")
    evidence = CAMPAIGN / "large-fp16-t32-layer0-three-gemm-r1/execution.json"
    obs = read(evidence,TREATMENT)
    require(obs["owned"]["owned_job_final_active_processes"] == obs["owned"]["exit_code"] == 0
            and obs["observation"]["gpu_treatment"]["treatment_supported"] is True, "real Large treatment proof required")
    original_ref = CAMPAIGN / "large-fp16-sm86-flash-independent-t32-r1/reference.json"
    ref = read(original_ref,REFERENCE)
    original = Path(ref["graph"]["path"])
    sidecar = Path(ref["weights"]["path"])
    require(ref["graph"]["sha256"] == GRAPH and ref["weights"]["sha256"] == WEIGHTS
            and ref["graph"] == pin(original) and ref["weights"] == pin(sidecar), "original Large artifact changed")
    plan = read(Path(ref["conversion_plan"]["path"]),ref["conversion_plan"]["sha256"])
    runtime = read(Path(plan["runtime_manifest"]["path"]),plan["runtime_manifest"]["sha256"])
    require(pin(Path(sys.executable)) == plan["python"], "exact interpreter required")
    deadline = time.monotonic() + 120
    owner = ROOT / "scripts/arbiter-fp16-execution.py"
    require(pin(owner) == plan["wrapper"], "approved ownership wrapper required")
    converter = runpy.run_path(str(owner))
    converter["windows_owner"](path=output)
    verifier_path = ROOT / "scripts/arbiter-fp16-execution/verify_compact.py"
    require(pin(verifier_path)["sha256"] == "50d5c7237cd057a1d08ea79794bbdda0f1e141438a2d22580a760e418f9e3d75",
            "sealed import verifier differs")
    verifier = runpy.run_path(str(verifier_path))
    graph_path = output / "model.onnx"
    converter["guard"](plan,graph_path,deadline=deadline)
    sys.addaudithook(lambda event,args: converter["output_access"](event,args,output))
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import onnx
    baseline = onnx.load_model(original, load_external_data=False)
    candidate = rewrite(onnx,baseline)
    result = verify(onnx,baseline,candidate)
    with graph_path.open("xb") as file:
        file.write(candidate.SerializeToString())
    converter["windows_owner"](path=graph_path)
    require(verify(onnx,baseline,onnx.load_model(graph_path,load_external_data=False)) == result,
            "written candidate differs")
    imported = verifier["loaded_runtime"](runtime["files"],lambda p, _: pin(p),lambda: None,
                                          (sys.modules[k] for k in sys.modules.keys()-before))
    return {"imported_runtime_modules":len(imported),"schema":"airk.private.large-qkv-fp32-engineering-stage/1","qualification":False,
            "original_graph":ref["graph"],"original_reference":pin(original_ref),"treatment":pin(evidence),
            "graph":pin(graph_path),"weights":ref["weights"],"verification":result,
            "logical_external_name":"weights.bin","new_weight_bytes":0,"source":pin(Path(__file__).resolve())}


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output",type=Path)
    print(json.dumps(stage(parser.parse_args().output),allow_nan=False))
