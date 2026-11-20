# SPDX-License-Identifier: Apache-2.0
"""Bind the previously selected real TRAIN Q543 row, its sparse readout and official Large FP32 control."""
import hashlib
import json
import math
import runpy
from pathlib import Path

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
INPUT="af125f4c7ef1cd3776ea0cf7308743724a958744a921559746d907d0278868a4"
RECORD="50ed40cec62a39b0067515b9aab2cd97c2849ef547dad149c49d86a32a863912"
CAPTURE="0eb6a5b4f433e9022179a1c950095f1466e9d8ca0b111ecd63ea2a0d7eeaaa47"
PROFILE="d0e3c74813347b9bb4d3e17ffa4e337bbd16574ca80a81e4b1504c7e1546b307"
PUBLISHER="4c6f43d0d81539cfc6bf7b6ddf5ae69273e61508d0db3d22a5e7488c60560b6b"

def require(ok,reason):
    if not ok:
        raise ValueError(reason)

def pinned(path,sha,expected_bytes):
    path=path.absolute()
    require(path.resolve(strict=True)==path and not any(p.is_symlink() or p.is_junction() for p in (path,*path.parents))
            and path.stat().st_size==expected_bytes,"wrong original path or length: "+path.name)
    with path.open("rb") as source:
        raw=source.read(expected_bytes+1)
    require(len(raw)==expected_bytes,"original case grew or truncated: "+path.name)
    require(hashlib.sha256(raw).hexdigest()==sha,"changed original "+path.name)
    return json.loads(raw),{"path":str(path),"bytes":len(raw),"sha256":sha}

def original(campaign):
    root=campaign/"compact-development-TRAIN_Q543-focus1024-run-r1"
    cap,cp=pinned(root/"capture.private.json",CAPTURE,1624)
    r,rp=pinned(root/"fixture/context-cal-admissions/record-0001.json",RECORD,204733)
    require(cap["case_id"]=="TRAIN_Q543" and cap["judged"] is False and r["Context_NN_dispatch"] is False
            and r["ordinal_in_private_run"]==1 and r["request"]["remaining_input_tokens"]==4096 and
            r["remaining_ns_at_observation"]==100253589300,"original TRAIN development or allowance differs")
    rows=[v for v in r["compiler_census"] if v["profile"]==PROFILE and v["model"]=="arbiter-context-qwen3-4b"]
    require(len(rows)==1 and rows[0]["NN_invocations"]==0 and rows[0]["input_tokens"]==1588 and rows[0]["fits_remaining_allowance"],
            "original Large compiler entry missing")
    x=rows[0]["compiled"]
    require(x["input_digest"]==INPUT and len(x["input_ids"])==1588 and x["attention_mask"]==[1]*1588 and
            x["position_ids"]==list(range(1588)) and x["decision_position"]==1587 and
            x["candidate_positions"]==list(range(32,39)) and x["option_mask"]==[True]*7 and len(r["request"]["roster"]["cards"])==7,
            "wrong compiled native Large row")
    return {"name":"TRAIN_Q543","input":x,"roster":r["request"]["roster"],"remaining_ns":r["remaining_ns_at_observation"],
            "remaining_input_tokens":r["request"]["remaining_input_tokens"],"pins":[cp,rp]}

def score_publisher(torch, model, row):
    i64=torch.int64
    args=[torch.tensor([row[key]],dtype=i64) for key in ("input_ids","attention_mask","position_ids")]
    args.extend([torch.tensor([row["decision_position"]],dtype=i64),torch.tensor([row["candidate_positions"]],dtype=i64),
                 torch.tensor([row["option_mask"]],dtype=torch.bool)])
    with torch.inference_mode():
        scores,hidden=model(*args)
    require(hidden.device.type=="cpu" and tuple(hidden.shape)==(1,len(row["input_ids"]),2560),
            "official Large publisher hidden output differs")
    require(scores.device.type=="cpu" and scores.dtype==torch.float32 and tuple(scores.shape)==(1,len(row["option_mask"])),
            "official Large publisher sparse output differs")
    values=scores[0].tolist()
    require(all(math.isfinite(v) for v in values),"nonfinite publisher output")
    return values

