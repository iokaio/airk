# SPDX-License-Identifier: Apache-2.0
"""Bind the selected 1024 graph to the independently checked publisher W8 parameters."""
from collections import Counter
import hashlib
import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
ORIGINAL = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z/fp32-paired-answer-r1/arbiter-focus-qwen3-reranker-06b-current-assets/model.onnx"
SOURCE_GRAPH = "5072c663bb8fbc587208dea2564c8ad27880177875e62633f12ea64250ec2738"
SELECTED_GRAPH = "a16fcaa1bf2d67b019b30cda56d76ba23d589c346ef2ae76d75c229ebe866810"
W8 = "1c819b7781fbf9b811f6ec1453e91d7f32a1bd6e5489168e0612d4fcf015b143"
LEGACY_SHA = "6fcc75ec4b7ca354dc86275948df76563f522a2f15d93eed8b35128424988758"


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def load_decoder():
    path = Path(__file__).with_name("focus_w8_reference.py")
    require(digest(path) == LEGACY_SHA, "original independent decoder differs")
    spec = importlib.util.spec_from_file_location("focus1024_original_w8_decoder", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def verify_lift(original, selected):
    """Account for every byte of the model after independently reversing static constants."""
    import numpy as np
    import onnx
    before, after = original.graph, selected.graph
    require(len(before.input) == len(after.input) == 3 and
            len(before.node) == len(after.node) == 3957, "static graph census differs")
    for left, right in zip(before.input, after.input, strict=True):
        old = left.type.tensor_type.shape.dim
        new = right.type.tensor_type.shape.dim
        require([x.dim_value for x in old] == [1, 512] and
                [x.dim_value for x in new] == [1, 1024], "selected input width differs")
        new[1].dim_value = 512
    shapes = {(1, 512, -1, 128): 84, (1, 8, 2, 512, 128): 56,
              (1, 16, 512, 128): 56, (1, 512, -1): 28}
    seen = Counter()
    for left, right in zip(before.node, after.node, strict=True):
        if left == right:
            continue
        require(left.name == right.name and left.op_type == right.op_type == "Constant" and
                len(left.attribute) == len(right.attribute) == 1, "nonconstant graph change")
        a = onnx.numpy_helper.to_array(left.attribute[0].t)
        b = onnx.numpy_helper.to_array(right.attribute[0].t)
        require(a.dtype == b.dtype, "constant type changed")
        if left.name in ("/Constant", "/Constant_1"):
            columns = left.name.endswith("_1")
            def grid(width):
                return np.arange(width, dtype=np.int64).reshape((width, 1) if columns else (1, width))
            require(np.array_equal(a, grid(512)) and np.array_equal(b, grid(1024)),
                    "absolute position grid differs")
            key = left.name
        elif left.name == "/Constant_7":
            require(a.dtype == np.float32 and a.shape == (1, 1, 512, 512) and
                    b.shape == (1, 1, 1024, 1024) and not np.any(a) and not np.any(b),
                    "attention mask base differs")
            key = left.name
        else:
            require(a.dtype == np.int64 and a.ndim == 1 and a.size <= 5, "unknown shape change")
            key = tuple(a.tolist())
            require(key in shapes and np.array_equal(b, np.where(a == 512, 1024, a)),
                    "reshape constant differs")
        seen[key] += 1
        reference_tensor = left.attribute[0].t
        tensor = right.attribute[0].t
        tensor.raw_data = reference_tensor.raw_data
        del tensor.dims[:]
        tensor.dims.extend(reference_tensor.dims)
    require(seen == {**shapes, "/Constant": 1, "/Constant_1": 1, "/Constant_7": 1}
            and original == selected, "unaccounted graph bytes or initializers differ")


ORIGINAL_REVIEW = "a58d8168b074e048974ebad4e0ad4947f7e560c1ae2b97f5f790610fc66128fd"


def original_assets():
    return ((ORIGINAL, SOURCE_GRAPH), (ORIGINAL.with_name("model.onnx.data"), W8))


def authenticate_sources(items, hash_file=digest):
    for path, expected in items:
        require(hash_file(path) == expected, f"Focus source differs: {path}")


def authenticate_review(digest_value, rows):
    require(digest_value == ORIGINAL_REVIEW and len(rows) == 10,
            "complete original ten-file Focus review required")


def inspect_candidate(graph_path, external_file_map, original_fp32_graph):
    import onnx
    legacy = load_decoder()
    graph = Path(graph_path)
    data = graph.with_name("model.onnx.data")
    require(Path(external_file_map[graph.name]) == graph and
            Path(external_file_map[data.name]) == data, "candidate file map differs")
    sources = ((graph, SELECTED_GRAPH), (data, W8), *original_assets())
    signatures = {str(path): legacy.file_signature(path) for path, _ in sources}
    authenticate_sources(sources)
    source = {ORIGINAL.name: ORIGINAL, data.name: ORIGINAL.with_name(data.name)}
    plan = legacy.inspect_candidate(ORIGINAL, source, original_fp32_graph)
    verify_lift(plan.models[0], onnx.load_model(graph, load_external_data=False))
    plan.graph = graph
    plan.files = dict(external_file_map)
    plan.signatures.update(signatures)
    plan.evidence.update(graph=str(graph), graph_sha256=SELECTED_GRAPH,
                         source_graph_sha256=SOURCE_GRAPH, static_pair_width=1024,
                         unchanged_external_weights_sha256=W8)
    plan.focus1024_decoder = legacy
    return plan


def apply_candidate(torch, wrapper, checkedplan, original_state):
    return checkedplan.focus1024_decoder.apply_candidate(torch, wrapper, checkedplan, original_state)
