# SPDX-License-Identifier: Apache-2.0
"""Inputs of a CUDA pair lifetime (plan 12.5.2 item 4): asset directories and the pair spec.

Evaluation tooling, never product. Runs as the verified human with ``python -I -B``.

``assets`` builds one profile's asset directory from its asset map by copying
each declared asset. Hard links cannot be used: the private fixture opens every
input through a rooted directory that refuses a file with more than one link
("rooted file has aliases"), and a link would also alias the original, which
other lifetimes then could not open. Every asset is authenticated first (under
``artifacts``, the profile's declared size and SHA-256, a single link), copied,
and authenticated again as copied. The output is a fresh directory holding
exactly the profile's asset names, plus a receipt beside it
(``<dir>.receipt.json``) that records the bytes copied. Budget the copy first.

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
review; it names ``parity``/``parity_sha256``, the ``l5c_focus_parity`` receipt
against its CPU twin. The service verifies its placement against the live
session and re-checks the parity receipt.
"""

import argparse
import hashlib
import json
import os
import shutil
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


def links(path):
    return os.stat(path).st_nlink


def build_assets(profile_path, profile_sha256, map_path, map_sha256, out, artifacts=ARTIFACTS):
    """Copies every declared asset into the fresh directory `out`, authenticated before and after."""
    profile = read_pinned(profile_path, profile_sha256)
    mapping = read_pinned(map_path, map_sha256)
    out = Path(out)
    require(under(out.parent, artifacts) and not out.exists(), "the asset directory must be fresh and under artifacts")
    names = [asset["name"] for asset in profile["assets"]]
    require(len(set(names)) == len(names) and set(names) <= set(mapping), "the asset map must name every asset")
    for name in names:
        # A plain file name only: no directory part, drive or parent step.
        require(isinstance(name, str) and name and Path(name).name == name and not os.path.splitdrive(name)[0]
                and name not in (".", ".."), f"asset name {name!r} is not a plain file name")
    sources = {}
    for asset in profile["assets"]:
        source = Path(mapping[asset["name"]].removeprefix("\\\\?\\"))
        require(source.is_file() and under(source, artifacts), f"asset {asset['name']} is not a file under artifacts")
        require(links(source) == 1, f"asset {asset['name']} has another link; the fixture refuses aliased files")
        require(source.stat().st_size == asset["bytes"] and sha256(source) == asset["sha256"],
                f"asset {asset['name']} differs from its declaration")
        sources[asset["name"]] = source
    out.mkdir()
    rows, copied = [], 0
    try:
        for asset in profile["assets"]:
            target = out / asset["name"]
            with open(sources[asset["name"]], "rb") as reader, open(target, "xb") as writer:
                shutil.copyfileobj(reader, writer, 4 << 20)
            require(target.stat().st_size == asset["bytes"] and sha256(target) == asset["sha256"]
                    and links(target) == 1 and links(sources[asset["name"]]) == 1,
                    f"copy of {asset['name']} differs from its declaration")
            copied += asset["bytes"]
            rows.append({"name": asset["name"], "role": asset["role"], "bytes": asset["bytes"],
                         "sha256": asset["sha256"], "source": str(sources[asset["name"]]), "copy": str(target)})
    except BaseException as error:
        # The named cleanup point: remove the partial copy and keep a receipt of why.
        shutil.rmtree(out, ignore_errors=True)
        write_new(out.parent / (out.name + ".failed.json"),
                  {"schema": ASSETS_SCHEMA + "/failed", "directory": str(out), "removed": not out.exists(),
                   "copied_bytes_before_failure": copied, "error": f"{type(error).__name__}: {error}"})
        raise
    receipt = {"schema": ASSETS_SCHEMA, "profile": {"path": str(profile_path), "sha256": profile_sha256,
               "manifest": profile["manifest"], "id": profile["profile_id"], "model": profile["model"]},
               "asset_map": {"path": str(map_path), "sha256": map_sha256}, "directory": str(out),
               "method": "copy", "copied_bytes": copied, "assets": rows}
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
        require(set(entry) == {"profile", "profile_sha256", "assets", "parity", "parity_sha256"},
                "a Focus entry names its parity receipt and no review")
        require(device.get("backend") == "cuda" and (device.get("placement") or {}).get("mode") == "strict_gpu",
                "a CUDA Focus entry is a strict-GPU profile")
        parity = read_pinned(entry["parity"], entry["parity_sha256"])
        require(parity.get("schema") == "airk.private.l5c-focus-parity/1" and parity.get("complete") is True
                and parity.get("passed") is True and (parity.get("cuda") or {}).get("manifest") == profile["manifest"]
                and (parity.get("tolerance") or 1) <= 1e-3 and (parity.get("row_count") or 0) >= 8
                and parity.get("winners_agree") == parity.get("row_count"),
                "the Focus parity receipt is not a passing parity of this profile")
        return profile, row, {"path": str(entry["parity"]), "sha256": entry["parity_sha256"]}, None
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
    focus_parity = None
    for name, entry in sorted(entries.items()):
        profile, row, pin, uuid = entry_input(entry)
        model = MODELS.get(profile["model"])
        require(model is not None and model not in models, "at most one Compact, one Large and one Focus entry")
        models.add(model)
        roster.append(row)
        stage = "focus" if model == "focus" else "context"
        if stage == "focus":
            focus_parity = pin
        elif pin is not None:
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
    if focus_parity is not None:
        pair["focus_parity"] = focus_parity
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
                          "copied_bytes": receipt["copied_bytes"], "receipt_sha256": digest}))
    else:
        entries = parse_named(args.entry, lambda path: json.loads(Path(path).read_bytes()))
        rows = parse_named(args.row, lambda text: dict(zip(("stage", "manifest"), text.split(":", 1))))
        pair, digest = build_pair(entries, rows, args.out)
        print(json.dumps({"pair": str(Path(args.out) / "pair.json"), "pair_sha256": digest,
                          "profiles": sorted(pair["profiles"])}))


if __name__ == "__main__":
    main()
