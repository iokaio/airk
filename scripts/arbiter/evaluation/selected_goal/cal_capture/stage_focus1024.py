# SPDX-License-Identifier: Apache-2.0
"""Stage the native static1024 descriptor overlay and unchanged CAL100 population."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys

from prepare import CAMPAIGN, CENSUS, pin, read

ORIGINAL_AUDIT = "39e36a6fcc40f28412ff97ede00917b3bcd5147675c70510691b01e901607287"
NUMERIC = "dd6d22fb2f17bb18b05b396b5e378405da6d53872e5fcff565c46ed08c7feed4"
FOCUS = "arbiter-focus-qwen3-reranker-06b"


def require(value, reason):
    if not value:
        raise ValueError(reason)


def seal(path):
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return pin(path, digest)


def create_json(path, value):
    with path.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")
    return seal(path)


def stage(metadata, bootstrap_digest, output):
    require(metadata.is_absolute() and metadata.resolve(strict=True) == metadata and
            metadata.parent == CAMPAIGN and output.is_absolute() and output.parent == CAMPAIGN and
            output.resolve(strict=True) == output and not any(output.iterdir()), "fresh checked campaign paths required")
    original = read(CAMPAIGN / "cal100-launch-input-audit-r1.json", ORIGINAL_AUDIT)
    require(original["census"]["sha256"] == CENSUS and original["phase_counts"] == [8] * 12 + [4],
            "fixed CAL-only population differs")
    for entry in [original["census"], *original["inputs"].values(), *original["compiled_sdk"].values()]:
        pin(entry["path"], entry["sha256"])
    bootstrap = read(metadata / "bootstrap-receipt.json", bootstrap_digest)
    profile_path = metadata / "experimental-profile.json"
    profile_ref = seal(profile_path)
    profile = read(profile_path, profile_ref["sha256"])
    require(bootstrap["schema"] == "airk.private.native-focus1024-bootstrap/1" and bootstrap["width"] == 1024 and
            bootstrap["original_manifest"] == original["profiles"][FOCUS] and bootstrap["independent_numeric_sha256"] == NUMERIC and
            bootstrap["release_qualified"] is False and profile["model"] == FOCUS and profile["max_tokens"] == 1024 and
            profile["compiler"] == bootstrap["compiler"] and profile["manifest"] == bootstrap["manifest"],
            "native Focus1024 issuer or independent proof differs")
    overlay = json.loads((metadata / "asset-files.private.json").read_bytes())
    declared = {asset["name"]: asset for asset in profile["assets"]}
    require(len(declared) == len(profile["assets"]) and set(declared) == set(overlay), "native asset overlay differs")
    assets = output / "assets"
    assets.mkdir()
    for name, asset in declared.items():
        source = Path(overlay[name])
        # Rust canonical paths on Windows may use the local extended-length prefix.
        prefix = "\\\\?\\"
        if str(source).startswith(prefix + CAMPAIGN.drive + "\\"):
            source = Path(str(source).removeprefix(prefix))
        require(Path(name).name == name and source.is_absolute() and source.is_relative_to(CAMPAIGN) and
                source.resolve(strict=True) == source and pin(source, asset["sha256"])["bytes"] == asset["bytes"],
                "asset path or physical identity differs")
        target = assets / name
        with target.open("xb") as destination, source.open("rb") as stream:
            shutil.copyfileobj(stream, destination, length=1 << 20)
        pin(target, asset["sha256"])
    original_roster = original["inputs"]["inputs"]
    rows = read(original_roster["path"], original_roster["sha256"])
    replaced = 0
    for index, row in enumerate(rows):
        old = read(row["profile"], row["profile_sha256"])
        require(old["manifest"] == original["profiles"][old["model"]], "original roster manifest differs")
        if old["model"] == FOCUS:
            fixture = [entry for entry in declared.values() if entry["role"] == "fixture"]
            require(len(fixture) == 1 and fixture[0]["name"] == "native-runtime1024-fixture.json",
                    "independent full-width fixture missing")
            rows[index] = {"profile": str(profile_path), "profile_sha256":profile_ref["sha256"],
                           "assets":str(assets), "fixtures":str(assets / fixture[0]["name"])}
            replaced += 1
    require(len(rows) == 3 and replaced == 1, "the selected roster must change one Focus descriptor")
    roster = create_json(output / "inputs.json", rows)
    original["inputs"]["inputs"] = roster
    original["profiles"][FOCUS] = profile["manifest"]
    original["fixture_environment"]["AIRK_ARBITER_EVALUATION_INPUTS"] = roster["path"]
    original["fixture_environment"]["AIRK_ARBITER_EVALUATION_INPUTS_SHA256"] = roster["sha256"]
    original["focus_static1024"] = {"bootstrap":pin(metadata / "bootstrap-receipt.json", bootstrap_digest),
                                    "resource_policy_digest":bootstrap["resource_policy_digest"],
                                    "original_audit_sha256": ORIGINAL_AUDIT}
    return create_json(output / "cal100-launch-audit.json", original)


if __name__ == "__main__":
    require(sys.dont_write_bytecode and sys.flags.no_site, "checked Python -B -S required")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--bootstrap-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(stage(args.metadata, args.bootstrap_sha256, args.output), sort_keys=True))
