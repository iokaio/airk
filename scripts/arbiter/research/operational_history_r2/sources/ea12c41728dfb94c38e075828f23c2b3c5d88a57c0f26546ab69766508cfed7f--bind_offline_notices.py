# SPDX-License-Identifier: Apache-2.0
"""Bind explicit full-document selections to genuine native final plan identities."""
import argparse
import json
from pathlib import Path
import package_offline_bundles as package


def bind(plan_path, plan_pin, selections_path, selections_pin, output, anchor):
    owner = package.owner
    anchor, output = Path(anchor).absolute(), Path(output).absolute()
    if output.exists() or ".." in output.parts or not output.is_relative_to(anchor):
        raise ValueError("fresh notice mapping under named anchor required")
    owner.no_alias(output.parent, anchor)
    _, plan = owner.pinned_json(plan_path, plan_pin, anchor)
    _, selection = owner.pinned_json(selections_path, selections_pin, anchor)
    bundles = plan.get("bundles", [])
    if (plan.get("schema") != "airk.arbiter.offline-bundle-plan/1" or len(bundles) != 3
            or not owner.HEX.fullmatch(plan.get("promotion_decision_sha256", ""))
            or selection.get("schema") != "airk.private-offline-notice-selections/1"):
        raise ValueError("native final plan and explicit document selection required")
    identities, models, files = set(), set(), {}
    for row in bundles:
        identity, model = row.get("profile", ""), row.get("model")
        if (not owner.HEX.fullmatch(identity) or identity in identities or model in models
                or not owner.HEX.fullmatch(row.get("qualification", ""))):
            raise ValueError("duplicate/invalid final identity or missing qualification")
        identities.add(identity)
        models.add(model)
        files[identity] = selection.get("files", {}).get(model, [])
    if models != set(selection.get("files", {})):
        raise ValueError("exact final native model/document selection census required")
    mapping = {"schema": "airk.private-offline-notice-mapping/1", "plan_sha256": plan_pin,
               "selection_sha256": selections_pin, "files": files}
    raw = (json.dumps(mapping, indent=2) + "\n").encode("utf-8")
    # Validate full document pins before publishing the exclusive local mapping.
    package.validate_notices(mapping, plan_pin, identities, anchor)
    owner.write_new(output, raw)
    return {"mapping_sha256": owner.digest(output), "plan_sha256": plan_pin,
            "selection_sha256": selections_pin, "profiles": len(files),
            "qualified": False, "approved": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("plan", "selections", "output", "anchor"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("plan", "selections"):
        parser.add_argument("--" + name + "-sha256", required=True)
    args = parser.parse_args()
    print(json.dumps(bind(args.plan, args.plan_sha256, args.selections, args.selections_sha256,
                          args.output, args.anchor)))


if __name__ == "__main__":
    main()
