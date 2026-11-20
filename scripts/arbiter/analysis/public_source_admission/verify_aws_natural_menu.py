# SPDX-License-Identifier: Apache-2.0
"""Capture complete paginated source visibility for the shared original AWS corpus."""
import json
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
native = runpy.run_path(str(HERE / "native_cohort_source_reads.py"))
require = native["require"]
INVENTORY = "aws-natural-menu-inventory-r1.private.json"
INVENTORY_SHA = "adea1f245f326d7814c011eb9538df1d9fd4343cec2cfc50aedbb45885831c65"
PACK_SHA = "065678389e33c963d9f011b5d6f5d6f1cb6a34c42e3b1b3b573a2372017c1fb3"
RIGHTS_SHA = "f2b1f170d4fca4cc2f23fe0976a0f868778ea7971680fe0b0904736a0b0691d1"

COHORT = native["Cohort"](
    name="aws-natural", corpus_directory=".", manifest_relative_path=INVENTORY,
    manifest_sha256=INVENTORY_SHA, case_count=1, document_count=35515, selector_field="menu",
    receipt_identity={"active_question_count": 41, "source_menu": "documents/",
                      "source_pack_receipt_sha256": PACK_SHA, "source_notice_lineage_sha256": RIGHTS_SHA},
)


def inputs(manifest):
    campaign = native["CAMPAIGN"]
    pack_file = campaign / "aws-natural-source-pack-r1/pack.private.json"
    rights_file = campaign / "aws-source-notice-lineage-r1.private.json"
    native["checked"](pack_file, PACK_SHA)
    native["checked"](rights_file, RIGHTS_SHA)
    hashes = json.loads(pack_file.read_bytes())["files"]
    rights = json.loads(rights_file.read_bytes())["source_to_notice_scope"]
    require(rights.keys() == hashes.keys() == manifest["source_files"].keys(), "source/rights inventories differ")
    members = {name.removeprefix("documents/"): {"sha256": digest} for name, digest in hashes.items()}
    cases = manifest["cases"]
    require(len(cases) == 41 and all(c["menu_scope"] == "documents/" for c in cases), "natural scope differs")
    return "aws-docs-full-r1", [{"selector": {"menu": "documents/"}, "cases": cases, "members": members}], members


def observe(cohort, cases, tree, call_tool):
    case, = cases
    status = call_tool("source-status", "airk_status", {"response_mode": "full", "include_status": True})
    scope = status["scope_observation"]
    count = status["files"]
    require(type(count) is int and 0 < count <= cohort.document_count, "invalid native file count")
    paths = []
    seen = set()
    cursors = set()
    args = {"prefix": "", "limit": 100, "max_bytes": 65536, "response_mode": "full", "include_status": True}
    while True:
        page = call_tool(f"source-page-{len(cursors):04d}", "airk_list_files", args)
        rows = page["paths"]
        require(scope == page["scope_observation"] and isinstance(rows, list) and 0 < len(rows) <= 100,
                "empty/oversized page or changed native scope")
        require(all(p in case["members"] and p not in seen for p in rows) and len(set(rows)) == len(rows),
                "duplicate or foreign native source path")
        paths.extend(rows)
        seen.update(rows)
        cursor = page.get("cursor")
        if not page["truncated"]:
            require(not cursor and len(paths) == count, "incomplete last native page")
            break
        require(len(paths) < count and isinstance(cursor, str) and cursor and cursor not in cursors,
                "invalid native continuation")
        cursors.add(cursor)
        args = {**args, "cursor": cursor}
    positive_paths = sorted({row["source_path"].removeprefix("documents/") for row in case["cases"]} & seen)
    positive_files = {}
    for position, path in enumerate(positive_paths):
        info = call_tool(f"original-positive-{position:02d}", "airk_file_info",
                         {"path": path, "response_mode": "full", "include_status": True})
        expected_hash = case["members"][path]["sha256"]
        require(info["scope_observation"] == scope and info["path"] == path
                and info["content_hash"] == expected_hash, "original positive differs in full native publication")
        positive_files[path] = expected_hash
    return [{"selector": case["selector"], "active_question_count": len(case["cases"]),
             "visible_positive_file_hashes": positive_files,
             "native_scope": scope, "source_path_prefix": "documents/", "native_visible_paths": paths,
             "source_absent_from_native_inventory": sorted(case["members"].keys() - seen),
             "question_source_visibility": [{"selector": row["selector"],
                                            "known_positive_visible": row["source_path"].removeprefix("documents/") in seen}
                                           for row in case["cases"]],
             "native_pages": len(cursors) + 1, "ranker_offer_observed": False}]


if __name__ == "__main__":
    native["run_cohort"](COHORT, Path(__file__).resolve(), prepare_inputs=inputs, observe=observe)
