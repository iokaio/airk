# SPDX-License-Identifier: Apache-2.0
"""Independent mathematical Torch reference for the exact selected Focus W8 layout.

No ORT import, quantizer, target kernel or expected-output capture is used.
The worker owns all source/runtime pins and real process/model execution custody.
"""
from __future__ import annotations
import hashlib
import json
import math
from pathlib import Path
import re
import stat
import struct
import sys

SPEC_URL = "https://raw.githubusercontent.com/microsoft/onnxruntime/v1.30.0/docs/ContribOperators.md#com.microsoft.MatMulNBits"
BLOCK = 32
LAYERS = 28
PROJECTED = 197
NONQUANTIZED = 114
CHUNK = 4 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def expected_projection_keys():
    return {f"base.layers.{i}.{section}.{name}.weight"
            for i in range(LAYERS)
            for section, names in (("self_attn", ("q_proj", "k_proj", "v_proj", "o_proj")),
                                   ("mlp", ("gate_proj", "up_proj", "down_proj")))
            for name in names} | {"readout_rows"}


def expected_nonquantized_keys():
    return {"base.embed_tokens.weight", "base.norm.weight"} | {
        f"base.layers.{i}.{name}" for i in range(LAYERS) for name in (
            "self_attn.q_norm.weight", "self_attn.k_norm.weight",
            "input_layernorm.weight", "post_attention_layernorm.weight")}


def projection_key(name):
    if name == "/MatMul_Q8":
        return "readout_rows"
    match = re.fullmatch(r"/base/layers\.([0-9]+)/(self_attn|mlp)/([a-z_]+)/MatMul_Q8", name)
    require(match is not None, "unrecognized quantized projection name")
    layer, section, name = match.groups()
    key = f"base.layers.{int(layer)}.{section}.{name}.weight"
    require(key in expected_projection_keys(), "projection outside complete publisher roster")
    return key


def file_signature(path):
    path = Path(path)
    meta = path.lstat()
    require(path.is_absolute() and stat.S_ISREG(meta.st_mode)
            and not stat.S_ISLNK(meta.st_mode) and not getattr(meta, "st_file_attributes", 0) & 0x400
            and meta.st_nlink == 1, "ordinary unlinked absolute source file required")
    return (meta.st_dev, meta.st_ino, meta.st_size, meta.st_mtime_ns)


class Reader:
    def __init__(self, graph, supplied=None):
        self.graph = Path(graph)
        self.supplied = {name: Path(value) for name, value in (supplied or {}).items()}
        self.strict_map = supplied is not None
        self.handles = {}
        self.signatures = {}
        self.signatures[str(self.graph)] = file_signature(self.graph)

    def close(self):
        for handle in self.handles.values():
            handle.close()

    def verify(self):
        for path, original in self.signatures.items():
            require(file_signature(Path(path)) == original, "consumed tensor source changed")

    def location(self, tensor):
        entries = {item.key: item.value for item in tensor.external_data}
        require(len(entries) == len(tensor.external_data)
                and set(entries) <= {"location", "offset", "length"},
                "unknown external tensor storage declaration")
        name = entries.get("location", "")
        require(name and Path(name).name == name and not any(c in name for c in ("/", "\\", ":")),
                "external tensor must name one ordinary local file")
        require(not self.strict_map or name in self.supplied,
                "external candidate tensor absent from native file map")
        path = self.supplied.get(name, self.graph.parent / name)
        require(path == self.graph.parent / name,
                "native file map differs from graph-local external storage")
        signature = file_signature(path)
        self.signatures.setdefault(str(path), signature)
        require(self.signatures[str(path)] == signature, "external source identity changed")
        offset = int(entries.get("offset", "0"))
        length = int(entries.get("length", str(signature[2] - offset)))
        require(offset >= 0 and length >= 0 and offset + length <= signature[2],
                "external tensor slice exceeds original file")
        return path, offset, length

    def size(self, tensor):
        import onnx
        dtype = onnx.helper.tensor_dtype_to_np_dtype(tensor.data_type)
        require(dtype.kind in "buif" and all(d >= 0 for d in tensor.dims),
                "unsupported tensor element type/shape")
        return math.prod(tensor.dims) * dtype.itemsize

    def chunks(self, tensor):
        expected = self.size(tensor)
        if tensor.external_data:
            path, offset, length = self.location(tensor)
            require(length == expected, "external tensor byte count differs from exact type/shape")
            handle = self.handles.setdefault(str(path), None)
            if handle is None:
                handle = path.open("rb")
                self.handles[str(path)] = handle
            handle.seek(offset)
            remaining = length
            while remaining:
                raw = handle.read(min(CHUNK, remaining))
                require(raw, "external tensor ended early")
                remaining -= len(raw)
                yield raw
        else:
            import onnx
            raw = bytes(tensor.raw_data) if tensor.raw_data else onnx.numpy_helper.to_array(tensor).tobytes()
            require(len(raw) == expected, "inline tensor payload differs from exact type/shape")
            for offset in range(0, len(raw), CHUNK):
                yield raw[offset:offset + CHUNK]

    def payload(self, tensor, maximum):
        require(self.size(tensor) <= maximum, "bounded tensor payload exceeded")
        return b"".join(self.chunks(tensor))


