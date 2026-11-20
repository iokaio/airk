# SPDX-License-Identifier: Apache-2.0
"""Prepare only source pins for the Rust CU15 product metadata observation."""
import json
from pathlib import Path
import sys

sys.path.insert(0,str(Path(__file__).resolve().parent))
from windows_notice_sources import REQUEST, ROOT, REQUEST_SHA, digest


def pin(path, sha):
    if digest(path)!=sha:
        raise ValueError(f"product source differs: {path}")
    return {"path":str(path),"sha256":sha}


def request():
    config = json.loads(Path(pin(REQUEST,REQUEST_SHA)["path"]).read_bytes())
    campaign = REQUEST.parent
    notices = campaign/"windows-product-notice-sources-r3"
    env_path = Path(config["fresh_output"])/"environment.json"
    environment = pin(env_path,"05c074577ed80c5f2ac733a5775f71732ff631c890a78932083fddde9a7b4d5c")
    runtime = env_path.parent
    env = json.loads(env_path.read_bytes())
    build = pin(runtime/"fresh-static-build-proof.json","5618cca298c6472e5e5bab539ca27c23bce9282eb5e17b5866858f5a34bfbd4d")
    if json.loads(Path(build["path"]).read_bytes())!=env["build"] or env["activation_pins_available"] is not False:
        raise ValueError("foreign build or environment")
    original = env["build"]
    aggregate = env["aggregate"]
    locked = lambda row: pin(Path(row["path"]),row["sha256"])
    legal = pin(notices/"legal-owner.json","98960834a8c607c326127fd359ae330702be3fc5fd05dc50b3e6ece9843f555e")
    notice_rows = json.loads(Path(legal["path"]).read_bytes())
    parents = [locked(row) for row in config["native_inputs"][:3]]
    source = {
        "schema":"airk.private.cuda-product-source-input/1", "build_proof":build,
        "producer_receipt":locked(original["build_receipt"]), "aggregate_proof":environment,
        "aggregate_receipt":pin(runtime/"fresh-aggregate.json",aggregate["receipt_sha256"]),
        "source_lock":locked(original["source_lock"]), "cpu_lock":locked(original["cpu_lock"]),
        "cuda_inventory":parents[1], "cuda_version":parents[0], "cudnn_inventory":parents[2],
        "verifier":pin(ROOT/"scripts/cuda-static-runtime/build/fresh_verify.py",
                       "4dd46151df7ff631ae93bb5c807a9c7c4de7c87579dd845ba64b22ea5e3c706f"),
        "aggregate_archive":pin(runtime/aggregate["output"]["path"],aggregate["output"]["sha256"]),
        "legal_inventory":legal,
        "legal_owner":pin(Path(__file__).with_name("windows_notice_sources.py"),
                          "56e5111774c73aa6ee32114e68d51154a0056243c358d5d2b06a6efe84f1dc9d"),
        "notices":[],
    }
    for row in notice_rows:
        path = notices/"notices"/row["package_name"]
        if path.parent != notices/"notices" or path.stat().st_size!=row["bytes"]:
            raise ValueError("invalid staged source notice")
        source["notices"].append({"component":row["component"],"package_name":row["package_name"],
                                  "source":pin(path,row["sha256"]),"original_source_sha256":row["source_artifact_sha256"],
                                  "bytes":row["bytes"]})
    return source


if __name__=="__main__":
    print(json.dumps(request(),sort_keys=True,allow_nan=False))
