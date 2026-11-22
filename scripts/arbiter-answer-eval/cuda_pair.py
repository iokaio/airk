# SPDX-License-Identifier: Apache-2.0
"""Inputs of a two-entry CUDA lifetime (plan 12.5.2 item 4): asset directories and the pair spec.

Evaluation tooling, never product. Runs as the verified human with ``python -I -B``.

``assets`` builds one profile's asset directory from its asset map by hard links
to the existing verified files, so no model byte is copied. Every asset is
authenticated first: the map's file must exist on the destination volume under
``artifacts`` and match the profile's declared size and SHA-256. The output is a
fresh directory holding exactly the profile's asset names, plus a receipt beside
it (``<dir>.receipt.json``).

``pair`` writes the pinned inputs of one lifetime with up to two CUDA Context
entries (one Compact, one Large), an optional CUDA Focus entry and optional
extra CUDA rows:

- ``roster.json``: the same_install CUDA roster (one ``{profile, profile_sha256,
  assets, fixtures}`` per entry);
- ``pair.json``: the roster pin, the protected review pins
  (``AIRK_ARBITER_EVALUATION_CUDA_REVIEWS``), the adapter and the profile rows
  ``gpu_fixture.py stage`` uses.

``python -I -B cuda_pair.py assets --profile P --profile-sha256 S --asset-map M --asset-map-sha256 S --out DIR``
``python -I -B cuda_pair.py pair --entry NAME=ENTRY.json ... [--row NAME=STAGE:MANIFEST ...] --out DIR``

An entry file names ``profile``/``profile_sha256``, ``assets`` (a directory built by
``assets``), ``review``/``review_sha256`` (the pass-1 graph diagnostic receipt) and
``pass1_profile_sha256``. A CUDA Focus entry (a strict-GPU profile) names no
review: the service verifies its placement against the live session.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
ARTIFACTS = ROOT / "artifacts"
PAIR_SCHEMA = "airk.private.answer-eval-cuda-pair/1"
ASSETS_SCHEMA = "airk.private.answer-eval-cuda-assets/1"
MODELS = {"arbiter-context-qwen3-17b": "compact", "arbiter-context-qwen3-4b": "large",
          "arbiter-focus-qwen3-reranker-06b": "focus"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path, chunk=4 << 20):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        while block := stream.read(chunk):
            digest.update(block)
    return digest.hexdigest()


def pinned(path, expected):
    path = Path(path)
    require(path.is_file() and sha256(path) == expected, f"pinned file changed: {path}")
    return path


def read_pinned(path, expected):
    return json.loads(pinned(path, expected).read_bytes())


def under(path, root):
    try:
        Path(path).resolve().relative_to(Path(root).resolve())
        return True
    except ValueError:
        return False


def write_new(path, value):
    data = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    with open(path, "xb") as stream:
        stream.write(data)
    return hashlib.sha256(data).hexdigest()


def build_assets(profile_path, profile_sha256, map_path, map_sha256, out, artifacts=ARTIFACTS, link=os.link):
    """Hard-links every declared asset into the fresh directory `out` after authenticating it."""
    profile = read_pinned(profile_path, profile_sha256)
    mapping = read_pinned(map_path, map_sha256)
    out = Path(out)
    require(under(out.parent, artifacts) and not out.exists(), "the asset directory must be fresh and under artifacts")
    names = [asset["name"] for asset in profile["assets"]]
    require(len(set(names)) == len(names) and set(names) <= set(mapping), "the asset map must name every asset")
    sources = {}
    for asset in profile["assets"]:
        source = Path(mapping[asset["name"]].removeprefix("\\\\?\\"))
        require(source.is_file() and under(source, artifacts), f"asset {asset['name']} is not a file under artifacts")
        require(os.path.splitdrive(source.resolve())[0].lower() == os.path.splitdrive(out.resolve())[0].lower(),
                f"asset {asset['name']} is on another volume; a hard link is impossible")
        require(source.stat().st_size == asset["bytes"] and sha256(source) == asset["sha256"],
                f"asset {asset['name']} differs from its declaration")
        sources[asset["name"]] = source
    out.mkdir()
    rows = []
    for asset in profile["assets"]:
        target = out / asset["name"]
        link(sources[asset["name"]], target)
        require(os.path.samefile(sources[asset["name"]], target) and target.stat().st_size == asset["bytes"],
                f"link {asset['name']} does not name its source")
        rows.append({"name": asset["name"], "role": asset["role"], "bytes": asset["bytes"], "sha256": asset["sha256"],
                     "source": str(sources[asset["name"]]), "link": str(target)})
    receipt = {"schema": ASSETS_SCHEMA, "profile": {"path": str(profile_path), "sha256": profile_sha256,
               "manifest": profile["manifest"], "id": profile["profile_id"], "model": profile["model"]},
               "asset_map": {"path": str(map_path), "sha256": map_sha256}, "directory": str(out),
               "method": "hard_link", "copied_bytes": 0, "assets": rows}
    receipt_sha = write_new(out.parent / (out.name + ".receipt.json"), receipt)
    return receipt, receipt_sha


def entry_input(entry):
    """One roster row and its review pin, from an entry spec. Everything is re-authenticated."""
    profile = read_pinned(entry["profile"], entry["profile_sha256"])
    assets = Path(entry["assets"])
    require(assets.is_dir() and under(assets, ARTIFACTS), "entry assets must be a directory under artifacts")
    fixture = None
    for asset in profile["assets"]:
        path = assets / asset["name"]
        require(path.is_file() and path.stat().st_size == asset["bytes"] and sha256(path) == asset["sha256"],
                f"entry asset {asset['name']} differs from its declaration")
        if asset["role"] == "fixture":
            fixture = path
    require(fixture is not None, "the profile declares no compiler fixture asset")
    row = {"profile": str(entry["profile"]), "profile_sha256": entry["profile_sha256"], "assets": str(assets),
           "fixtures": str(fixture)}
    device = profile.get("device_execution") or {}
    if MODELS.get(profile["model"]) == "focus":
        require(set(entry) == {"profile", "profile_sha256", "assets"}, "a Focus entry names no review")
        require(device.get("backend") == "cuda" and (device.get("placement") or {}).get("mode") == "strict_gpu",
                "a CUDA Focus entry is a strict-GPU profile")
        return profile, row, None, None
    review = read_pinned(entry["review"], entry["review_sha256"])
    reviewed = review.get("profile") or {}
    require(review.get("schema") == "airk.private.l5a-gpu/1" and review.get("step") == "graph"
            and review.get("complete") is True and review.get("cpu_payload_check") is None
            and "diagnostic_error" not in review and "error" not in review,
            "the review is not a complete, passing graph diagnostic")
    require(reviewed.get("sha256") == entry["pass1_profile_sha256"] and reviewed.get("id") == profile["profile_id"]
            and reviewed.get("model") == profile["model"], "the review is of another profile")
    before, after = reviewed.get("device_execution") or {}, profile.get("device_execution") or {}
    for key in set(before) | set(after):
        require(key in ("placement", "memory") or before.get(key) == after.get(key),
                f"the profile's device execution differs from its review in {key}")
    placement = after.get("placement") or {}
    require(placement.get("mode") == "bounded_hybrid" and placement.get("policy") == review.get("bounded_policy"),
            "the profile's placement does not name its reviewed policy")
    uuid = (review.get("adapter_before") or {}).get("uuid")
    require(isinstance(uuid, list) and len(uuid) == 16 and any(uuid), "the review names no adapter")
    pin = {"path": str(entry["review"]), "sha256": entry["review_sha256"],
           "profile_sha256": entry["pass1_profile_sha256"]}
    return profile, row, pin, uuid


def build_pair(entries, rows, out):
    """entries: {name: entry spec}; rows: {name: {"stage", "manifest"}} extra CUDA rows (such as Focus)."""
    out = Path(out)
    require(under(out.parent, ARTIFACTS) and not out.exists(), "the pair directory must be fresh and under artifacts")
    require(1 <= len(entries) <= 3, "a pair has one to three CUDA entries")
    roster, reviews, adapters, profile_rows, models = [], [], set(), {}, set()
    for name, entry in sorted(entries.items()):
        profile, row, pin, uuid = entry_input(entry)
        model = MODELS.get(profile["model"])
        require(model is not None and model not in models, "one Compact and one Large entry at most")
        models.add(model)
        roster.append(row)
        stage = "focus" if model == "focus" else "context"
        if pin is not None:
            # Review pins follow the Context entries in roster order, as the service zips them.
            reviews.append(pin)
            adapters.add(json.dumps(uuid))
        profile_rows[name] = {"stage": stage, "device": "cuda", "manifest": profile["manifest"], "model": model}
    require(len(adapters) == 1, "the Context entries must name one adapter")
    for name, row in rows.items():
        require(name not in profile_rows and row["stage"] in ("focus", "context") and len(row["manifest"]) == 64,
                f"row {name} is a new name with a stage and a manifest")
        profile_rows[name] = {"stage": row["stage"], "device": "cuda", "manifest": row["manifest"]}
    out.mkdir()
    roster_sha = write_new(out / "roster.json", roster)
    pair = {"schema": PAIR_SCHEMA, "roster": {"path": str(out / "roster.json"), "sha256": roster_sha},
            "reviews": reviews, "adapter_uuid": json.loads(adapters.pop()), "profiles": profile_rows}
    pair_sha = write_new(out / "pair.json", pair)
    return pair, pair_sha


def parse_named(values, parse):
    result = {}
    for value in values or []:
        name, _, rest = value.partition("=")
        require(name and rest and name not in result, f"bad or repeated NAME=VALUE: {value}")
        result[name] = parse(rest)
    return result


def main():
    global ARTIFACTS
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    sub = commands.add_parser("assets")
    # The campaign's artifacts root (default: this checkout's); a worktree under it names the main one.
    sub.add_argument("--artifacts", default=str(ARTIFACTS))
    for name in ("profile", "profile-sha256", "asset-map", "asset-map-sha256", "out"):
        sub.add_argument("--" + name, required=True)
    sub = commands.add_parser("pair")
    sub.add_argument("--artifacts", default=str(ARTIFACTS))
    sub.add_argument("--entry", action="append", required=True)
    sub.add_argument("--row", action="append")
    sub.add_argument("--out", required=True)
    args = parser.parse_args()
    ARTIFACTS = Path(args.artifacts)
    if args.command == "assets":
        receipt, digest = build_assets(args.profile, args.profile_sha256, args.asset_map, args.asset_map_sha256,
                                       args.out, artifacts=ARTIFACTS)
        print(json.dumps({"directory": receipt["directory"], "assets": len(receipt["assets"]),
                          "copied_bytes": 0, "receipt_sha256": digest}))
    else:
        entries = parse_named(args.entry, lambda path: json.loads(Path(path).read_bytes()))
        rows = parse_named(args.row, lambda text: dict(zip(("stage", "manifest"), text.split(":", 1))))
        pair, digest = build_pair(entries, rows, args.out)
        print(json.dumps({"pair": str(Path(args.out) / "pair.json"), "pair_sha256": digest,
                          "profiles": sorted(pair["profiles"])}))


if __name__ == "__main__":
    main()
