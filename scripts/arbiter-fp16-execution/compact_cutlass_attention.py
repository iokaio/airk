# SPDX-License-Identifier: Apache-2.0
"""Prepare an original-operand, one-node FP32 CUTLASS prototype and check its CUDA decision row."""
import argparse
from array import array
import hashlib
import json
import math
from pathlib import Path
import runpy
import struct
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
ATTENTION = CAMPAIGN / "compact-fp16-full-attention-cut-r1/execution.json"
OWNER_SHA = "2e59784add6b697286297e3b67cee693fcf7f882f8ab8e71d4bb03f1d27e3867"
RUNTIME = CAMPAIGN / "compact-fp16-present-conversion-plan-r2/compact-runtime.json"
RUNTIME_SHA = "a557be764ebcf8a2b777403ac349f2d3ecadfabb5d65e16b8c47026239644c95"
QUERIES, KV_HEADS, DIM, TOKENS = 16, 8, 128, 1014


def pin(path):
    with path.open("rb") as stream:
        sha = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": sha}


def require(value, message):
    if not value:
        raise ValueError(message)


def checked_json(path, sha):
    require(pin(path)["sha256"] == sha, "input changed: " + str(path))
    return json.loads(path.read_bytes())


def graph(onnx):
    helper = onnx.helper
    shape = [1, "tokens", QUERIES * DIM]
    value = lambda name: helper.make_tensor_value_info(name, onnx.TensorProto.FLOAT, shape)
    node = helper.make_node("MultiHeadAttention", ["q", "k", "v"], ["attention"],
                            domain="com.microsoft", name="airk_cutlass_gqa_decision_control",
                            num_heads=QUERIES, unidirectional=1, scale=DIM ** -0.5)
    g = helper.make_graph([node], "airk_compact_fp32_cutlass_control",
                          [value(n) for n in ("q", "k", "v")], [value("attention")])
    model = helper.make_model(g, ir_version=9, opset_imports=[
        helper.make_opsetid("", 17), helper.make_opsetid("com.microsoft", 1)])
    onnx.checker.check_model(model)
    return model.SerializeToString()


def prepare(directory):
    require(directory.is_absolute() and directory.parent == CAMPAIGN and directory.is_dir()
            and not any(directory.iterdir()) and sys.byteorder == "little", "fresh approved empty directory required")
    owner = checked_json(ATTENTION, OWNER_SHA)
    native_pin = owner["outputs"]["native"]
    native = checked_json(Path(native_pin["path"]), native_pin["sha256"])
    # Independently recomputes the complete double precision causal attention of the genuine last query.
    math_owner = HERE / "compact_full_cut_reference.py"
    independent = runpy.run_path(str(math_owner))["exact_dense_attention"](ATTENTION, OWNER_SHA, include_fp64=True)
    buffers = []
    for short, tokens, width in (("q16",1,2048), ("k16",1014,1024), ("v16",1014,1024)):
        name = "/airk_g5/attention/13/" + short
        entry, = [r for r in native["full_cut_rows"] if r["name"] == name]
        require(entry["width"] == width and entry["tokens_retained"] == tokens
                and len(entry["words"]) == tokens * width, "incorrect native input shape")
        buffers.append(array("H", entry["words"]).tobytes())
    runtime = checked_json(RUNTIME, RUNTIME_SHA)
    sys.path.extend(runtime["import_paths"])
    import onnx
    model = directory / "attention.onnx"
    operands = directory / "operands.fp16"
    reference = directory / "decision.fp64"
    model.write_bytes(graph(onnx))
    operands.write_bytes(b"".join(buffers))
    reference.write_bytes(struct.pack("<2048d", *independent.pop("fp64_decision_values")))
    return {"schema": "airk.private.compact-cutlass-operand-preparation/1", "owner": pin(ATTENTION),
            "native": native_pin, "graph": pin(model), "operands": pin(operands), "reference": pin(reference),
            "independent_source": pin(math_owner), "onnx_runtime_source": pin(RUNTIME), "source": pin(Path(__file__).resolve()),
            "baseline_half_comparison": independent, "query_heads": QUERIES, "kv_heads": KV_HEADS,
            "head_dim": DIM, "tokens": TOKENS, "decision": TOKENS-1, "product_admission": False}


def backend_sources(provider_stage):
    """Pin the exact compiled workspace branch and SM80 arithmetic specialization."""
    producer = checked_json(provider_stage / "fresh-compile.json",
                            "fd10bcc2115e681c827162362b90f3133abf5714bd58984921f0b7e38df6b890")
    build = Path(producer["build_root"])
    src = build.parents[1] / "source/onnxruntime/contrib_ops/cuda/bert"
    hashes = {
        "multihead_attention.cc":"3049c974561a7ac03d81800b93d39599837fee0c71a7d5fc29cb0e8a9de22ab0",
        "attention_impl.cu":"bd885825cb3aa27dc0bf2ccf197c1ad41a3e66c14bf167f23685ab59eab094a5",
        "cutlass_fmha/fmha_sm80.cu":"d2709595bf37bff367f876cda5d58c3bf7f5b3cecb153cfa7388d116a3ddecad",
        "cutlass_fmha/memory_efficient_attention.h":"117f9b45d0dccb1b70435bab28737840b864423be51144fc9706d68a99809deb",
        "cutlass_fmha/kernel_forward.h":"9c8bbae05d95d819cb5401cc01e94916386301ba70783d2b4291a97fac2ac501",
        "attention_kernel_options.cc":"cfb3b7400e69387a14df7941628a21e895d0e94a4424782b4b0040fee2ad9e4e",
    }
    sources = {leaf:pin(src / leaf) for leaf in hashes}
    require(all(sources[leaf]["sha256"] == sha for leaf,sha in hashes.items()), "attention provider source changed")
    cutlass = pin(build / "_deps/cutlass-src/examples/41_fused_multi_head_attention/gemm_kernel_utils.h")
    require(cutlass["sha256"] == "1421fcc8a1032124c8136570747c0d7b84ea2efbf7454ce0a746694bfcc3c3a4",
            "SM80 FastF32 tensorcore specialization changed")
    return {"producer":pin(provider_stage / "fresh-compile.json"),"provider_sources":sources,
            "sm80_gemm_type":cutlass,"internal_math":"OpMultiplyAddFastF32, tiled FP32 softmax and FP32 output",
            "qkv_workspace_upper_bytes":3*TOKENS*QUERIES*DIM*4, "additional_attention_workspace_bytes":0,
            "full_score_tensor_equivalent_bytes":TOKENS*TOKENS*QUERIES*4}


