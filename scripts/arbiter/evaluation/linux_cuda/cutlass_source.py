# SPDX-License-Identifier: Apache-2.0
"""Compare GNU's built CUTLASS provider and read-only original Compact inputs without activating CUDA."""
import json
from pathlib import Path

from configure_ort import BASE, CAMPAIGN, sha
from shipping_build import document
from verified_inputs import identity

PROVIDER_RECEIPT = "ea2bc4d9736a097984ab8d0ef7dd5c63bbc2dc6cd5f0f5f8bf5f06285b8efcc9"
STAGE = "4465463b434f6bb5e9798307c31e622dc3da4d67b6a5f6bf5793daf5c94beabf"
GRAPH = "147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9"
WEIGHTS = "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3"
ATTENTION = {
    "multihead_attention.cc":"3049c974561a7ac03d81800b93d39599837fee0c71a7d5fc29cb0e8a9de22ab0",
    "attention_impl.cu":"bd885825cb3aa27dc0bf2ccf197c1ad41a3e66c14bf167f23685ab59eab094a5",
    "cutlass_fmha/fmha_sm80.cu":"d2709595bf37bff367f876cda5d58c3bf7f5b3cecb153cfa7388d116a3ddecad",
    "cutlass_fmha/memory_efficient_attention.h":"117f9b45d0dccb1b70435bab28737840b864423be51144fc9706d68a99809deb",
    "cutlass_fmha/kernel_forward.h":"9c8bbae05d95d819cb5401cc01e94916386301ba70783d2b4291a97fac2ac501",
    "attention_kernel_options.cc":"cfb3b7400e69387a14df7941628a21e895d0e94a4424782b4b0040fee2ad9e4e",
}
KERNEL = "1421fcc8a1032124c8136570747c0d7b84ea2efbf7454ce0a746694bfcc3c3a4"


def observe(path, expected=None):
    if path.resolve(strict=True) != path or not path.is_file() or path.stat().st_uid != identity():
        raise ValueError("unexpected native file " + str(path))
    value = sha(path)
    if expected is not None and expected != value:
        raise ValueError("native byte identity differs: " + str(path))
    return {"path":str(path),"bytes":path.stat().st_size,"sha256":value}


def main():
    uid = identity()
    root = BASE/"linux-cuda-ort-r4"
    build = root/"b/Release"
    provider = document(CAMPAIGN/"linux-cuda-ort-final-provider-r1.json",PROVIDER_RECEIPT)
    ninja = observe(build/"build.ninja",provider["graph"])
    lines = (build/"build.ninja").read_text().splitlines()
    group = root/"source/onnxruntime/contrib_ops/cuda/bert"
    cu = group/"cutlass_fmha/fmha_sm80.cu"
    rules = [i for i,row in enumerate(lines) if row.startswith("build ") and
             (str(cu)+" || cmake_object_order_depends_target_onnxruntime_providers_cuda") in row]
    if len(rules) != 1:
        raise ValueError("missing unique native SM80 efficient attention object")
    rule = rules[0]
    obj = lines[rule].split(": ",1)[0].removeprefix("build ")
    flags, = [row for row in lines[rule+1:rule+8] if row.startswith("  FLAGS =")]
    link, = [row for row in lines if row.startswith("build libonnxruntime_providers_cuda.so: ")]
    if obj not in link.split() or "compute_86" not in flags or "sm_86" not in flags:
        raise ValueError("original SM86 provider link did not consume the efficient attention object")
    stage = CAMPAIGN/"compact-cutlass-protected-staged-r1"
    stage_row = document(stage/"stage.json",STAGE)
    if not stage_row["complete"] or stage_row["candidate_graph"]["sha256"] != GRAPH or stage_row["candidate_weights"]["sha256"] != WEIGHTS:
        raise ValueError("original protected Compact graph/weight binding changed")
    inputs = [observe(stage/"model.onnx",GRAPH),observe(stage/"weights.bin",WEIGHTS)]
    return {"uid":uid,"inputs":inputs,"stage_sha256":STAGE,"native_provider_build_sha256":PROVIDER_RECEIPT,
            "provider":observe(build/"libonnxruntime_providers_cuda.so",provider["provider"]["sha256"]),
            "ninja":ninja,"object":observe(build/obj),"flags":flags,"link_target":"libonnxruntime_providers_cuda.so",
            "ort_sources":{name:observe(group/name,digest) for name,digest in ATTENTION.items()},
            "cutlass_math":observe(build/"_deps/cutlass-src/examples/41_fused_multi_head_attention/gemm_kernel_utils.h",KERNEL),
            "gpu_invocations":0,"native_graph_observed":False,"qualified":False}


if __name__ == "__main__":
    print(json.dumps(main(),sort_keys=True))
