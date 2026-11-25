# SPDX-License-Identifier: Apache-2.0
"""Build one diagnostic MiniLM ONNX asset with two views of the same half weights."""
import argparse
from base64 import b64decode, b64encode
from hashlib import sha256
import json
from pathlib import Path
import runpy
from time import perf_counter

import numpy as np
import onnx
from onnx import TensorProto, external_data_helper, numpy_helper
import onnxruntime as ort
from onnxruntime.transformers.float16 import convert_float_to_float16


BASE = runpy.run_path(str(Path(__file__).with_name("export_minilm_fp16.py")))
METADATA_KEY = "airk.cuda_graph.onnx.base64"
EXTERNAL_FILE = "minilm-fp16.bin"
CPU_GRAPH = "67462feaf92ed9752a4950dbd749a344a08adf4c8b1a87f64c558e7411e51f39"
MAX_MODEL_BYTES = 64 << 20


def varint(data, position, stop):
    value = 0
    for shift in range(0, 70, 7):
        if position == stop:
            break
        byte = data[position]
        position += 1
        value |= (byte & 127) << shift
        if byte < 128 and (shift != 63 or byte <= 1):
            return value, position
    raise ValueError("Truncated or invalid ONNX varint")


def byte_fields(data, start, stop, field):
    """Return positions of exact length-delimited protobuf values."""
    while start < stop:
        tag, start = varint(data, start, stop)
        number, wire = divmod(tag, 8)
        if wire == 0:
            _, start = varint(data, start, stop)
            continue
        if wire == 2:
            length, start = varint(data, start, stop)
        elif wire in (1, 5):
            length = 8 if wire == 1 else 4
        else:
            raise ValueError("Unsupported ONNX protobuf wire type")
        end = start + length
        BASE["require"](end <= stop, "ONNX field exceeds its message")
        if number == field and wire == 2:
            yield start, end
        start = end


def initializer_ranges(encoded, model):
    """Find the raw half bytes, using the loaded ONNX schema's field numbers."""
    require = BASE["require"]
    require(len(encoded) <= MAX_MODEL_BYTES, "ONNX asset exceeds byte budget")
    graph_field = model.DESCRIPTOR.fields_by_name["graph"].number
    (graph_range,) = byte_fields(encoded, 0, len(encoded), graph_field)
    initializer_field = model.graph.DESCRIPTOR.fields_by_name["initializer"].number
    tensor_ranges = list(byte_fields(encoded, *graph_range, initializer_field))
    require(len(tensor_ranges) == len(model.graph.initializer) == 101, "Weight census changed")
    ranges = []
    for tensor, bounds in zip(model.graph.initializer, tensor_ranges, strict=True):
        raw_field = tensor.DESCRIPTOR.fields_by_name["raw_data"].number
        (raw_range,) = byte_fields(encoded, *bounds, raw_field)
        start, stop = raw_range
        require(
            tensor.data_type == TensorProto.FLOAT16
            and memoryview(encoded)[start:stop] == tensor.raw_data,
            f"Invalid half initializer range: {tensor.name}",
        )
        ranges.append(raw_range)
    return ranges


def dynamic_batch(model):
    for value in (*model.graph.input, *model.graph.output):
        value.type.tensor_type.shape.dim[0].dim_param = "batch"
    return model


def make_asset(cpu_bytes, original):
    require = BASE["require"]
    cpu = dynamic_batch(onnx.load_model_from_string(cpu_bytes))
    require(not any(pair.key == METADATA_KEY for pair in cpu.metadata_props), "Duplicate CUDA view")
    onnx.checker.check_model(cpu, full_check=True)
    original_cpu = cpu.SerializeToString(deterministic=True)
    ranges = initializer_ranges(original_cpu, cpu)

    explicit = dynamic_batch(onnx.load_model_from_string(BASE["widened_reference"](original)))
    wide = explicit.SerializeToString(deterministic=True)
    cuda = convert_float_to_float16(explicit, keep_io_types=True, force_fp16_initializers=True)
    onnx.checker.check_model(cuda, full_check=True)
    inline_cuda = cuda.SerializeToString(deterministic=True)
    types = {
        value.name: value.type.tensor_type.elem_type
        for value in (*cuda.graph.input, *cuda.graph.output, *cuda.graph.value_info)
    }
    types.update((tensor.name, tensor.data_type) for tensor in cuda.graph.initializer)
    matmuls = [node for node in cuda.graph.node if node.op_type == "MatMul"]
    require(len(matmuls) == 48 and all(
        all(types[name] == TensorProto.FLOAT16 for name in node.input)
        for node in matmuls
    ), "CUDA matrix inputs are not all FP16")
    targets = {tensor.name: tensor for tensor in cuda.graph.initializer}
    require(len(targets) == len(ranges), "CUDA initializer count changed")
    for node, cpu_tensor, (start, stop) in zip(
        list(cpu.graph.node)[:len(ranges)], cpu.graph.initializer, ranges, strict=True
    ):
        require(
            node.op_type == "Cast" and list(node.input) == [cpu_tensor.name],
            f"CPU graph stopped widening {cpu_tensor.name}",
        )
        tensor = targets[node.output[0]]
        require(
            tensor.data_type == TensorProto.FLOAT16 and tensor.dims == cpu_tensor.dims
            and numpy_helper.to_array(tensor).tobytes() == cpu_tensor.raw_data,
            f"CUDA weights differ: {tensor.name}",
        )
        # External data's in-memory "file" is the complete final ONNX asset.
        external_data_helper.set_external_data(
            tensor, location=EXTERNAL_FILE, offset=start, length=stop - start
        )
        tensor.ClearField("raw_data")
    alternate = cuda.SerializeToString(deterministic=True)
    pair = cpu.metadata_props.add()
    pair.key = METADATA_KEY
    pair.value = b64encode(alternate).decode("ascii")
    canonical = cpu.SerializeToString(deterministic=True)
    loaded = onnx.load_model_from_string(canonical)
    onnx.checker.check_model(loaded, full_check=True)
    require(
        initializer_ranges(canonical, loaded) == ranges,
        "Adding the CUDA graph moved shared initializer ranges",
    )
    (readback,) = [pair.value for pair in loaded.metadata_props if pair.key == METADATA_KEY]
    require(b64decode(readback, validate=True) == alternate, "CUDA metadata readback changed")
    return canonical, alternate, wide, inline_cuda


