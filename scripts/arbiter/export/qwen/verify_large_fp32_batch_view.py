# SPDX-License-Identifier: Apache-2.0
"""Independently verify the exact Large FP32 batch-view reference lineage."""

import argparse
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[4]
ORIGINAL = "f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94"
CORRECTED = "5c9a26693eec8522bbfb4d5c4445fa0b60684449a5ea2e1098ec71cca96405b1"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def encoded(value):
    return value.SerializeToString(deterministic=True)


def verify(original, corrected, onnx):
    prefix = "airk_large_fp32_batch_view"
    axis = f"{prefix}_axis0"
    mask = "/scorer/Where_output_0"
    mask_view = f"{prefix}_mask"
    h = onnx.helper
    require(encoded(corrected.graph.initializer[-1]) ==
            encoded(h.make_tensor(axis, onnx.TensorProto.INT64, [1], [0])), "unexpected axis")
    corrected_nodes = iter(corrected.graph.node)
    add_names = {f"/scorer/layers.{i}/self_attn/Add_2": i for i in range(36)}
    found = set()

    def check_view(op, name, data, output):
        expected = h.make_node(op, [data, axis], [output], name=name)
        require(encoded(next(corrected_nodes)) == encoded(expected), f"unexpected {name}")

    for before in original.graph.node:
        if before.name in add_names:
            layer = add_names[before.name]
            stem = f"{prefix}_{layer}"
            require(before.domain == "" and before.op_type == "Add"
                    and len(before.input) == 2 and before.input[1] == mask
                    and len(before.output) == 1, "unexpected original mask Add")
            check_view("Squeeze", f"{stem}_squeeze_logits", before.input[0], f"{stem}_logits")
            after = onnx.NodeProto()
            after.CopyFrom(next(corrected_nodes))
            require(list(after.input) == [f"{stem}_logits", mask_view]
                    and list(after.output) == [f"{stem}_masked"], "unexpected added broadcast")
            after.input[:] = before.input
            after.output[:] = before.output
            require(encoded(after) == encoded(before), "changed Add attributes or identity")
            check_view("Unsqueeze", f"{stem}_restore_batch", f"{stem}_masked", before.output[0])
            found.add(layer)
        else:
            require(encoded(next(corrected_nodes)) == encoded(before), "unrelated node changed")
            if mask in before.output:
                check_view("Squeeze", f"{prefix}_squeeze_mask", mask, mask_view)
    require(found == set(range(36)) and next(corrected_nodes, None) is None,
            "expected all and only 36 batch-view changes")
    del corrected.graph.node[:]
    corrected.graph.node.extend(original.graph.node)
    del corrected.graph.initializer[-1]
    require(encoded(corrected) == encoded(original), "unrelated model field changed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("original", "corrected", "onnx-site-packages", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    require(args.output.is_absolute() and args.output.resolve().is_relative_to(ROOT / "artifacts"),
            "fresh private evidence destination required")
    sys.path.insert(0, str(args.onnx_site_packages.resolve(strict=True)))
    import onnx

    source, derived = args.original.read_bytes(), args.corrected.read_bytes()
    require(hashlib.sha256(source).hexdigest() == ORIGINAL
            and hashlib.sha256(derived).hexdigest() == CORRECTED, "graph identity differs")
    verify(onnx.load_model_from_string(source), onnx.load_model_from_string(derived), onnx)
    proof = {
        "schema": "airk.private.large-fp32-batch-view-reference-lineage/1",
        "predecessor_graph": ORIGINAL,
        "candidate_graph": CORRECTED,
        "exact_inverse": True,
        "verified_mask_adds": 36,
        "operand_order_unchanged": True,
        "comparison_basis": "equivalent_predecessor",
        "native_scoring_dispatched": False,
    }
    with args.output.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(proof, stream, sort_keys=True, indent=2)
        stream.write("\n")


if __name__ == "__main__":
    main()
