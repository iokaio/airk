# SPDX-License-Identifier: Apache-2.0
"""Census the native-visible subset of every staged file at one fixed fd base."""
import argparse
import json
from pathlib import Path
import runpy


HERE = Path(__file__).resolve().parent
native = runpy.run_path(str(HERE / "native_cohort_source_reads.py"))
CAMPAIGN = native["CAMPAIGN"]
CORPUS = "contextbench-fd-whole-r1/sources"
MANIFEST = "0d8dec6fa1f7b2c16ea100e066936d4e84f19e8fd2a1b3bc40db0f731e7e0d8e"
require = native["require"]


def observe(cohort, cases, tree, call_tool):
    case, = cases
    rows = {row["path"]: row for row in case["documents"]}
    inventory = call_tool("list-full-base", "airk_list_files", {
        "prefix": "", "limit": 100, "max_bytes": 65536, "response_mode": "full", "include_status": True,
    })
    paths = inventory["paths"]
    require(not inventory["truncated"] and not inventory.get("cursor") and len(set(paths)) == len(paths)
            and set(paths) <= rows.keys(), "incomplete or foreign native inventory")
    status = call_tool("scope-status", "airk_status", {"response_mode": "full", "include_status": True})
    require(len(paths) == status["files"] and status["scope_observation"] == inventory["scope_observation"],
            "published file count or fixed scope differs")
    visible = []
    for position, path in enumerate(paths):
        entry = rows[path]
        info = call_tool(f"document-{position:02d}", "airk_file_info",
                         {"path": path, "response_mode": "full", "include_status": True})
        require(info["path"] == path and info["content_hash"] == entry["sha256"] and info["source_bytes"] == entry["bytes"]
                and info["scope_observation"] == inventory["scope_observation"], "native blob, bytes or scope differs")
        visible.append({"path": path, "oid": entry["oid"], "sha256": entry["sha256"], "bytes": entry["bytes"],
                        "chunks": info["chunks"], "native_extraction_method": info["extraction_method"],
                        "native_parse_status": info["parse_status"]})
    invisible = sorted(rows.keys() - set(paths))
    return [{"selector": case["selector"], "native_inventory_order": paths, "native_visible": visible,
             "source_files": len(rows), "absent_from_complete_native_list": invisible,
             "returned_scope": status["scope_observation"], "ranker_offer_observed": False}]


def main():
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--output", required=True, type=Path)
    args, _ = parser.parse_known_args()
    path = CAMPAIGN / CORPUS / "manifest.private.json"
    native["checked"](path, MANIFEST)
    manifest = json.loads(path.read_bytes())
    matching = [r for r in manifest["revisions"] if args.output.name.startswith("fd-" + r["base"][:8] + "-")]
    require(len(matching) == 1, "native capture target must identify one exact base")
    selected, = matching

    def prepare(observed):
        require(observed == manifest, "source manifest changed")
        docs = {r["path"]: r for r in selected["files"]}
        return selected["base"], [{"selector": {"base": selected["base"]}, "documents": selected["files"]}], docs

    cohort = native["Cohort"](name="fd-" + selected["base"][:8], corpus_directory=CORPUS,
                              manifest_sha256=MANIFEST, case_count=1, document_count=48, selector_field="base",
                              receipt_identity={"repo": selected["repo"], "base": selected["base"], "git_tree": selected["tree"]})
    native["run_cohort"](cohort, Path(__file__).resolve(), prepare_inputs=prepare, observe=observe)


if __name__ == "__main__":
    main()