def load_publisher_state(torch, folder, index, check, phase, chunk_bytes=4 << 20, *, names=None, shape_only=False):
    """Promote authenticated BF16 tensor payloads without a whole-shard memory mapping."""
    require(type(chunk_bytes) is int and chunk_bytes > 0 and chunk_bytes % 2 == 0, "aligned BF16 chunks required")
    state = {}
    poison = torch.tensor(float("nan"), dtype=torch.float32) if shape_only else None
    buffer = bytearray(chunk_bytes)
    view = memoryview(buffer)
    for shard in sorted(set(index.values())):
        require(Path(shard).name == shard, "unexpected publisher shard path")
        path = folder / shard
        expected = {name for name, source in index.items() if source == shard}
        phase("open_publisher_shard", shard)
        with path.open("rb") as stream:
            header_size = int.from_bytes(stream.read(8), "little")
            require(0 < header_size <= 1 << 20, "publisher header length differs")
            header = json.loads(stream.read(header_size))
            header.pop("__metadata__", None)
            require(set(header) == expected, "publisher header/index mismatch")
            cursor = 0
            for name, item in sorted(header.items(), key=lambda entry: entry[1]["data_offsets"][0]):
                shape = item["shape"]
                count = math.prod(shape)
                begin, end = item["data_offsets"]
                require(name.startswith("model.") and item["dtype"] == "BF16"
                        and shape and all(type(n) is int and n > 0 for n in shape)
                        and begin == cursor and end - begin == 2 * count
                        and end <= path.stat().st_size - 8 - header_size, "invalid original BF16 tensor extent")
                key = name.removeprefix("model.")
                if shape_only or (names is not None and key not in names):
                    stream.seek(end-begin, 1)
                    if shape_only:
                        state[key] = poison.expand(shape)
                    cursor = end
                    continue
                tensor = torch.empty(count, dtype=torch.float32, device="cpu")
                for offset in range(0, end - begin, chunk_bytes):
                    check()
                    size = min(chunk_bytes, end - begin - offset)
                    require(stream.readinto(view[:size]) == size, "incomplete original BF16 tensor")
                    half_words = torch.frombuffer(view[:size], dtype=torch.bfloat16)
                    tensor[offset // 2:(offset + size) // 2].copy_(half_words)
                state[name.removeprefix("model.")] = tensor.reshape(shape)
                cursor = end
            require(stream.read(1) == b"", "publisher shard trailing data")
        phase("consumed_publisher_shard", shard)
    if names is not None:
        require(set(state)==names, "unknown requested publisher state")
    return state


def streamed_official_model(torch, make_wrapper, config, folder, index, check, phase):
    """Run the official module with only its active decoder layer materialized."""
    def load(names=None, shape_only=False):
        return load_publisher_state(torch,folder,index,check,lambda *_:None,names=names,shape_only=shape_only)

    placeholders = load(shape_only=True)
    fixed = {name for name in placeholders if not name.startswith("layers.")}
    require(fixed == {"embed_tokens.weight", "norm.weight"}, "unreviewed persistent publisher parameters")
    state = dict(placeholders)
    state.update(load(fixed))
    model = make_wrapper(config, state)
    del state
    layers = model.text.layers
    require(len(layers) == config["num_hidden_layers"], "official decoder population differs")
    trace = []
    active = None
    def attach(i, module):
        prefix = f"layers.{i}."
        selected = {name for name in placeholders if name.startswith(prefix)}
        empty = {name.removeprefix(prefix): placeholders[name] for name in selected}
        require(set(empty) == set(module.state_dict()) and selected, "publisher decoder state roster differs")

        def before(layer, args):
            nonlocal active
            require(active is None and len(trace) % len(layers) == i, "publisher decoder overlap or order differs")
            actual = {name.removeprefix(prefix): v for name,v in load(selected).items()}
            layer.load_state_dict(actual, strict=True, assign=True)
            active = i

        def after(layer, args, output):
            nonlocal active
            require(active == i, "original decoder did not return")
            layer.load_state_dict(empty, strict=True, assign=True)
            trace.append(i)
            active = None
            phase("official_decoder_returned", i)

        module.register_forward_pre_hook(before)
        module.register_forward_hook(after)
    for i, layer in enumerate(layers):
        attach(i, layer)
    return model, trace


def publisher(torch,prep,meta,case,check):
    src=ROOT/prep["OWNER"]
    require(hashlib.sha256(src.read_bytes()).hexdigest()==PUBLISHER, "official Large publisher changed")
    owner=runpy.run_path(str(src))
    owner["freeze_math"]()
    folder,config,index=owner["authenticate"](prep["PUBLISHER"],"Qwen/Qwen3-4B")
    require(len(index)==398 and len(set(index.values()))==3 and config["tie_word_embeddings"],"Large publisher identity differs")
    def phase(name,detail=None):
        check()
        print("AIRK_LARGE_REFERENCE_PHASE "+json.dumps({"stage":name,"detail":detail}),flush=True)
    official,trace=streamed_official_model(torch,owner["make_wrapper"],config,folder,index,check,phase)
    check()
    control,=[r for r in meta["rows"] if len(r["input"]["input_ids"])==32]
    base=score_publisher(torch,official,control["input"])
    require(trace == list(range(36)), "original positive control did not consume all decoder layers")
    error=max(abs(a-b) for a,b in zip(base,control["reference_original_fp32"],strict=True))
    require(error<=0.001,"official Large T32 positive control differs")
    values=score_publisher(torch,official,case["input"])
    check()
    require(trace == list(range(36))*2, "publisher forward count or decoder order differs")
    return {"decoder_trace":trace,"original_fp32":values,"t32_control":base,"t32_max_abs":error,"official_forwards":2,
            "official_source_sha256":PUBLISHER,"chunked_reader_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),"max_bf16_chunk_bytes":4<<20,"input_digest":INPUT,
            "model":"Qwen/Qwen3-4B","revision":owner["REVISIONS"]["Qwen/Qwen3-4B"]}