def tensor_semantics(tensor, reader):
    raw = reader.payload(tensor, 16 * 1024 * 1024)
    return {"dtype": tensor.data_type, "shape": list(tensor.dims),
            "value_sha256": hashlib.sha256(raw).hexdigest()}


def node_semantics(node, reader):
    import onnx
    attrs = {}
    for attribute in node.attribute:
        require(attribute.name not in attrs, "duplicate node attribute")
        kind = attribute.type
        if kind == onnx.AttributeProto.TENSOR:
            value = tensor_semantics(attribute.t, reader)
        elif kind == onnx.AttributeProto.FLOAT:
            value = struct.pack("<f", attribute.f).hex()
        elif kind == onnx.AttributeProto.FLOATS:
            value = [struct.pack("<f", x).hex() for x in attribute.floats]
        elif kind == onnx.AttributeProto.INT:
            value = attribute.i
        elif kind == onnx.AttributeProto.INTS:
            value = list(attribute.ints)
        elif kind == onnx.AttributeProto.STRING:
            value = bytes(attribute.s).hex()
        elif kind == onnx.AttributeProto.STRINGS:
            value = [bytes(x).hex() for x in attribute.strings]
        else:
            raise ValueError("unknown/nested node attribute type")
        attrs[attribute.name] = {"type": kind, "value": value}
    return {"name": node.name, "op": node.op_type, "domain": node.domain,
            "inputs": list(node.input), "outputs": list(node.output), "attributes": attrs}


def abi(model):
    def row(value):
        tensor = value.type.tensor_type
        require(all(d.HasField("dim_value") for d in tensor.shape.dim), "symbolic input/output ABI")
        return (value.name, tensor.elem_type, tuple(d.dim_value for d in tensor.shape.dim))
    return ([row(x) for x in model.graph.input], [row(x) for x in model.graph.output])


