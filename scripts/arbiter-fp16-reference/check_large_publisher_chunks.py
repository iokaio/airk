# SPDX-License-Identifier: Apache-2.0
"""Compare chunked BF16 promotion against stock safetensors for two tiny shards."""
import argparse
import json
from pathlib import Path
import runpy
import sys

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--site", type=Path, required=True)
    parser.add_argument("--scratch", type=Path, required=True)
    args = parser.parse_args()
    if not sys.flags.isolated or not sys.dont_write_bytecode or not args.scratch.is_dir() or any(args.scratch.iterdir()):
        raise ValueError("isolated original runtime and fresh guarded D scratch required")
    sys.path.append(str(args.site.resolve(strict=True)))
    import torch
    from safetensors.torch import load_file, save_file
    torch.set_num_threads(2)
    load = runpy.run_path(str(Path(__file__).with_name("large_train_case.py")))["load_publisher_state"]
    original = {"model.a":torch.arange(513,dtype=torch.float32).reshape(3,171).bfloat16(),
                "model.b":torch.tensor([-27.5,-0.0,0.0,1.0078125],dtype=torch.bfloat16),
                "model.c":torch.arange(66,dtype=torch.bfloat16)}
    index = {}
    expected = {}
    for name, keys in (("one.safetensors",("model.b","model.a")),("two.safetensors",("model.c",))):
        path = args.scratch/name
        save_file({k:original[k] for k in keys},path)
        expected.update(load_file(path))
        index.update({k:name for k in keys})
    phases = []
    result = load(torch,args.scratch,index,lambda:None,lambda *row:phases.append(row),chunk_bytes=14)
    if len(phases)!=4 or set(result)!={k.removeprefix("model.") for k in expected}:
        raise ValueError("two exact shard transitions or original key roster missing")
    for key, source in expected.items():
        value = result[key.removeprefix("model.")]
        if value.shape!=source.shape or not torch.equal(value.view(torch.int32),source.float().view(torch.int32)):
            raise ValueError("stock FP32 tensor words differ")
    file = args.scratch/"two.safetensors"
    raw = file.read_bytes()
    size = int.from_bytes(raw[:8],"little")
    header=json.loads(raw[8:8+size])
    header["model.c"]["data_offsets"][0]=2
    encoded=json.dumps(header).encode()
    corrupt = args.scratch/"invalid.safetensors"
    corrupt.write_bytes(len(encoded).to_bytes(8,"little")+encoded+raw[8+size:])
    index["model.c"] = corrupt.name
    try:
        load(torch,args.scratch,index,lambda:None,lambda *_:None,chunk_bytes=14)
    except ValueError as error:
        if "extent" not in str(error):
            raise
    else:
        raise ValueError("corrupted extent was accepted")
    print("3 tensors across 2 shards matched stock FP32 words; corrupted extent rejected")
    owner_source = Path(__file__).resolve().parents[2]/"scripts/arbiter/export/qwen/qwen3-challengers-r1/qwen3_reference_r2.py"
    owner = runpy.run_path(str(owner_source))
    owner["freeze_math"]()
    config = owner["tiny_config"]()
    make = owner["make_wrapper"]
    random_model = make(config)
    original = {name: tensor.bfloat16().contiguous() for name,tensor in random_model.text.state_dict().items()}
    source = args.scratch/"tiny.safetensors"
    save_file({"model."+k:v for k,v in original.items()},source)
    full = make(config,{k:v.float() for k,v in original.items()})
    trainer = runpy.run_path(str(Path(__file__).with_name("large_train_case.py")))
    lazy, trace = trainer["streamed_official_model"](torch,make,config,args.scratch,
        {"model."+k:source.name for k in original},lambda:None,lambda *_:None)
    for width in (32,47):
        ids = torch.arange(width).unsqueeze(0)
        inputs = (ids,torch.ones_like(ids),ids,torch.tensor([width-1]),torch.tensor([[2,7,9]]),torch.tensor([[True,True,True]]))
        with torch.inference_mode():
            expected_outputs = full(*inputs)
            actual_outputs = lazy(*inputs)
        for first,second in zip(expected_outputs,actual_outputs,strict=True):
            if not torch.equal(first.view(torch.int32),second.view(torch.int32)):
                raise ValueError("stock and streamed tiny official scores or hidden words differ")
    if trace != [0,1,0,1]:
        raise ValueError("tiny streamed decoder chronology differs")
    print("two official tiny-model forwards matched fully resident raw scores and full hidden words")

if __name__=="__main__":
    main()
