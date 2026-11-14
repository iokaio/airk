# SPDX-License-Identifier: Apache-2.0
"""Stream exact ONNX external tensor bytes into one authenticated asset."""
import argparse
import hashlib
import json
from pathlib import Path
import time

ROOT=Path(__file__).resolve().parents[2]


def digest(path):
    result=hashlib.sha256()
    with Path(path).open("rb") as stream:
        while chunk:=stream.read(4194304):result.update(chunk)
    return result.hexdigest()


def tensors(message, tensor_type):
    if isinstance(message,tensor_type):
        yield message
        return
    for field,value in message.ListFields():
        if field.message_type is None:continue
        if field.is_repeated:
            for child in value:yield from tensors(child,tensor_type)
        else:yield from tensors(value,tensor_type)


def main():
    import numpy as np
    import onnx
    import onnxruntime as ort
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args();destination=args.output.resolve();source=args.input.resolve()
    if destination.exists() or not destination.is_relative_to(ROOT/"artifacts"):parser.error("fresh private destination required")
    runner=Path(__file__).read_bytes();graph_bytes=(source/"model.onnx").read_bytes();model=onnx.load_model_from_string(graph_bytes)
    destination.mkdir(parents=True);started=time.monotonic();seals={};mapping=[];offset=0
    data_path=destination/"weights.data"
    with data_path.open("xb") as sink:
        for tensor in tensors(model,onnx.TensorProto):
            if tensor.data_location!=onnx.TensorProto.EXTERNAL:continue
            props={row.key:row.value for row in tensor.external_data}
            name=props["location"]
            if Path(name).name!=name or name in (".","..") or Path(name).is_absolute():raise ValueError("nonlocal tensor asset")
            path=source/name
            if path.is_symlink() or not path.is_file():raise ValueError("unsafe tensor asset")
            before=digest(path);begin=int(props.get("offset","0"));length=int(props.get("length",str(path.stat().st_size-begin)))
            if begin<0 or length<0 or begin+length>path.stat().st_size:raise ValueError("external range out of bounds")
            with path.open("rb") as stream:
                stream.seek(begin);remaining=length;part_hash=hashlib.sha256()
                while remaining:
                    chunk=stream.read(min(4194304,remaining))
                    if not chunk:raise ValueError("external data truncated")
                    remaining-=len(chunk);sink.write(chunk);part_hash.update(chunk)
            if digest(path)!=before:raise ValueError("external tensor changed while copying")
            seals[str(path)]=before
            tensor.ClearField("external_data")
            for key,value in (("location","weights.data"),("offset",str(offset)),("length",str(length))):
                row=tensor.external_data.add();row.key=key;row.value=value
            mapping.append({"tensor":tensor.name,"source":name,"source_offset":begin,"offset":offset,"length":length,"range_sha256":part_hash.hexdigest()})
            offset+=length
    onnx.save_model(model,str(destination/"model.onnx"))
    reference_bytes=(source/"reference.npz").read_bytes();(destination/"reference.npz").write_bytes(reference_bytes)
    reference=np.load(destination/"reference.npz")
    options=ort.SessionOptions();options.intra_op_num_threads=1;options.inter_op_num_threads=1;options.enable_cpu_mem_arena=False
    session=ort.InferenceSession(str(destination/"model.onnx"),options,providers=["CPUExecutionProvider"])
    actual=session.run(None,{row.name:reference[row.name] for row in session.get_inputs()})
    errors=[float(np.max(np.abs(actual[0]-reference["actual_scores"]))),float(np.max(np.abs(actual[1]-reference["actual_hidden"])))]
    if max(errors)!=0:raise ValueError("byte consolidation changed runtime values")
    if (source/"model.onnx").read_bytes()!=graph_bytes or (source/"reference.npz").read_bytes()!=reference_bytes or Path(__file__).read_bytes()!=runner:
        raise ValueError("source drift")
    if any(digest(p)!=sha for p,sha in seals.items()):raise ValueError("source initializer drift")
    result={"schema":"airk.development-model-consolidation/1","source_graph_sha256":hashlib.sha256(graph_bytes).hexdigest(),
        "source_sha256":hashlib.sha256(runner).hexdigest(),"source_external_seals":seals,"mapping":mapping,
        "assets":[{"name":p.name,"bytes":p.stat().st_size,"sha256":digest(p)} for p in (destination/"model.onnx",data_path)],
        "original_onnx_max_errors":errors,"elapsed_seconds":time.monotonic()-started,
        "abi":{"inputs":[{"name":r.name,"type":r.type,"shape":r.shape} for r in session.get_inputs()],"outputs":[{"name":r.name,"type":r.type,"shape":r.shape} for r in session.get_outputs()]},
        "scope":"Exact external-byte consolidation and same-input CPU numerical preservation; no new model quality or additional-shape claim"}
    (destination/"runner.py").write_bytes(runner);(destination/"receipt.json").write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps({"external_tensors":len(mapping),"assets":result["assets"],"maximum_errors":errors,"elapsed_seconds":result["elapsed_seconds"]}),flush=True)


if __name__=="__main__":main()
