# SPDX-License-Identifier: Apache-2.0
"""Launch one measured Focus1024 native qualification selector with a fresh owned CPU Job."""
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from export_graph import CYCLE, ROOT, require, save, sha

WRAPPER = ROOT / "scripts/arbiter/evaluation/run_compact_answer.py"
sha(WRAPPER, "9625bb14ca85eaf5057b3b272d92eb99589fa13e47ea1eeef56d35b0fcb09496")
native = runpy.run_path(str(WRAPPER))
PREFIX = "inference_engine::pool::arbiter::qualification::focus::"
STAGES = {
    "parity": ("focus_native_weight8_static1024_matches_publisher_and_compiled_model_views", "AIRK_FOCUS_NATIVE_PARITY_OUTPUT"),
    "resource": ("focus_native_cpu4_tier_records_maximum1024_pair_resources", "AIRK_FOCUS_NATIVE_RESOURCE_OUTPUT"),
    "original-compile": ("focus_static1024_compiles_original_dev_pair_without_dispatch", "AIRK_FOCUS_NATIVE_COMPILE_OUTPUT"),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", required=True, choices=STAGES)
    parser.add_argument("--image", required=True, type=Path)
    parser.add_argument("--image-sha256", required=True)
    parser.add_argument("--descriptor", required=True, type=Path)
    parser.add_argument("--descriptor-sha256", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    out = args.output.resolve()
    require(out.parent == CYCLE and out.name.startswith("focus1024-native-" + args.stage + "-r")
            and out.name.split("-r")[-1].isdigit(), "fresh named campaign stage required")
    image, descriptor = args.image.resolve(), args.descriptor.resolve()
    require(image.is_relative_to(CYCLE) and descriptor.is_relative_to(CYCLE), "pinned campaign image/descriptor required")
    sha(image, args.image_sha256)
    sha(descriptor, args.descriptor_sha256)
    selector, output_env = STAGES[args.stage]
    proof = out / "native.private.json"
    if args.worker:
        require(os.environ.get("AIRK_FOCUS1024_NATIVE_OWNER") == str(out), "native fixture owner required")
        with native["capped_worker_job"](out):
            env = {k:v for k,v in os.environ.items() if not k.startswith("AIRK_")}
            env.update(AIRK_FOCUS_QUALIFICATION_INPUT=str(descriptor), AIRK_FOCUS_QUALIFICATION_INPUT_SHA256=args.descriptor_sha256,
                       TEMP=str(out/"temp"), TMP=str(out/"temp"), CUDA_VISIBLE_DEVICES="")
            env[output_env] = str(proof)
            if args.stage == "original-compile":
                env["AIRK_FOCUS_NATIVE_ORIGINAL_ANSWER"] = str(CYCLE/"techqa-predispatch-pilot-run-r2/commands/answer.stdout")
            with (out/"native.log").open("xb") as log:
                subprocess.run([image,"--exact",PREFIX+selector,"--ignored","--test-threads","1"],cwd=ROOT,env=env,
                               timeout=240,check=True,stdin=subprocess.DEVNULL,stdout=log,stderr=log,
                               creationflags=subprocess.CREATE_NO_WINDOW)
            require("1 passed; 0 failed" in (out/"native.log").read_text(encoding="utf8") and proof.is_file(),
                    "no passing original native selector/receipt")
        return
    require(not out.exists(), "existing output forbidden")
    out.mkdir()
    (out/"temp").mkdir()
    source_pin = sha(Path(__file__).resolve())
    owner = native["owner_module"]()
    result = owner.run_evaluation_fixture([sys.executable,"-I","-B","-S",Path(__file__).resolve(),*sys.argv[1:],"--worker"],
                                          ROOT, out/"outer.log", 300, os.environ | {"AIRK_FOCUS1024_NATIVE_OWNER":str(out)})
    result.update(stage=args.stage,selector=PREFIX+selector,source_sha256=source_pin,image_sha256=args.image_sha256,
                  descriptor_sha256=args.descriptor_sha256)
    if proof.is_file():
        result["native_sha256"] = sha(proof)
    save(out/"owned.json",result)
    require(owner.successful(result) and result["owned_job_final_active_processes"] == 0
            and result["owned_job_final_member_pids"] == [] and proof.is_file(), "native stage failed to settle")
    sha(Path(__file__).resolve(), source_pin)
    sha(image,args.image_sha256)
    sha(descriptor,args.descriptor_sha256)


if __name__ == "__main__":
    main()
