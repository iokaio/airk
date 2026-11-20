# SPDX-License-Identifier: Apache-2.0
"""Observe every document in one original TechQA menu through an isolated native index.

The first exact case was chosen by opaque ID before inspecting outcomes. Only
its 50 documents are mounted. The fixture executes inventory and metadata reads,
with no Answer, Focus, Context, search or scoring request.
"""
from pathlib import Path
import runpy

native = runpy.run_path(str(Path(__file__).with_name("native_cohort_source_reads.py")))
require = native["require"]
COHORT = native["Cohort"](
    name="techqa-menu", corpus_directory="techqa-full-menu-corpus-r1",
    manifest_sha256="34109e91b1aa8acd13af56fc706ef3f73006502e3fdffed23843d13b263d1821",
    case_count=1, document_count=50, selector_field="question_id",
    receipt_identity={"ordered_menu_freeze_sha256": "50f876ebb9cf3a8328314e7be3e01f343b2d1b67cd7979fe2aaceeb2c82064f7",
                      "observation": "complete source menu native inventory and document identities"},
)


def menu_inputs(manifest):
    case = manifest["pilot_case"]
    require(case == min(manifest["ordered_menus"]) == "DEV_Q002", "model-blind pilot selector differs")
    ids = manifest["ordered_menus"][case]
    require(len(ids) == len(set(ids)) == 50, "complete unique publisher menu required")
    tree = manifest["pilot_source_root"]
    source = {}
    documents = []
    for identifier in ids:
        descriptor = manifest["documents"][identifier]
        expected = identifier + ".txt"
        require(descriptor["path"] == tree + "/" + expected, "menu member is outside the pilot root")
        source[expected] = descriptor
        documents.append({"publisher_id": identifier, "path": expected,
                          "bytes": descriptor["bytes"], "sha256": descriptor["sha256"]})
    return tree, [{"selector": {"question_id": case}, "documents": documents}], source


def observe_menu(cohort, cases, tree, call_tool):
    case = cases[0]
    documents = case["documents"]
    inventory = call_tool("list-complete-menu", "airk_list_files", {
        "prefix": "", "limit": 100, "max_bytes": 65536, "include_status": True, "response_mode": "full",
    })
    paths = inventory["paths"]
    require(not inventory["truncated"] and not inventory.get("cursor") and len(paths) == cohort.document_count
            and set(paths) == {row["path"] for row in documents}, "native inventory differs from the publisher menu")
    ordered = []
    for position, doc in enumerate(documents):
        info = call_tool(f"file-{position:02}", "airk_file_info", {
            "path": doc["path"], "response_mode": "full", "include_status": True,
        })
        require(info["path"] == doc["path"] and info["source_bytes"] == doc["bytes"]
                and info["content_hash"] == doc["sha256"], "native bytes or path differ")
        ordered.append({"publisher_position_zero_based": position, **doc, "native_chunks": info["chunks"],
                        "native_extraction_method": info["extraction_method"], "native_parse_status": info["parse_status"]})
    return [{"selector": case["selector"], "native_inventory_order": paths,
             "ordered_documents": ordered, "answer_offer_captured": False}]


if __name__ == "__main__":
    native["run_cohort"](COHORT, Path(__file__).resolve(), prepare_inputs=menu_inputs, observe=observe_menu)
