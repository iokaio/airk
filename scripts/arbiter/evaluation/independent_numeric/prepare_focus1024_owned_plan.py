# SPDX-License-Identifier: Apache-2.0
"""Bind the original Windows CPU execution owner to a measured /3 Focus build."""
import argparse
import json
from pathlib import Path

from maintained_helpers import REPOSITORY, private_root, worker


ORIGINAL = REPOSITORY / "artifacts/advanced-split/models/actual-composed-service-r2/software-v2-native-execution-preparation-r3"
PLAN_SHA = {"resource": "85ac0eb791825e897ee3a4e4214d0398ad35bbf6d6dd3ebe8318bd7d33c0aaca",
            "numeric": "5849013fc957aab81a0e3de38a0c089e0c4c762d3a8d67b30672c6c7f91be7a7"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--mode", choices=PLAN_SHA, required=True)
    parser.add_argument("--request", type=Path, required=True)
    parser.add_argument("--request-sha256", required=True)
    args = parser.parse_args()
    work = private_root(args.work.resolve(strict=True))
    seals = {}

    def seal(path, sha=None, sha256=None):
        sha = sha or sha256
        pin = {"path": str(path), "sha256": sha}
        worker.pinned(pin)
        if seals.setdefault(str(path), sha) != sha:
            raise ValueError("incompatible seals for same file")
        return pin

    def read(pin):
        return json.loads(worker.pinned(pin).read_bytes())

    original = seal(ORIGINAL / "plans" / ("focus-" + args.mode + "-b2-r4.json"), PLAN_SHA[args.mode])
    old_plan = read(original)
    # All the original owner's own support sources remain independently pinned.
    for path, sha in old_plan["input_seals"].items():
        if Path(path).is_relative_to(ORIGINAL) and not Path(path).is_relative_to(ORIGINAL / "plans") and not Path(path).is_relative_to(ORIGINAL / "runs"):
            seal(path, sha)
    pid = worker.windows_sid()
    if pid != old_plan["human_sid"]:
        raise ValueError("different human")
    inventory = read(seal(work / "native-image-inventory.json", "3642c1cdf881ef8aa58a0cbaa152d3c451d8b54bf1bd34357c452c5850834593"))
    bridge = read(seal(work / "source-bridge.json", "b8dd809257639eecb23a4c7c395a620887f9acf3f398e1299b231e2adecf630a"))
    before = read(seal(**bridge["original_build_receipt"]))
    after = read(seal(**bridge["current_build_receipt"]))
    old_group, new_group = (build["groups"]["native"] for build in (before, after))
    if (inventory["actual_build_receipt"] != bridge["current_build_receipt"] or
            old_group["sha256"] != bridge["old_native_group"] or new_group["sha256"] != bridge["new_native_group"]):
        raise ValueError("bridge does not cover actual image")
    previous_image = Path(bridge["original_build_receipt"]["path"]).parent / "airk_inference.exe"
    previous = seal(previous_image, "dba73a2eccad3e0314fdb76a993938f6c1c2763f3aee9d7b0ca9c212a7cbae3f")
    request_pin = seal(args.request, args.request_sha256)
    request = read(request_pin)
    manifest = read(seal(**request["source_manifest"]))
    profile = read(seal(**request["profile"]))
    if (manifest["native_build_receipt"] != inventory["actual_build_receipt"] or
            request["policy"]["focus_pair_input_tokens"] != 1024 or profile["model"] != old_plan["model"] or
            profile["cpu_fit"] != old_plan["cpu_fit"] or profile["memory_reservation_bytes"] != old_plan["reservation_bytes"]):
        raise ValueError("wrong request, model, build or CPU fit")
    snapshot = Path(manifest["repository_root"])
    current = {}
    for item in manifest["source_files"]:
        seal(**item)
        relative = Path(item["path"]).relative_to(snapshot).as_posix()
        if new_group["files"].get(relative) != item["sha256"]:
            raise ValueError("source absent from actual native build")
        current[relative] = item
    unchanged = []
    for item in old_plan["unchanged_scoring_source_pins"]:
        key = Path(item["path"]).relative_to(REPOSITORY).as_posix()
        if old_group["files"][key] != current[key]["sha256"]:
            raise ValueError("original scoring source changed between these actual builds")
        unchanged.append(current[key])
    for asset in profile["assets"]:
        seal(request["files"][asset["name"]], asset["sha256"])
    roster = old_plan["native_operation_roster"]
    if args.mode == "numeric":
        fixture = read(seal(**request["numeric_fixture"]))
        metadata = read(seal(work / "assembled" / "assembly.json", worker.digest(work / "assembled" / "assembly.json")))
        if metadata["fixture"] != request["numeric_fixture"] or len(fixture["rows"]) != 14:
            raise ValueError("wrong independent numeric fixture")
        for pin in [fixture["reference_producer"], *fixture.get("original_reference_files", {}).values()]:
            if isinstance(pin, dict) and "path" in pin:
                seal(pin["path"], pin["sha256"])
        for kind in ("original_fp32", "same_artifact"):
            path = work / ("reference-" + kind) / "execution.json"
            execution = read(seal(path, worker.digest(path)))
            if not execution["complete"] or execution["torch_forward_invocations"] != 14:
                raise ValueError("incomplete original reference")
        roster = [roster[0]] + [{"kind": "numeric", "ordinal": index, "input_digest": row["input"]["input_digest"]}
                               for index, row in enumerate(fixture["rows"], 1)]
    elif request["numeric_fixture"] is not None:
        raise ValueError("unexpected resource fixture")
    plan = {key: old_plan[key] for key in ("schema", "mode", "tier", "model", "human_sid", "cpu_fit",
            "reservation_bytes", "limits", "command_selector", "environment", "owner_sources", "scope")}
    plan.update(profile=profile["manifest"], repository_root=str(snapshot), native_input=request_pin,
                policy=request["policy"], policy_digest=request["policy_digest"], native_operation_roster=roster,
                old_image_evidence={"image": previous}, old_native_build=bridge["original_build_receipt"],
                output_parent=str(work), unchanged_scoring_source_pins=unchanged,
                required_observer_source_changes={name: new_group["files"][name] for name in old_plan["required_observer_source_changes"]},
                input_seals=seals, qualification_granted=False, measurement_identity=old_plan["measurement_identity"])
    destination = work / ("owned-plan-" + args.mode + ".json")
    with destination.open("x", encoding="utf8") as out:
        json.dump(plan, out, indent=2)
        out.write("\n")
    print(worker.digest(destination))


if __name__ == "__main__":
    main()
