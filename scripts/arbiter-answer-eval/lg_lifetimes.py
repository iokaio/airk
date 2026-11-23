# SPDX-License-Identifier: Apache-2.0
"""The LG lifetimes file for ``make_loop_spec.py gate --lifetimes`` (changes entry 41).

Evaluation tooling, never product. Writes ``{arm: {launcher, start_args, device_gib, host_gib,
fixtures, lifetime}}`` for the judged arms C (Focus + Compact FP16), L (Focus + Large FP16) and
F (Focus only), each in its own fixture root, and optionally the precision arms.

Every CUDA arm names a pair written by ``cuda_pair.py pair`` and pinned by SHA-256 here; arm F
reuses a pair that holds a Context entry (the review pin supplies the adapter) but stages only
Focus. A fresh root starts with ``--seed-cpu focus`` and indexes the repositories during start;
``--base NAME`` restarts from a closed lifetime of that root instead.

A precision arm (C32, L32) gets a row when it names its base (``--base C32=NAME``): it needs no pair,
because ``gpu_fixture.py --model compact|large`` attaches the reviewed FP32 single-entry model, and it
always restarts from its own root's base.

The lifetimes index the loop pair: bookbuilder-v2 (which serves the bookbuilder scenarios through
the root's ``index-aliases.json``) and munarium. No lifetime holds an airk index (user direction,
22 November 2026: the index under test holds nothing about the experiment).

Memory is declared from the A6000 measurements (GPU_FIXTURE.md, changes entries 17, 20 and 29):
GPU peaks Focus 1,024 W8 1.8 GiB (2.7 GB process peak in the parity run), Compact FP16 6.0 GiB,
Large FP16 14.0 GiB; host working-set peak about 17.5 GB with Large. Each figure is rounded up
with headroom. The services' own private host reservations are larger (32 GiB Compact, 40 GiB
Large, lifecycle constants) and are accounting inside each service, not resident memory.

``python -I -B lg_lifetimes.py --series 06-gate-lg --r-root R --image DIR --pair C=pair.json:SHA
--pair L=pair.json:SHA --pair F=pair.json:SHA --base C=06-gate-lg-C-base --base L=06-gate-lg-L-base
--base F=06-gate-lg-F-base [--base C32=06-gate-lg-C32-base --base L32=06-gate-lg-L32-base] --out FILE``
"""

import argparse
import hashlib
import json
from pathlib import Path

# Device and host GiB declared per judged arm (measured peaks rounded up with headroom).
MEMORY = {"C": (10, 16), "L": (18, 24), "F": (3, 8)}
# The index names of every LG lifetime, in fixture.py's order (the repository with denies first). The
# bookbuilder scenarios are served by the bookbuilder-v2 index through index-aliases.json, copied into each
# root. Scenario repositories (what the settled guard checks) are common.REPOS: munarium and bookbuilder.
REPOSITORIES = "bookbuilder-v2,munarium"
SCENARIO_REPOSITORIES = "munarium,bookbuilder"
FORBIDDEN_INDEX = "airk"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def arm_row(arm, series, r_root, image, pair, pair_sha, base=None, seconds=21000, root_arm=None):
    """One judged arm's lifetime row. ``root_arm`` names another arm's root to run in: a spare root
    (``--root L=C32``), used when the arm's own root has spent its one CUDA lifetime. A base is
    model-agnostic, so any closed base of the pair's indexes serves any arm; the lifetime keeps the arm's
    name, ``<series>-<arm>``, and ``base`` must then be the spare root's base."""
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
    require(root_arm is None or bool(base), f"arm {arm} in the root of {root_arm} needs that root's base")
    fixtures = Path(r_root) / f"fixture-{series}-{root_arm or arm}"
    if base:
        check_base(fixtures, base)
        used = cuda_lifetimes(fixtures)
        require(not used, f"{fixtures} has already spent its one CUDA lifetime ({', '.join(used)}): "
                          "a restart needs the saved configuration unchanged, so use a spare root (--root)")
    return {"launcher": "gpu_fixture.py", "start_args": start, "device_gib": device, "host_gib": host,
            "fixtures": str(fixtures), "lifetime": f"{series}-{arm}", "repositories": SCENARIO_REPOSITORIES}


def cuda_lifetimes(fixtures):
    """The CUDA lifetimes a root has had: lifetime folders that hold ``gpu.json`` (``gpu_fixture.py`` writes
    it at start). A root allows one, because a restart needs the saved configuration unchanged and a CUDA
    lifetime changes it; a failed start counts too, since it may already have configured the installation."""
    return sorted(path.parent.name for path in Path(fixtures).glob("*/gpu.json"))