def reconcile():
    """Reopen the naturally closed second Job whose outer ANSI parser refused its numeric output."""
    directory = CAMPAIGN / "compact-cutlass-fp32-native-r2"
    receipt = directory / "execution.json"
    previous = checked_json(receipt,"695473dbe6a439733bdea972edc3288d54833db53d1f9458393b87d82789b36d")
    owned = previous["owned"]
    require(previous["complete"] is False and previous["error"]=="ValueError: both actual original queries must use CUTLASS FP32 efficient attention"
            and owned["exit_code"]==0 and owned["owned_descendants_settled"] and not owned["owned_job_forced_termination"]
            and owned["owned_job_final_active_processes"]==0,"expected original postprocessing-only refusal")
    stdout = directory / "stdout-stderr.log"
    raw = stdout.read_bytes()
    require(hashlib.sha256(raw).hexdigest()=="343ec9798ba9c0d7d54021cb78f00eb1815abf55112a4681c4be5afcef468663",
            "original retained stdout differs")
    corrected = HERE / "probe_compact_cutlass.py"
    native, records, scratch = runpy.run_path(str(corrected))["native_output"](raw)
    prep_pin = previous["preparation"]
    prep = checked_json(Path(prep_pin["path"]),prep_pin["sha256"])
    require(native["model_sha256"]==prep["graph"]["sha256"] and native["operands_sha256"]==prep["operands"]["sha256"]
            and Path(native["image"])==Path(previous["image"]["path"]) and native["query_zero_prefix"]==native["query_repeated_prefix"]
            and len(native["native_call_us"])==2 and native["placement"]["complete"], "original scalar output custody differs")
    for item in (previous["image"],previous["runtime_manifest"],previous["build"],previous["build_log"],prep_pin,
                 prep["graph"],prep["operands"],prep["reference"],*previous["sources"].values()):
        require(pin(Path(item["path"]))==item,"original frozen source or input changed")
    ref=struct.unpack("<2048d",Path(prep["reference"]["path"]).read_bytes())
    vector=native["query_zero_prefix"]
    require(len(vector)==2048 and all(math.isfinite(x) for x in vector),"invalid native decision vector")
    errors=[x-y for x,y in zip(vector,ref,strict=True)]
    return {"schema":"airk.private.compact-cutlass-original-output-reconciliation/1","complete":True,"qualification":False,
            "original_owner":pin(receipt),"original_parent_complete":False,"stdout":pin(stdout),"preparation":prep_pin,
            "reviewed_parser":pin(corrected),"source":pin(Path(__file__).resolve()),"kernel_records":records,"scratch":scratch,
            "native_call_us":native["native_call_us"],"head_values_compared":len(errors),"max_abs":max(map(abs,errors)),
            "rms":math.sqrt(math.sumprod(errors,errors)/len(errors)),"owned_peak_host_bytes":owned["peak_host_committed_bytes"],
            "original_before":native["before_session"],"after_each_sync":native["after_each_sync"],
            "after_session_release":native["after_session_release"],"backend_sources":previous["backend_sources"],
            "retrospective_stdout_pin":True,"post_process_adapter_snapshot_observed":False}


def compare(preparation, closure, closure_sha):
    prep = json.loads(preparation.read_bytes())
    owner = checked_json(closure, closure_sha)
    require(owner["complete"] and owner["owned"]["exit_code"] == 0 and owner["owned"]["owned_descendants_settled"]
            and owner["owned"]["owned_job_final_active_processes"] == 0 and owner["preparation"] == pin(preparation),
            "closed authentic operand capture required")
    native = owner["result"]
    ref = prep["reference"]
    require(pin(Path(ref["path"])) == ref and native["query_zero_prefix"] == native["query_repeated_prefix"],
            "unaltered authentic reference and equality across unrelated prefix queries required")
    reference = struct.unpack("<2048d", Path(ref["path"]).read_bytes())
    real = native["query_zero_prefix"]
    require(len(real) == 2048 and all(math.isfinite(x) for x in real), "finite full decision row required")
    errors = [x-y for x,y in zip(real, reference, strict=True)]
    return {"schema":"airk.private.compact-cutlass-operand-comparison/1", "capture":pin(closure),
            "preparation":pin(preparation), "max_abs":max(map(abs,errors)),
            "rms":math.sqrt(math.sumprod(errors,errors)/len(errors)), "qualification":False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(prepare(args.prepare), allow_nan=False))
