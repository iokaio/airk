# SPDX-License-Identifier: Apache-2.0
"""The LG lifetimes file for ``make_loop_spec.py gate --lifetimes`` (changes entry 30).

Evaluation tooling, never product. Writes ``{arm: {launcher, start_args, device_gib, host_gib,
fixtures, lifetime}}`` for the judged arms C (Focus + Compact FP16), L (Focus + Large FP16) and
F (Focus only), each in its own fixture root, and optionally the precision arms.

Every CUDA arm names a pair written by ``cuda_pair.py pair`` and pinned by SHA-256 here; arm F
reuses a pair that holds a Context entry (the review pin supplies the adapter) but stages only
Focus. A fresh root starts with ``--seed-cpu focus`` and indexes the repositories during start;
``--base NAME`` restarts from a closed lifetime of that root instead.

Memory is declared from the A6000 measurements (GPU_FIXTURE.md, changes entries 17, 20 and 29):
GPU peaks Focus 1,024 W8 1.8 GiB (2.7 GB process peak in the parity run), Compact FP16 6.0 GiB,
Large FP16 14.0 GiB; host working-set peak about 17.5 GB with Large. Each figure is rounded up
with headroom. The services' own private host reservations are larger (32 GiB Compact, 40 GiB
Large, lifecycle constants) and are accounting inside each service, not resident memory.

``python -I -B lg_lifetimes.py --series 05-gate-lg --r-root R --image DIR --pair C=pair.json:SHA
--pair L=pair.json:SHA --pair F=pair.json:SHA [--base C=NAME ...] --out FILE``
"""

import argparse
import hashlib
import json
from pathlib import Path

# Device and host GiB declared per judged arm (measured peaks rounded up with headroom).
MEMORY = {"C": (10, 16), "L": (18, 24), "F": (3, 8)}
REPOSITORIES = "airk,bookbuilder"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def arm_row(arm, series, r_root, image, pair, pair_sha, base=None, seconds=21000):
    require(arm in MEMORY, f"unknown judged arm {arm}")
    require(Path(pair).is_file() and sha256(pair) == pair_sha, f"pair of arm {arm} changed: {pair}")
    profiles = json.loads(Path(pair).read_bytes())["profiles"]
    stages = {row["stage"] for row in profiles.values() if "model" in row}
    require("focus" in stages, f"arm {arm} needs a pair with a CUDA Focus entry")
    require(arm == "F" or "context" in stages, f"arm {arm} needs a CUDA Context entry")
    start = ["--image", str(image), "--model", "pair", "--pair", str(pair), "--pair-sha256", pair_sha,
             "--repositories", REPOSITORIES, "--seconds", str(seconds)]
    start += ["--restart-from", base] if base else ["--seed-cpu", "focus"]
    device, host = MEMORY[arm]
    return {"launcher": "gpu_fixture.py", "start_args": start, "device_gib": device, "host_gib": host,
            "fixtures": str(Path(r_root) / f"fixture-{series}-{arm}"), "lifetime": f"{series}-{arm}"}


def build(series, r_root, image, pairs, bases):
    require(set(pairs) == set(MEMORY), "pairs for C, L and F are required")
    out = {arm: arm_row(arm, series, r_root, image, *pairs[arm], base=bases.get(arm)) for arm in sorted(pairs)}
    roots = [row["fixtures"] for row in out.values()]
    require(len(set(roots)) == len(roots), "every arm needs its own fixture root")
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--series", required=True)
    parser.add_argument("--r-root", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--pair", action="append", required=True, help="ARM=path:sha256")
    parser.add_argument("--base", action="append", default=[], help="ARM=closed lifetime of that root")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    pairs = {}
    for value in args.pair:
        arm, _, rest = value.partition("=")
        path, _, digest = rest.rpartition(":")
        pairs[arm] = (path, digest)
    bases = dict(value.partition("=")[::2] for value in args.base)
    data = (json.dumps(build(args.series, args.r_root, args.image, pairs, bases), indent=2, sort_keys=True)
            + "\n").encode()
    with open(args.out, "xb") as stream:
        stream.write(data)
    print(json.dumps({"lifetimes": args.out, "sha256": hashlib.sha256(data).hexdigest()}))


if __name__ == "__main__":
    main()
