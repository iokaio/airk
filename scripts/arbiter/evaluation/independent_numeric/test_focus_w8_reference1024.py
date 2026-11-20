# SPDX-License-Identifier: Apache-2.0
"""Read-only structure controls on the exact private Focus1024 source, without scoring."""
import argparse
import json
from pathlib import Path

import focus_w8_reference1024 as reference


def controls(graph_path, original_fp32_graph):
    import onnx
    weights = graph_path.with_name("model.onnx.data")
    plan = reference.inspect_candidate(graph_path, {graph_path.name: graph_path, weights.name: weights},
                                       original_fp32_graph)
    if (len(plan.projections), len(plan.nonquantized)) != (197, 114):
        raise AssertionError("incomplete original publisher state")
    original = plan.models[0]
    for modification in ("width", "mask", "weight", "node", "constant-metadata"):
        changed = onnx.load_model(graph_path, load_external_data=False)
        if modification == "width":
            changed.graph.input[0].type.tensor_type.shape.dim[1].dim_value = 512
        elif modification == "mask":
            mask = next(n for n in changed.graph.node if n.name == "/Constant_7")
            tensor = mask.attribute[0].t
            tensor.raw_data = bytes([0, 0, 128, 63]) + tensor.raw_data[4:]
        elif modification == "weight":
            changed.graph.initializer[0].name += "_replaced"
        elif modification == "node":
            changed.graph.node[-1].output[0] += "_replaced"
        else:
            mask = next(n for n in changed.graph.node if n.name == "/Constant_7")
            mask.attribute[0].t.name += "_replaced"
        try:
            reference.verify_lift(original, changed)
        except ValueError:
            continue
        raise AssertionError("failed to refuse " + modification)
    sources = ((graph_path, reference.SELECTED_GRAPH), (weights, reference.W8), *reference.original_assets())
    for altered, _ in sources:
        source_hashes = dict(sources)
        source_hashes[altered] = "0" * 64
        try:
            reference.authenticate_sources(sources, source_hashes.__getitem__)
        except ValueError:
            continue
        raise AssertionError("accepted altered source: " + str(altered))
    review = reference.ROOT / "artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/focus-w8-review-r1.json"
    rows = json.loads(review.read_bytes())["files"]
    digest = reference.digest(review)
    reference.authenticate_review(digest, rows)
    reference.authenticate_sources((Path(row["path"]), row["sha256"]) for row in rows)
    for mutation in ((digest, rows[:-1]), ("0" * 64, rows)):
        try:
            reference.authenticate_review(*mutation)
        except ValueError:
            continue
        raise AssertionError("accepted incomplete or changed original review")
    print("1 graph and 10 historical sources matched, 5 topology + 4 source + 2 review mutations refused; 197 W8 + 114 original bindings")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--graph", type=Path, required=True)
    parser.add_argument("--fp32-graph", type=Path, required=True)
    args = parser.parse_args()
    controls(args.graph, args.fp32_graph)
