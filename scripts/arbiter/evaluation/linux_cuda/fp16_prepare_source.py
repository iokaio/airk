# SPDX-License-Identifier: Apache-2.0
"""Emit the Rust-owned Linux CPU source input through its exact model-specific selector."""
import argparse
import json
import os
from pathlib import Path
import pwd
import subprocess
import time

from fp16_supervisor import BUILD, MODULE, private, sha, verified_file

CAMPAIGN = Path("/mnt/d/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z")
ORIGINALS = {
    "compact": ("fresh-cu15-current-compact-numeric-cpu-input-r1.json",
                "18722ef1b3a38777afff5a3ff4ddcb2a396f2730e4e5097a462092e584ce8c27"),
    "large": ("fp32-expanded-reference-preflight-r1/current-native-metadata-source-only-r2/"
              "arbiter-context-qwen3-4b/resource-run-input.private.json",
              "af9a46022b51994f9b5dbe620433a8868d1ad7b40fd223019167ae2ef90cac8b"),
}


def run(image_path, image_digest, destination, model="compact"):
    original_name, original_sha = ORIGINALS[model]
    original = CAMPAIGN / original_name
    selector = MODULE + f"linux::emit_linux_{model}_cpu_resource_input_from_pinned_original"
    if os.getresuid() != (1000,) * 3 or pwd.getpwuid(os.getuid()).pw_name != "tyler":
        raise ValueError("registered default human required")
    private(image_path)
    if destination.parent != BUILD or os.path.lexists(destination):
        raise ValueError("fresh model-free source projection directory required")
    private(BUILD, True)
    os.umask(0o077)
    with verified_file(original, original_sha), verified_file(image_path, image_digest) as image:
        destination.mkdir(mode=0o700)
        private(destination, True)
        tmp = destination / "private-temp"
        tmp.mkdir(mode=0o700)
        private(tmp, True)
        output = destination / f"linux-{model}-cpu-input.json"
        env = dict(PATH="/usr/bin:/bin", HOME="/home/tyler", LANG="C", LC_ALL="C",
                   TEMP=str(tmp), TMP=str(tmp), TMPDIR=str(tmp), AIRK_PRIVATE_CENSUS_BASE=str(tmp),
                   AIRK_SOFTWARE_V2_NATIVE_INPUT=str(original), AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256=original_sha,
                   AIRK_CUDA_QUALIFICATION_OUTPUT=str(output))
        started = time.monotonic()
        args = [str(image_path),"--exact",selector,"--ignored","--nocapture","--test-threads=1"]
        with (destination / "native.log").open("xb") as log:
            completed = subprocess.run(args, executable=f"/proc/self/fd/{image.fileno()}",
                                       cwd=destination, env=env, pass_fds=(image.fileno(),),
                                       timeout=120, stdout=log, stderr=subprocess.STDOUT)
        elapsed = time.monotonic() - started
        if completed.returncode or elapsed > 120 or b"test result: ok. 1 passed;" not in (destination/"native.log").read_bytes():
            raise ValueError("model-free selector failed or selected zero tests")
        if sha(image) != image_digest:
            raise ValueError("frozen test image changed")
        private(output)
        with output.open("rb") as file:
            digest = sha(file)
        receipt = {"schema":f"airk.private.linux-{model}-fp16-source-projection/1",
                   "image":{"path":str(image_path),"sha256":image_digest},
                   "original":{"path":str(original),"sha256":original_sha},
                   "linux_input":{"path":str(output),"sha256":digest},
                   "command":args,"elapsed_seconds":elapsed,"exit":completed.returncode,
                   "uid":os.getuid(),"native_selectors_passed":1,
                   "runtime_admission":False,"numeric_forwards":0,"qualifies_gpu":False}
        with (destination/"receipt.json").open("x") as file:
            json.dump(receipt,file,separators=(",",":"))
        for path in destination.rglob("*"):
            private(path,path.is_dir())
        return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image",type=Path,required=True)
    parser.add_argument("--image-sha",required=True)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--model", choices=tuple(ORIGINALS), default="compact")
    args = parser.parse_args()
    print(json.dumps(run(args.image,args.image_sha,args.output,args.model),sort_keys=True))
