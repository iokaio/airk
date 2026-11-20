# SPDX-License-Identifier: Apache-2.0
"""Bind the Linux Large candidate and source-side SM86 attention recipe without loading CUDA."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess

from configure_ort import BASE, CAMPAIGN, sha
from fp16_supervisor import private, verified_file
from unpack import guarded
from verified_inputs import identity

CONFIGURE = "e579de0a21f10459506187c92cd3eb3c93be5b47d10e03619a07f02419f2edf8"
ORIGINAL = "264a3f5a9ddfb9f43a22328c88ce6497624dec87c6c8659fd5b5a7b3a6464ec3"
SM86_REFERENCE = "c214278ab06a24053d0a309b294e459ec7ba717e99f4e39edd3fedaa26c878bf"
SELECTOR = ("inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::"
            "fp16_large::verify_linux_large_fp16_reference_inputs")
FILES = {
    "flash_fwd_kernel.h":"2ad5673e98305312dfa7cd49df1869c69cd2899f687c823dbcfcd41ab7f91f96",
    "softmax.h":"c611ccb150262140147b84fbde1941634d2cbb00ce1f096d5d26df3518497db2",
    "flash_fwd_launch_template.h":"cd8d91016f32c8ea88cc9bc116609bb010714f3ead7a7f7b05ea076294c59740",
    "flash_fwd_hdim128_fp16_sm80.cu":"d00bb746e879dd53dc930cb3d74ceecf97b9f6b19ee3a7b6c6b24690c4c467d8",
    "kernel_traits.h":"4cc8c2a99db4db04e1ebe218ea42ea381f6ab6e49764db234d6db3271f419fc2",
    "flash_api.cc":"5470523bce4c2afa545b7fe2382ba2c938534a8afdf99a17d8ae32d9f6e4f2a7",
}


def run(options):
    uid = identity()
    os.umask(0o077)
    configure = CAMPAIGN / "linux-cuda-ort-configure-r4.json"
    proof = CAMPAIGN / "large-fp16-baseline-independent-t32-r2/reference.json"
    corrected = CAMPAIGN / "large-fp16-sm86-flash-independent-t32-r1/reference.json"
    if sha(configure) != CONFIGURE or sha(proof) != ORIGINAL or sha(corrected) != SM86_REFERENCE:
        raise ValueError("configure or original independent reference changed")
    cfg = json.loads(configure.read_bytes())
    root = Path(cfg["root"])
    kernel_dir = root / "source/onnxruntime/contrib_ops/cuda/bert/flash_attention"
    actual = {name:sha(kernel_dir / name) for name in FILES}
    cutlass = root / "b/Release/_deps/cutlass-src/include/cute/atom/mma_traits_sm80.hpp"
    trait = sha(cutlass)
    if actual != FILES or trait != "6c39df1b65facd9322bfdf4a74dfb0bf97f1694892ac9b43a61e1590191a364e":
        raise ValueError("Linux flash-attention source or MMA trait differs from independent recipe")
    source = str(kernel_dir / "flash_fwd_hdim128_fp16_sm80.cu")
    rules = cfg["graph_proof"]["commands"]["libonnxruntime_providers_cuda.so"]
    data = subprocess.run([cfg["tools"]["ninja"], "-C", str(root / "b/Release"), "-t", "commands",
                           "libonnxruntime_providers_cuda.so"], check=True, capture_output=True, timeout=30).stdout
    if hashlib.sha256(data).hexdigest() != rules["sha256"]:
        raise ValueError("Linux CUDA compiler commands changed")
    commands = [line for line in data.decode().splitlines() if source in line.split()]
    if len(commands) != 1 or "sm_86" not in commands[0] or "compute_86" not in commands[0]:
        raise ValueError("Large H128 FP16 SM86 kernel is not compiled by the frozen provider")
    baseline = json.loads(proof.read_bytes())
    reference = json.loads(corrected.read_bytes())
    for key in ("graph", "weights", "source_inputs", "input_digest", "original_fp32"):
        if baseline[key] != reference[key]:
            raise ValueError("corrected attention reference differs from the original Large input/asset: " + key)
    if len(reference["matching_fp16"]) != 8:
        raise ValueError("eight independently produced option scores required")
    guarded(BASE,uid,stat.S_ISDIR)
    if options.output.parent != BASE or os.path.lexists(options.output):
        raise ValueError("fresh guest reference observation directory required")
    options.output.mkdir(0o700)
    private(options.output,True)
    out = options.output / "observed.json"
    temp = options.output / "tmp"
    temp.mkdir(0o700)
    private(temp,True)
    env = {"PATH":"/usr/bin:/bin", "HOME":"/home/tyler", "LC_ALL":"C", "TMPDIR":str(temp),
           "AIRK_CUDA_FP16_INDEPENDENT_PROOF":str(proof),"AIRK_CUDA_FP16_INDEPENDENT_PROOF_SHA256":ORIGINAL,
           "AIRK_CUDA_QUALIFICATION_OUTPUT":str(out)}
    with verified_file(options.image, options.image_sha256) as image, (options.output/"native.log").open("xb") as log:
        result = subprocess.run([str(options.image), "--exact", SELECTOR, "--ignored", "--test-threads=1"],
                                executable=f"/proc/self/fd/{image.fileno()}",pass_fds=(image.fileno(),),
                                cwd=options.output,env=env,timeout=120,stdout=log,stderr=subprocess.STDOUT)
    private(out)
    observed = json.loads(out.read_bytes())
    if (result.returncode or b"test result: ok. 1 passed;" not in (options.output/"native.log").read_bytes()
        or observed["input_digest"] != reference["input_digest"] or observed["weights"] != reference["weights"]["sha256"]
        or observed["graph"] != reference["graph"]["sha256"] or observed["model_requests_dispatched"] is not False):
        raise ValueError("native Large asset-pairing control failed")
    return {"configure":CONFIGURE,"original_reference":ORIGINAL,"sm86_reference":SM86_REFERENCE,"kernel_sources":actual,
            "cutlass_mma_traits":trait,"nvcc_command_sha256":hashlib.sha256(commands[0].encode()).hexdigest(),
            "native_observation":observed,"native_image":options.image_sha256,"native_receipt":sha(out),
            "runtime_kernel_selection_observed":False,"numeric_forwards":0,"qualified":False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image",type=Path,required=True)
    parser.add_argument("--image-sha256",required=True)
    parser.add_argument("--output",type=Path,required=True)
    print(json.dumps(run(parser.parse_args()),sort_keys=True))
