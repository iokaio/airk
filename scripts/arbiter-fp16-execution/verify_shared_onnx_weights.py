# SPDX-License-Identifier: Apache-2.0
"""Exercise ONNX Runtime's sibling-graph external-weight resolution on a literal CPU control."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT/"artifacts/v190/final-plan/exec-20261116T052942Z"
RUNTIME_SHA = "a557be764ebcf8a2b777403ac349f2d3ecadfabb5d65e16b8c47026239644c95"


def run(output):
    runtime=(CAMPAIGN/"compact-fp16-present-conversion-plan-r2/compact-runtime.json").read_bytes()
    if hashlib.sha256(runtime).hexdigest()!=RUNTIME_SHA or output.parent!=CAMPAIGN or output!=output.resolve():
        raise ValueError("invalid runtime or output")
    runtime=json.loads(runtime)
    sys.path.extend(runtime["import_paths"])
    import numpy as np
    import onnx
    import onnxruntime as ort
    for module in (onnx,ort,np):
        path=Path(module.__file__).resolve()
        if runtime["files"][str(path)]["sha256"]!=hashlib.sha256(path.read_bytes()).hexdigest():
            raise ValueError("package source differs")
    weight=output/"weights.bin"
    with weight.open("xb") as writer:
        writer.write(struct.pack("<f",3.0))
    results={}
    for operation,expected in (("Add",5.0),("Mul",6.0)):
        init=onnx.TensorProto(name="w",data_type=onnx.TensorProto.FLOAT,dims=[1],data_location=onnx.TensorProto.EXTERNAL)
        for key,value in (("location","weights.bin"),("offset","0"),("length","4")):
            init.external_data.add(key=key,value=value)
        value=lambda name: onnx.helper.make_tensor_value_info(name,onnx.TensorProto.FLOAT,[1])
        graph=onnx.helper.make_graph([onnx.helper.make_node(operation,["x","w"],["y"])],operation,[value("x")],[value("y")],[init])
        model=onnx.helper.make_model(graph,ir_version=8,opset_imports=[onnx.helper.make_opsetid("",17)])
        name=output/(operation+".onnx")
        with name.open("xb") as writer:
            writer.write(model.SerializeToString())
        options=ort.SessionOptions()
        options.intra_op_num_threads=1
        options.inter_op_num_threads=1
        session=ort.InferenceSession(str(name),sess_options=options,providers=["CPUExecutionProvider"])
        result=session.run(["y"],{"x":np.asarray([2.0],dtype=np.float32)})[0].tolist()
        if result != [expected]:
            raise ValueError("wrong sibling graph or external weight")
        results[name.name]={"sha256":hashlib.sha256(name.read_bytes()).hexdigest(),"returned":result}
        del session
    receipt={"schema":"airk.private.onnx-shared-external-control/1","runtime":ort.__version__,"provider":"CPUExecutionProvider",
             "outputs":results,"sidecar_sha256":hashlib.sha256(weight.read_bytes()).hexdigest(),"logical_sidecar_bytes":weight.stat().st_size,
             "script_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),"product_qualification":False}
    with (output/"execution.json").open("x") as stream:
        json.dump(receipt,stream,allow_nan=False)


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output",type=Path)
    args=parser.parse_args()
    if not(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode):
        raise ValueError("use the pinned isolated Python with -I -B -S")
    run(args.output)