def quantized_layout(node, initializers):
    import onnx
    require(node.op_type == "MatMulNBits" and node.domain == "com.microsoft"
            and len(node.input) == 3 and len(node.output) == 1,
            "exact three-input MatMulNBits projection required; optional inputs refuse")
    require(all(a.type == onnx.AttributeProto.INT for a in node.attribute), "noninteger quantization attribute")
    attributes = {a.name: a.i for a in node.attribute}
    require(len(attributes) == len(node.attribute)
            and set(attributes) == {"K", "N", "bits", "block_size"},
            "unknown quantization attributes, including accuracy/packing overrides")
    k, n = attributes["K"], attributes["N"]
    require(attributes["bits"] == 8 and attributes["block_size"] == BLOCK
            and k > 0 and n > 0 and k % BLOCK == 0,
            "only exact selected weight8/block32/no-padding layout supported")
    require(node.input[1] in initializers and node.input[2] in initializers,
            "quantized weight/scales must be original constant initializers")
    q, scales = initializers[node.input[1]], initializers[node.input[2]]
    require(q.data_type == onnx.TensorProto.UINT8 and tuple(q.dims) == (n, k // BLOCK, BLOCK)
            and scales.data_type == onnx.TensorProto.FLOAT and tuple(scales.dims) == (n, k // BLOCK),
            "quantized tensor dtype/shape differs from exact operator storage")
    return k, n, q, scales


class CheckedPlan:
    """Native artifact descriptions only; no execution or qualification capability."""
    pass


def inspect_candidate(graph_path, external_file_map, original_fp32_graph):
    import onnx
    graph_path, original_fp32_graph = Path(graph_path), Path(original_fp32_graph)
    require(Path(external_file_map[graph_path.name]) == graph_path,
            "native candidate file map does not bind exact graph path")
    candidate_reader = Reader(graph_path, external_file_map)
    original_reader = Reader(original_fp32_graph)
    try:
        candidate = onnx.load_model(graph_path, load_external_data=False)
        original = onnx.load_model(original_fp32_graph, load_external_data=False)
        for model in (candidate, original):
            require(not model.functions and not model.training_info and not model.graph.sparse_initializer,
                    "local functions/training/sparse state not part of selected full export")
        expected_abi = ([(name, onnx.TensorProto.INT64, (1, 512))
                         for name in ("input_ids", "attention_mask", "position_ids")],
                        [("logits", onnx.TensorProto.FLOAT, (1, 2))])
        require(abi(candidate) == abi(original) == expected_abi, "selected static512 ABI differs")
        require([(o.domain, o.version) for o in original.opset_import] == [("", 18)],
                "original FP32 exporter opset differs")
        require(len(candidate.opset_import) == 2
                and {(o.domain, o.version) for o in candidate.opset_import}
                == {("", 18), ("com.microsoft", 1)}, "candidate operator imports differ")
        old = {t.name: t for t in original.graph.initializer}
        current = {t.name: t for t in candidate.graph.initializer}
        require(len(old) == len(original.graph.initializer) == 311
                and len(current) == len(candidate.graph.initializer) == 508,
                "full selected initializer roster differs")
        require(len(candidate.graph.node) == len(original.graph.node) == 3957,
                "full selected graph topology census differs")
        before_nodes = {node.name: node for node in original.graph.node}
        require(len(before_nodes) == len(original.graph.node) and "" not in before_nodes,
                "unique complete original graph node names required")
        require(len({node.name for node in candidate.graph.node}) == len(candidate.graph.node),
                "duplicate candidate graph node name")
        projections = []
        topology = []
        keys, old_weights, quantized_names, matched_nodes = set(), set(), set(), set()
        for after in candidate.graph.node:
            original_name = after.name[:-3] if after.op_type == "MatMulNBits" and after.name.endswith("_Q8") else after.name
            require(original_name in before_nodes and original_name not in matched_nodes,
                    "candidate graph node absent or ambiguous in original topology")
            before = before_nodes[original_name]
            matched_nodes.add(original_name)
            if after.op_type == "MatMulNBits":
                key = projection_key(after.name)
                k, n, q, scales = quantized_layout(after, current)
                require(before.op_type == "MatMul" and before.domain == "" and not before.attribute
                        and len(before.input) == 2 and len(before.output) == 1
                        and after.name == before.name + "_Q8"
                        and after.input[0] == before.input[0] and list(after.output) == list(before.output)
                        and after.input[1] == before.input[1] + "_Q8"
                        and after.input[2] == before.input[1] + "_scales",
                        "candidate is not the exact one-for-one original projection conversion")
                require(before.input[1] in old and before.input[1] not in old_weights
                        and old[before.input[1]].data_type == onnx.TensorProto.FLOAT
                        and tuple(old[before.input[1]].dims) == (k, n)
                        and key not in keys, "original constant projection/source-state mapping differs")
                keys.add(key); old_weights.add(before.input[1])
                quantized_names.update((q.name, scales.name))
                candidate_reader.location(q) if q.external_data else None
                candidate_reader.location(scales) if scales.external_data else None
                projections.append({"key": key, "K": k, "N": n, "q": q, "scales": scales,
                                    "node": after.name, "original_weight": before.input[1]})
                topology.append(node_semantics(before, original_reader))
            else:
                expected = node_semantics(before, original_reader)
                require(node_semantics(after, candidate_reader) == expected,
                        "non-converted topology/edge/attribute changed")
                topology.append(expected)
        require(matched_nodes == set(before_nodes), "original named graph node omitted")
        topology.sort(key=lambda node: node["name"])
        require(len(projections) == PROJECTED and keys == expected_projection_keys()
                and len(quantized_names) == PROJECTED * 2,
                "complete197 unique publisher projection/readout closure required")
        nonquantized = set(old) - old_weights
        require(nonquantized == set(current) - quantized_names == expected_nonquantized_keys()
                and len(nonquantized) == NONQUANTIZED,
                "complete114 original nonquantized state closure required")
        for name in nonquantized:
            require(current[name].data_type == old[name].data_type == onnx.TensorProto.FLOAT
                    and list(current[name].dims) == list(old[name].dims),
                    "nonquantized initializer type/shape changed")
            if current[name].external_data:
                candidate_reader.location(current[name])
        candidate_reader.verify(); original_reader.verify()
        plan = CheckedPlan()
        plan.graph = graph_path
        plan.original_graph = original_fp32_graph
        plan.files = dict(external_file_map)
        plan.projections = projections
        plan.nonquantized = {name: current[name] for name in sorted(nonquantized)}
        plan.models = (candidate, original)
        plan.signatures = {**candidate_reader.signatures, **original_reader.signatures}
        plan.evidence = {"schema": "airk.private.focus-weight8-reference-binding/1",
            "mathematical_spec": SPEC_URL, "target_kernel_used_for_reference": False,
            "operator": {"bits": 8, "block_size": 32, "default_zero_point": 128,
                         "default_accuracy_level": 0, "weight_prepacked": False},
            "graph": str(graph_path), "graph_sha256": hashlib.sha256(graph_path.read_bytes()).hexdigest(),
            "original_fp32_graph": str(original_fp32_graph),
            "original_fp32_graph_sha256": hashlib.sha256(original_fp32_graph.read_bytes()).hexdigest(),
            "full_topology_nodes": len(topology), "projection_count": len(projections),
            "nonquantized_initializer_count": len(nonquantized),
            "restored_original_topology_sha256": hashlib.sha256(
                json.dumps(topology, sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
            "model_forward_executed": False, "qualification_granted": False}
        return plan
    finally:
        candidate_reader.close(); original_reader.close()


def decode_weight(torch, q_bytes, scales_bytes, n, k):
    require(sys.byteorder == "little" and n > 0 and k > 0 and k % BLOCK == 0,
            "selected little-endian complete-block layout required")
    require(len(q_bytes) == n * k and len(scales_bytes) == n * (k // BLOCK) * 4,
            "exact quantized/scales byte lengths required")
    quantized = torch.frombuffer(bytearray(q_bytes), dtype=torch.uint8).reshape(n, k // BLOCK, BLOCK)
    scales = torch.frombuffer(bytearray(scales_bytes), dtype=torch.float32).reshape(n, k // BLOCK)
    require(bool(torch.isfinite(scales).all().item()), "nonfinite quantized scale")
    decoded = ((quantized.to(torch.float32) - 128) * scales.unsqueeze(-1)).reshape(n, k)
    require(bool(torch.isfinite(decoded).all().item()), "nonfinite independently decoded weight")
    return decoded


def cpu_state(torch, tensor, shape):
    require(tensor.dtype == torch.float32 and tensor.device.type == "cpu"
            and tuple(tensor.shape) == tuple(shape) and tensor.is_contiguous()
            and bool(torch.isfinite(tensor).all().item()),
            "exact original CPU FP32 contiguous state required")
    return tensor.detach().view(torch.uint8).reshape(-1)


def state_hash(byte_view):
    digest = hashlib.sha256()
    for offset in range(0, byte_view.numel(), CHUNK):
        digest.update(byte_view[offset:offset + CHUNK].numpy().tobytes())
    return digest.hexdigest()


def apply_candidate(torch, wrapper, checkedplan, original_state):
    require(isinstance(checkedplan, CheckedPlan), "checked candidate topology plan required")
    for path, expected in checkedplan.signatures.items():
        require(file_signature(Path(path)) == expected, "checked graph/tensor sources changed")
    expected_keys = expected_projection_keys() | expected_nonquantized_keys()
    actual = wrapper.state_dict()
    require(set(original_state) == set(actual) == expected_keys,
            "full311 persistent publisher/wrapper state closure required")
    parameters = dict(wrapper.named_parameters())
    require(set(parameters) == expected_keys - {"readout_rows"}
            and "readout_rows" in dict(wrapper.named_buffers()),
            "publisher parameter/readout ownership mapping differs")
    reader = Reader(checkedplan.graph, checkedplan.files)
    decoded = []
    bindings = []
    try:
        for name in sorted(expected_keys):
            original_bytes = cpu_state(torch, original_state[name], actual[name].shape)
            actual_bytes = cpu_state(torch, actual[name], actual[name].shape)
            require(torch.equal(original_bytes, actual_bytes),
                    "wrapper state differs from independently loaded original publisher state")
        for name, tensor in checkedplan.nonquantized.items():
            view = cpu_state(torch, original_state[name], tensor.dims)
            digest = hashlib.sha256(); offset = 0
            for raw in reader.chunks(tensor):
                observed = torch.frombuffer(bytearray(raw), dtype=torch.uint8)
                require(torch.equal(view[offset:offset + len(raw)], observed),
                        "candidate nonquantized bytes differ from original publisher state")
                digest.update(raw); offset += len(raw)
            require(offset == view.numel(), "complete nonquantized tensor bytes absent")
            bindings.append({"state": name, "kind": "unchanged_fp32",
                             "shape": list(tensor.dims), "bytes": offset,
                             "candidate_and_publisher_sha256": digest.hexdigest()})
        for projection in checkedplan.projections:
            key, n, k = projection["key"], projection["N"], projection["K"]
            original_bytes = cpu_state(torch, original_state[key], (n, k))
            q_bytes = reader.payload(projection["q"], n * k)
            scales_bytes = reader.payload(projection["scales"], n * (k // BLOCK) * 4)
            weight = decode_weight(torch, q_bytes, scales_bytes, n, k)
            decoded.append((key, weight))
            bindings.append({"state": key, "kind": "candidate_uint8_block32",
                "node": projection["node"], "shape": [n, k],
                "q_sha256": hashlib.sha256(q_bytes).hexdigest(),
                "scales_sha256": hashlib.sha256(scales_bytes).hexdigest(),
                "original_publisher_fp32_sha256": state_hash(original_bytes),
                "independently_decoded_fp32_sha256": state_hash(cpu_state(torch, weight, (n, k)))})
        reader.verify()
        for path, expected in checkedplan.signatures.items():
            require(file_signature(Path(path)) == expected, "sources changed before model mutation")
        # Every refusal-sensitive check completes before the first parameter write.
        with torch.no_grad():
            for key, weight in decoded:
                target = wrapper.readout_rows if key == "readout_rows" else parameters[key]
                target.copy_(weight)
        evidence = dict(checkedplan.evidence)
        evidence.update({"full_persistent_state_count": len(expected_keys),
            "state_bindings": bindings, "candidate_applied": True,
            "original_fp32_branch_must_use_separate_model": True,
            "scope": "Independent operator-spec dequantization plus publisher Torch computation; no target ORT reference output, model forward or qualification."})
        return evidence
    finally:
        reader.close()
