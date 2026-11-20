# SPDX-License-Identifier: Apache-2.0
"""Bind an offline r4 Ninja build to reviewed original CUDA and ORT source inventories."""
import argparse
import json
from pathlib import Path

import build_ort
import source_seal
import verified_inputs
from configure_ort import BASE, CAMPAIGN, sha

CUDA = "58ae2ffad7e4be29e0aa862af1ae982704a99806b8f0e161d8d8d4cd12e14be2"
SOURCES = "45acd5cb11124b35bf13ea781ab40d4410b92f1d71a02a60ea8a2ba45b6a8799"


def receipt(name, expected):
    path = CAMPAIGN / name
    if sha(path) != expected:
        raise ValueError("changed private build receipt: " + name)
    return json.loads(path.read_bytes())


def validate(options):
    verified_inputs.identity()
    if sha(options.configure) != options.sha256:
        raise ValueError("changed configure receipt")
    cfg = json.loads(options.configure.read_bytes())
    source = receipt("linux-cuda-r4-compiled-sources-r1.json", SOURCES)
    if source != source_seal.verify(Path(cfg["root"]), cfg):
        raise ValueError("authenticated source inventory changed")
    inputs = receipt("linux-cuda-authenticated-inputs-r1.json", CUDA)
    root = BASE / "cuda-build-inputs-r1"
    if inputs["staging_root"] != str(root):
        raise ValueError("CUDA inventory belongs to another stage")
    verified_inputs.verify(root, inputs["files"])
    return {"source_receipt": SOURCES, "source": source, "cuda_receipt": CUDA,
            "cuda_files_and_links": len(inputs["files"])}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--configure", type=Path, required=True)
    p.add_argument("--sha256", required=True)
    p.add_argument("--target", choices=("provider", "core", "shared"), required=True)
    p.add_argument("--jobs", type=int, choices=(2, 4), required=True)
    p.add_argument("--attempt", type=int, default=1)
    options = p.parse_args()
    before = validate(options)
    result = build_ort.run(options)
    after = validate(options)
    if before != after:
        raise ValueError("build input identity changed")
    result["authenticated_inputs_before"] = before
    result["authenticated_inputs_after"] = after
    print(json.dumps(result, sort_keys=True))