def session(graph, asset=None):
    options = ort.SessionOptions()
    options.intra_op_num_threads = 1
    options.inter_op_num_threads = 1
    options.enable_mem_pattern = False
    options.add_session_config_entry("session.intra_op.allow_spinning", "0")
    options.add_session_config_entry("session.inter_op.allow_spinning", "0")
    options.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
    if asset is not None:
        options.add_external_initializers_from_files_in_memory(
            [EXTERNAL_FILE], [asset], [len(asset)]
        )
    return ort.InferenceSession(graph, options, providers=["CPUExecutionProvider"])


def proof(canonical, alternate, wide, inline_cuda):
    require = BASE["require"]
    sessions = (
        session(canonical), session(wide), session(alternate, canonical), session(inline_cuda)
    )
    cases = []
    for batch, width in ((1, 32), (2, 128), (32, 32), (32, 128), (32, 256)):
        ids = np.zeros((batch, width), dtype=np.int64)
        mask = np.zeros_like(ids)
        words = [2023, 2003, 1037, 3231, 2005, 6251, 4126, 1012]
        lengths = [width - index % min(width - 2, 22) for index in range(batch)]
        for index, length in enumerate(lengths):
            ids[index, :length] = [101, *np.resize(words[index % 8:] + words[:index % 8], length - 2), 102]
            mask[index, :length] = 1
        inputs = dict(zip(BASE["INPUTS"], (ids, mask, np.zeros_like(ids)), strict=True))
        outputs = [model.run([BASE["OUTPUT"]], inputs)[0] for model in sessions]
        require(
            all(t.shape == (batch, width, 384) and t.dtype == np.float32 and np.isfinite(t).all()
                for t in outputs),
            "Wrong shape, output type, or nonfinite output",
        )
        cpu, reference, cuda_on_cpu, inline_on_cpu = outputs
        require(np.array_equal(cpu, reference), "Canonical CPU view differs from explicit widening")
        require(np.array_equal(cuda_on_cpu, inline_on_cpu), "CUDA external weight view differs")
        vecs = []
        for output in (cpu, cuda_on_cpu):
            vectors = (output * mask[:, :, None]).sum(axis=1, dtype=np.float64)
            vectors /= np.linalg.norm(vectors, axis=1, keepdims=True)
            vecs.append(vectors)
        cases.append({
            "batch": batch,
            "tokens": width,
            "cpu_equal_to_explicit_widening": True,
            "external_equal_to_inline_half_view_on_cpu": True,
            "half_view_on_cpu_max_output_difference": float(np.max(np.abs(cpu - cuda_on_cpu))),
            "half_view_on_cpu_min_normalized_cosine": float(np.min(np.sum(vecs[0] * vecs[1], axis=1))),
        })
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("source", "reference", "cpu-bundle", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    require = BASE["require"]
    require(args.output.is_dir() and not any(args.output.iterdir()), "Expected an empty output folder")
    started = perf_counter()
    original, provenance = BASE["authenticated_inputs"](args.source, args.reference)
    cpu_bytes = BASE["pinned"](args.cpu_bundle / "model.onnx", CPU_GRAPH)
    candidate, alternate, wide, inline_cuda = make_asset(cpu_bytes, original)
    cases = proof(candidate, alternate, wide, inline_cuda)
    json_bytes = BASE["json_bytes"]
    manifest = json.loads((args.cpu_bundle / "model.json").read_bytes())
    require(manifest["model_sha256"] == CPU_GRAPH, "CPU preproof manifest changed")
    manifest["model_sha256"] = sha256(candidate).hexdigest()
    assets = {
        "model.onnx": candidate,
        "model.json": json_bytes(manifest),
        "tokenizer.json": BASE["pinned"](args.cpu_bundle / "tokenizer.json", BASE["TOKENIZER"]),
        "LICENSE": BASE["pinned"](args.cpu_bundle / "LICENSE", BASE["LICENSE"]),
    }
    receipt = {
        "schema": "airk.indexing-plan.minilm-fp16-dual-view-draft/1",
        "source": provenance,
        "base_cpu_sha256": CPU_GRAPH,
        "external_location": EXTERNAL_FILE,
        "alternate_graph_metadata_key": METADATA_KEY,
        "alternate_graph_sha256": sha256(alternate).hexdigest(),
        "alternate_graph_bytes": len(alternate),
        "shared_half_initializers": 101,
        "alternate_matmul_fp16_inputs": 48,
        "onnx": onnx.__version__,
        "onnxruntime": ort.__version__,
        "gpu_run": False,
        "native_product_run": False,
        "wall_seconds": perf_counter() - started,
        "cases": cases,
        "assets": {
            name: {"bytes": len(data), "sha256": sha256(data).hexdigest()}
            for name, data in assets.items()
        },
    }
    for name, data in {**assets, "receipt.json": json_bytes(receipt)}.items():
        with (args.output / name).open("xb") as file:
            file.write(data)


if __name__ == "__main__":
    main()
