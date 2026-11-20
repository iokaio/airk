# SPDX-License-Identifier: Apache-2.0
"""Continue an authenticated settled Ninja checkpoint under the original absolute deadline."""
import argparse
import json
from pathlib import Path
import time

import authenticated_build
import build_ort
import checkpoint_ort
from configure_ort import sha


def run(options):
    if sha(options.checkpoint) != options.checkpoint_sha256:
        raise ValueError("unreviewed retained object inventory")
    checkpoint = json.loads(options.checkpoint.read_bytes())
    if (checkpoint["boot_id"] != checkpoint_ort.BOOT.read_text().strip()
        or checkpoint["configure"] != options.sha256 or options.configure.name != "linux-cuda-ort-configure-r4.json"):
        raise ValueError("build checkpoint belongs to another configure or boot")
    before = authenticated_build.validate(options)
    if checkpoint["tree"] != checkpoint_ort.tree():
        raise ValueError("retained sources, objects, graph, or dependency log changed")
    options.expires_boottime = checkpoint["expires_boottime"]
    if time.clock_gettime(time.CLOCK_BOOTTIME) >= options.expires_boottime:
        raise ValueError("original provider deadline expired")
    options.target = "provider"
    options.jobs = 4
    options.attempt = 2
    result = build_ort.run(options)
    after = authenticated_build.validate(options)
    if before != after:
        raise ValueError("build input identity changed during resumed compile")
    result.update({"authenticated_inputs_before":before, "authenticated_inputs_after":after,
                   "checkpoint_receipt":options.checkpoint_sha256, "original_expires_boottime":options.expires_boottime})
    return result


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--configure", type=Path, required=True)
    p.add_argument("--sha256", required=True)
    p.add_argument("--checkpoint", type=Path, required=True)
    p.add_argument("--checkpoint-sha256", required=True)
    print(json.dumps(run(p.parse_args()),sort_keys=True))
