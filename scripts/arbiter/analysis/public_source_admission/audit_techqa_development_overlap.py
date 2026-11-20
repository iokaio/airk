# SPDX-License-Identifier: Apache-2.0
"""Compare source identities across frozen development menus without opening questions or answers."""
import json
from pathlib import Path
import runpy


HERE = Path(__file__).resolve().parent
development = runpy.run_path(str(HERE.parent / "development_margins/capture_inputs.py"))
groups = runpy.run_path(str(HERE / "audit_active_confirmation_groups.py"))["groups"]
load = development["load"]
require = development["require"]


def main():
    campaign = development["CAMPAIGN"]
    corpus_sha = "34109e91b1aa8acd13af56fc706ef3f73006502e3fdffed23843d13b263d1821"
    exposure_sha = "2de707182f15be6e486a0e0fea004a52575e232a1db4caa4f3ad7d9ef992657c"
    corpus = load(campaign / "techqa-full-menu-corpus-r1/manifest.private.json", corpus_sha)
    exposure = load(campaign / "techqa-menu-train-exposure-r1.json", exposure_sha)
    train = load(development["SOURCES"] / "manifest.private.json", development["SOURCES_SHA"])
    menus = corpus["ordered_menus"]
    documents = corpus["documents"]
    require(set(menus) == set(exposure["cases"]) and len(menus) == 76 and
            all(len(ids) == len(set(ids)) == 50 for ids in menus.values()), "frozen source membership changed")
    require(len(train["files"]) == 750 and len({r["case"] for r in train["files"]}) == 15,
            "TRAIN membership changed")
    # Either a shared publisher ID or identical original bytes links two cases.
    def keys(identifier, digest):
        return (("publisher_id", identifier), ("content_sha256", digest))
    source_keys = {case: {key for doc in ids for key in keys(doc, documents[doc]["sha256"])}
                   for case, ids in menus.items()}
    train_keys = {key for row in train["files"] for key in keys(row["id"], row["sha256"])}
    components = groups(source_keys.items())
    direct = {case for case, identities in source_keys.items() if identities & train_keys}
    exposed = {"DEV_Q002"}
    affected = [group for group in components if set(group) & (direct | exposed)]
    remaining = [group for group in components if group not in affected]
    remainder = [case for group in remaining for case in group]
    product_overlap = {kind: sum(bool(exposure["cases"][case]["menu_matches"].get(kind)) for case in remainder)
                       for kind in ("literal_product_id", "literal_product_name")}
    print(json.dumps({"full_menu_sha256": corpus_sha, "train_source_sha256": development["SOURCES_SHA"],
                      "prior_exposure_sha256": exposure_sha, "screened": len(menus),
                      "development_case_ids": sorted(exposed), "direct_train_matches": sorted(direct),
                      "exposed_source_components": affected, "remaining_components": remaining,
                      "remaining_requests": len(remainder), "remaining_menu_product_overlap": product_overlap,
                      "confirmed_heldout_requests": None, "judged_paired_outcomes": 0}, sort_keys=True))


if __name__ == "__main__":
    main()
