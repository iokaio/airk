# SPDX-License-Identifier: Apache-2.0
"""Derive probe variants of a retained Context attention graph for memory measurements.

A probe artifact only. The product graph comes from the maintained exporter. See README.md.
"""
import argparse
import hashlib
import json
from pathlib import Path

HALF = "/airk_probe_half"
CHUNK = 1 << 26


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(path):
    sha = hashlib.sha256()
    with open(path, "rb") as file:
        for chunk in iter(lambda: file.read(1 << 24), b""):
            sha.update(chunk)
    return sha.hexdigest()


def external(tensor):
    return {item.key: item.value for item in tensor.external_data}


def consumers(graph):
    out = {}
    for node in graph.node:
        for index, name in enumerate(node.input):
            if name:
                out.setdefault(name, []).append((node, index))
    return out


def drop_attention_bias(graph):
    """Remove the explicit bias from every causal GroupQueryAttention node and prune what only fed it."""
    changed = 0
    for node in graph.node:
        if node.op_type != "GroupQueryAttention":
            continue
        attributes = {a.name: a.i for a in node.attribute if a.type == 2}
        require(attributes.get("causal") == 1 and len(node.input) == 11 and node.input[10],
                "expected a causal attention node with an explicit bias: " + node.name)
        del node.input[10:]
        while node.input and not node.input[-1]:
            del node.input[-1:]
        changed += 1
    needed = {output.name for output in graph.output}
    kept = []
    for node in reversed(list(graph.node)):
        if any(name in needed for name in node.output):
            kept.append(node)
            needed.update(name for name in node.input if name)
    kept.reverse()
    removed = len(graph.node) - len(kept)
    del graph.node[:]
    graph.node.extend(kept)
    return changed, removed


def store(tensor, path, payload):
    """Append bytes to a weight file in the variant folder and point the tensor at them."""
    with open(path, "ab") as file:
        offset = file.tell()
        file.write(payload)
    tensor.ClearField("raw_data")
    del tensor.external_data[:]
    for key, value in (("location", path.name), ("offset", str(offset)), ("length", str(len(payload)))):
        entry = tensor.external_data.add()
        entry.key, entry.value = key, value
    tensor.data_location = 1


def half_weights(onnx, np, graph, source_data, folder, minimum):
    """Store large FP32 weights as FP16 behind a cast, and copy every other external tensor unchanged."""
    used = consumers(graph)
    casts, rewired, plan = [], {}, {"matmul": 0, "gather": 0, "kept": 0, "half_bytes": 0, "kept_bytes": 0}
    half_file, rest_file = folder / "weights-fp16.bin", folder / "weights-rest.bin"
    with open(source_data, "rb") as source:
        for tensor in graph.initializer:
            if tensor.data_location != 1:
                continue
            where = external(tensor)
            require(where.get("location") == source_data.name, "unexpected external data file: " + tensor.name)
            source.seek(int(where["offset"]))
            payload = source.read(int(where["length"]))
            require(len(payload) == int(where["length"]), "short read for " + tensor.name)
            uses = used.get(tensor.name, [])
            elements = int(np.prod(tensor.dims)) if tensor.dims else 1
            matmul = bool(uses) and all(n.op_type == "MatMul" and i == 1 for n, i in uses)
            gather = bool(uses) and all(n.op_type == "Gather" and i == 0 for n, i in uses)
            if tensor.data_type != 1 or len(tensor.dims) != 2 or elements < minimum or not (matmul or gather):
                store(tensor, rest_file, payload)
                plan["kept"] += 1
                plan["kept_bytes"] += len(payload)
                continue
            values = np.frombuffer(payload, dtype="<f4")
            require(bool(np.isfinite(values).all()), "non-finite source weight: " + tensor.name)
            narrow = values.astype("<f2")
            require(bool(np.isfinite(narrow).all()), "finite overflow in half conversion: " + tensor.name)
            original = tensor.name
            tensor.name = original + HALF
            tensor.data_type = 10
            store(tensor, half_file, narrow.tobytes())
            plan["half_bytes"] += narrow.nbytes
            if matmul:
                casts.append(onnx.helper.make_node("Cast", [tensor.name], [original],
                                                   name="/airk_probe_cast/" + str(len(casts)), to=1))
                plan["matmul"] += 1
            else:
                for node, _ in uses:
                    node.input[0] = tensor.name
                    rewired[node.name] = node
                plan["gather"] += 1
    nodes = list(casts)
    for node in graph.node:
        nodes.append(node)
        if node.name in rewired:
            result = node.output[0]
            node.output[0] = result + HALF
            nodes.append(onnx.helper.make_node("Cast", [node.output[0]], [result],
                                               name="/airk_probe_rows_cast/" + node.name, to=1))
    del graph.node[:]
    graph.node.extend(nodes)
    plan["files"] = {path.name: {"bytes": path.stat().st_size, "sha256": digest(path)}
                     for path in (half_file, rest_file) if path.exists()}
    return plan


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--source-data", required=True, type=Path, help="the source graph's external weight file")
    parser.add_argument("--output", required=True, type=Path, help="a fresh, empty directory")
    parser.add_argument("--drop-attention-bias", action="store_true")
    parser.add_argument("--half-weights", action="store_true")
    parser.add_argument("--half-minimum-elements", type=int, default=1 << 20)
    args = parser.parse_args()
    require(args.output.is_dir() and not any(args.output.iterdir()), "a fresh empty output directory is required")
    require(args.drop_attention_bias or args.half_weights, "choose at least one change")

    import numpy as np
    import onnx

    model = onnx.load(str(args.source), load_external_data=False)
    receipt = {"schema": "airk.private.webgpu-probe-variant/1", "qualification": False,
               "source": {"path": str(args.source), "sha256": digest(args.source)},
               "source_data": {"path": str(args.source_data), "bytes": args.source_data.stat().st_size},
               "nodes_before": len(model.graph.node)}
    if args.drop_attention_bias:
        changed, removed = drop_attention_bias(model.graph)
        receipt["attention_bias"] = {"attention_nodes": changed, "pruned_nodes": removed}
    if args.half_weights:
        receipt["half_weights"] = half_weights(onnx, np, model.graph, args.source_data, args.output,
                                               args.half_minimum_elements)
    receipt["nodes_after"] = len(model.graph.node)
    target = args.output / "model.onnx"
    onnx.save(model, str(target))
    receipt["self_contained"] = bool(args.half_weights)
    if receipt["self_contained"]:
        # Without the half-weight rewrite the graph still points at the source weight file,
        # which is not in this folder, so the checker cannot open it.
        onnx.checker.check_model(str(target))
    receipt["model"] = {"path": str(target), "sha256": digest(target)}
    with (args.output / "variant.json").open("x", encoding="utf-8") as file:
        json.dump(receipt, file, indent=1)
    print(json.dumps({k: receipt[k] for k in ("nodes_before", "nodes_after", "self_contained")
                      } | {k: receipt[k] for k in ("attention_bias", "half_weights") if k in receipt}))


if __name__ == "__main__":
    main()
