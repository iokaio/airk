# SPDX-License-Identifier: Apache-2.0
"""Reopen a complete checked AWS source tree and bind the disjoint case rosters."""
import hashlib
import json
from pathlib import Path
import runpy
import stat
import sys

HERE = Path(__file__).resolve().parent
menu = runpy.run_path(str(HERE / "verify_aws_natural_menu.py"))
native = menu["native"]
require = native["require"]
campaign = native["CAMPAIGN"]


def verify():
    inventory_path = campaign / menu["INVENTORY"]
    native["checked"](inventory_path, menu["INVENTORY_SHA"])
    inventory = json.loads(inventory_path.read_bytes())
    name, groups, members = menu["inputs"](inventory)
    one, = groups
    active = {json.dumps(c["selector"], sort_keys=True) for c in one["cases"]}
    repeat_path = campaign / "aws-repeated-answer-locations-r1.json"
    repeat_raw = repeat_path.read_bytes()
    excluded_sha = hashlib.sha256(repeat_raw).hexdigest()
    require(excluded_sha == "4cf5c6484e736ca176333515c7a063e312222e832fae9d2b9b6949dd255feeba",
            "original excluded AWS selectors or occurrence evidence changed")
    repeated = json.loads(repeat_raw)
    excluded = {json.dumps(c["selector"], sort_keys=True) for c in repeated["cases"]}
    require(len(active) == 41 and len(excluded) == len(repeated["cases"]) == 24 and not active & excluded,
            "incorrect original source case populations")
    require(repeated["source_revision"] == inventory["revision"] and repeated["occurrence_count"] == 485
            and all(c["publisher_selected_range"] is None for c in repeated["cases"]),
            "ambiguous locations or original source identity changed")
    tree = campaign / name
    files = {}
    for p in tree.rglob("*"):
        meta = p.lstat()
        require(not meta.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT, "reparse entry in checked source")
        if stat.S_ISDIR(meta.st_mode):
            continue
        require(stat.S_ISREG(meta.st_mode) and meta.st_nlink == 1, "nonregular or linked source")
        rel = p.relative_to(tree).as_posix()
        require(rel in members, "unknown file in expanded source root")
        with p.open("rb") as stream:
            sha = hashlib.file_digest(stream, "sha256").hexdigest()
        require(sha == members[rel]["sha256"]
                and meta.st_size == inventory["source_files"]["documents/" + rel]["bytes"],
                "expanded source location, size or SHA-256 differs")
        files[rel] = meta.st_size
    require(files.keys() == members.keys(), "original file missing from expanded tree")
    return {"schema": "airk.private.aws-expanded-source-verification/1", "repository_revision": inventory["revision"],
            "source_tree": str(tree), "source_files_rehashed": len(files), "source_bytes": sum(files.values()),
            "guide_families": len(inventory["families"]), "eligible_selectors": len(active),
            "excluded_ambiguous_selectors": len(excluded), "excluded_occurrences": repeated["occurrence_count"],
            "selector_overlap": 0, "known_positive_paths": len({c["source_path"] for c in one["cases"]}),
            "canonical_original_scope": "documents/", "active_roster_sha256": inventory["active_roster_sha256"],
            "ambiguous_roster_sha256": excluded_sha,
            "inventory_sha256": menu["INVENTORY_SHA"], "source_pack_receipt_sha256": menu["PACK_SHA"],
            "source_rights_sha256": menu["RIGHTS_SHA"], "native_visible_paths_measured": False,
            "paired_trials": 0, "model_requests": 0}


if __name__ == "__main__":
    dst = Path(sys.argv[1])
    require(dst.parent == campaign and dst.name == "aws-expanded-source-verification-r1.private.json",
            "checked original campaign output required")
    data = verify()
    with dst.open("x", encoding="utf-8") as stream:
        json.dump(data, stream, sort_keys=True, separators=(",", ":"))
    print(data["source_files_rehashed"], data["source_bytes"], data["eligible_selectors"], data["excluded_ambiguous_selectors"])