def check_base(fixtures, base):
    """A base is a restart origin only when its lifetime closed cleanly, is restartable and holds exactly the
    loop pair's indexes (no airk index), with the alias file that serves the bookbuilder scenarios."""
    handle_path = Path(fixtures) / base / "handle.json"
    require(handle_path.is_file(), f"base lifetime {base} has no handle under {fixtures}")
    handle = json.loads(handle_path.read_bytes())
    require(handle.get("state") == "closed" and handle.get("restartable") is True and not handle.get("error"),
            f"base lifetime {base} is not a clean restart origin (state {handle.get('state')!r}, "
            f"restartable {handle.get('restartable')!r}, error {handle.get('error')!r})")
    indexes = sorted(handle.get("indexes") or {})
    require(FORBIDDEN_INDEX not in indexes, f"base lifetime {base} holds an {FORBIDDEN_INDEX} index")
    require(indexes == sorted(REPOSITORIES.split(",")), f"base lifetime {base} holds {indexes}, not {REPOSITORIES}")
    aliases_path = Path(fixtures) / "index-aliases.json"
    require(aliases_path.is_file(), f"{fixtures} has no index-aliases.json")
    raw = json.loads(aliases_path.read_bytes())
    aliases = raw.get("aliases", raw)
    require(aliases.get("bookbuilder") == "bookbuilder-v2", f"{aliases_path} does not serve bookbuilder by bookbuilder-v2")


# The FP32 precision arms (plan 12.7.3): gpu_fixture.py's single-entry FP32 CUDA models, one arm per lifetime,
# each in its own root and restarted from that root's base. Device and host GiB are the FP32 envelopes of
# make_loop_spec.LIFETIME_DEFAULTS (GPU_FIXTURE.md: Large about 40 GiB host and a 44 GiB device envelope).
PRECISION = {"C32": ("compact", 16, 16), "L32": ("large", 44, 40)}


def precision_row(arm, series, r_root, image, base, seconds=21000):
    """A precision arm's lifetime row. It names no pair: ``--model compact|large`` attaches the reviewed FP32
    entry. It always restarts from its base, because a fresh GPU lifetime would index during start."""
    require(arm in PRECISION, f"unknown precision arm {arm}")
    require(bool(base), f"precision arm {arm} needs its base lifetime (--base {arm}=NAME)")
    model, device, host = PRECISION[arm]
    fixtures = Path(r_root) / f"fixture-{series}-{arm}"
    check_base(fixtures, base)
    start = ["--image", str(image), "--model", model, "--repositories", REPOSITORIES, "--seconds", str(seconds),
             "--restart-from", base]
    return {"launcher": "gpu_fixture.py", "start_args": start, "device_gib": device, "host_gib": host,
            "fixtures": str(fixtures), "lifetime": f"{series}-{arm}", "repositories": SCENARIO_REPOSITORIES}


def build(series, r_root, image, pairs, bases, roots=None):
    """Rows for the judged arms C, L and F (a pair each) and for every precision arm that names a base.
    ``roots`` maps a judged arm to the arm whose root it runs in (a spare root, ``--root L=C32``)."""
    roots = dict(roots or {})
    require(set(pairs) == set(MEMORY), "pairs for C, L and F are required")
    unknown = sorted(set(bases) - set(MEMORY) - set(PRECISION))
    require(not unknown, f"unknown arms in --base: {unknown}")
    require(set(roots) <= set(MEMORY) and set(roots.values()) <= set(MEMORY) | set(PRECISION),
            f"--root maps a judged arm to an arm's root, not {roots}")
    out = {arm: arm_row(arm, series, r_root, image, *pairs[arm], base=bases.get(arm), root_arm=roots.get(arm))
           for arm in sorted(pairs)}
    for arm in sorted(PRECISION):
        if arm in bases:
            out[arm] = precision_row(arm, series, r_root, image, bases[arm])
    roots = [row["fixtures"] for row in out.values()]
    require(len(set(roots)) == len(roots), "every arm needs its own fixture root")
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--series", required=True)
    parser.add_argument("--r-root", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--pair", action="append", required=True, help="ARM=path:sha256")
    parser.add_argument("--base", action="append", default=[], help="ARM=closed lifetime of that arm's root")
    parser.add_argument("--root", action="append", default=[],
                        help="ARM=ROOTARM: run a judged arm in another arm's (spare) root; --base ARM= then "
                             "names that root's base")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    pairs = {}
    for value in args.pair:
        arm, _, rest = value.partition("=")
        path, _, digest = rest.rpartition(":")
        pairs[arm] = (path, digest)
    bases = dict(value.partition("=")[::2] for value in args.base)
    roots = dict(value.partition("=")[::2] for value in args.root)
    data = (json.dumps(build(args.series, args.r_root, args.image, pairs, bases, roots), indent=2, sort_keys=True)
            + "\n").encode()
    with open(args.out, "xb") as stream:
        stream.write(data)
    print(json.dumps({"lifetimes": args.out, "sha256": hashlib.sha256(data).hexdigest()}))


if __name__ == "__main__":
    main()
