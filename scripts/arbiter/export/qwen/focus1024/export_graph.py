# SPDX-License-Identifier: Apache-2.0
"""Re-export the authenticated full weight8 Focus graph at static [1,1024]."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[5]
CYCLE = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
SOURCE = CYCLE / "fp32-paired-answer-r1/arbiter-focus-qwen3-reranker-06b-current-assets"
MODELS = ROOT / "artifacts/advanced-split/models"
PUBLISHER = MODELS / "qwen3-reranker06-assets-r1/candidate-assets/Qwen--Qwen3-Reranker-0.6B"
GRAPH = "5072c663bb8fbc587208dea2564c8ad27880177875e62633f12ea64250ec2738"
WEIGHTS = "1c819b7781fbf9b811f6ec1453e91d7f32a1bd6e5489168e0612d4fcf015b143"
CHECKPOINT = "27cd75a405b9c1b46b59abfd88aaa209e6fed2a1972cde9b70e7659537c5e65b"
SHAPES = {(1, 512, -1, 128): 84, (1, 8, 2, 512, 128): 56, (1, 16, 512, 128): 56, (1, 512, -1): 28}


def require(value, reason):
    if not value:
        raise ValueError(reason)


def sha(path, expected=None):
    require(path.resolve(strict=True) == path and path.is_file() and not path.is_junction(), "canonical input required")
    with path.open("rb") as stream:
        result = hashlib.file_digest(stream, "sha256").hexdigest()
    require(expected is None or result == expected, "input changed: " + path.name)
    return result


def save(path, payload):
    with path.open("x", encoding="utf8", newline="\n") as out:
        json.dump(payload, out, indent=2)
        out.write("\n")


def export(output):
    import numpy as np
    import onnx
    from onnx import numpy_helper

    src = SOURCE / "model.onnx"
    data = SOURCE / "model.onnx.data"
    original = sha(src, GRAPH)
    sha(data, WEIGHTS)
    sha(PUBLISHER / "model.safetensors", CHECKPOINT)
    fp = MODELS / "qwen3-reranker06-static-export-r1/export-r1/receipt.json"
    quant = MODELS / "qwen3-reranker06-weight8-r1/weight8-r1/receipt.json"
    fp_sha = sha(fp, "55b9779a323b009a3ea02af059d75d4f4d396b93642e07faf1582f0c77c0dc43")
    q_sha = sha(quant, "883d756d7b5030c28fa4e936467be7fea4be33f0ae8284ac39e773e9c60bd714")
    require(CHECKPOINT in json.loads(fp.read_bytes())["input_pins"].values()
            and fp_sha in json.loads(quant.read_bytes())["input_pins"].values()
            and any(a["sha256"] == WEIGHTS for a in json.loads(quant.read_bytes())["assets"]), "publisher provenance differs")
    graph = onnx.load(str(src), load_external_data=False)
    require(len(graph.graph.initializer) == 508 and not graph.graph.value_info
            and sum(n.op_type == "MatMulNBits" for n in graph.graph.node) == 197, "weight8 graph topology changed")
    require([i.name for i in graph.graph.input] == ["input_ids", "attention_mask", "position_ids"], "wrong three input ABI")
    for inp in graph.graph.input:
        dims = inp.type.tensor_type.shape.dim
        require([d.dim_value for d in dims] == [1,512] and inp.type.tensor_type.elem_type == 7, "wrong input type/shape")
        dims[1].dim_value = 1024
    before = {t.name: t.SerializeToString() for t in graph.graph.initializer}
    seen = Counter()
    axes = set()
    consumers = {}
    for op in graph.graph.node:
        for index, name in enumerate(op.input):
            consumers.setdefault(name, []).append((op.op_type, index))
    for node in graph.graph.node:
        if node.op_type != "Constant":
            continue
        tensor = node.attribute[0].t
        values = numpy_helper.to_array(tensor)
        old = tuple(values.flatten().tolist()) if values.dtype == np.int64 and values.size <= 5 else None
        if old in SHAPES:
            expected = [("Equal", 0), ("Where", 2)] if old == (1, 8, 2, 512, 128) else [("Reshape", 1)]
            require(consumers[node.output[0]] == expected, "unexpected static shape consumers")
            seen[old] += 1
            result = np.where(values == 512, 1024, values)
        elif node.name in ("/Constant", "/Constant_1"):
            column = node.name == "/Constant_1"
            shape = lambda w: (w,1) if column else (1,w)
            require(np.array_equal(values, np.arange(512, dtype=np.int64).reshape(shape(512))), "position grid differs")
            result = np.arange(1024, dtype=np.int64).reshape(shape(1024))
            axes.add(node.name)
        elif node.name == "/Constant_7":
            require(values.shape == (1,1,512,512) and values.dtype == np.float32 and not np.any(values), "mask base differs")
            result = np.zeros((1,1,1024,1024), dtype=np.float32)
            axes.add(node.name)
        else:
            continue
        tensor.CopyFrom(numpy_helper.from_array(result, name=tensor.name))
    require(seen == SHAPES and axes == {"/Constant", "/Constant_1", "/Constant_7"}, "static shape census differs")
    require(all(t.SerializeToString() == before[t.name] for t in graph.graph.initializer), "learned tensor changed")
    if output is None:
        return {"shape_constants": sum(seen.values()), "position_and_mask_constants": len(axes)}
    dest = output / "assets"
    dest.mkdir()
    with (dest / "model.onnx").open("xb") as out:
        out.write(graph.SerializeToString(deterministic=True))
    shutil.copyfile(data, dest / data.name)
    sha(dest / data.name, WEIGHTS)
    return {"source_graph": original, "publisher_checkpoint": CHECKPOINT, "publisher_revision": "e61197ed45024b0ed8a2d74b80b4d909f1255473",
            "fp32_export_receipt": fp_sha, "weight8_export_receipt": q_sha, "graph": sha(dest / "model.onnx"),
            "external_weights": WEIGHTS, "unchanged_initializers": len(before), "matmul_nbits": 197,
            "reshape_constants": sum(seen.values()), "position_grid_constants": 2, "mask_base_constants": 1, "width": 1024}
