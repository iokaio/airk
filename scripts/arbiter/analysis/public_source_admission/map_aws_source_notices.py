# SPDX-License-Identifier: Apache-2.0
"""Bind each original AWS source path to its inherited publisher notice evidence."""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[4]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
PACK = CAMPAIGN / "aws-natural-source-pack-r1/pack.private.json"
PACK_SHA = "065678389e33c963d9f011b5d6f5d6f1cb6a34c42e3b1b3b573a2372017c1fb3"


def publisher_terms(name, text):
    words = " ".join(text.lower().split())
    if name.lower().startswith("notice"):
        return "copyright_or_third_party_notice"
    if "attribution-noderivatives 4.0" in words:
        return "CC-BY-ND-4.0"
    if "attribution-sharealike 4.0" in words:
        if "sample code" in words:
            return "CC-BY-SA-4.0_documentation_and_separate_sample_code_terms"
        return "CC-BY-SA-4.0"
    if "apache license version 2.0" in words or "apache 2.0 license" in words:
        return "Apache-2.0"
    if "permission is hereby granted, free of charge" in words:
        if "above copyright notice" in words:
            return "MIT"
        return "MIT-0" if "software is provided" in words else "other_publisher_text"
    if "mit-0 license" in words:
        return "MIT-0"
    return "other_publisher_text"


def collect():
    raw = PACK.read_bytes()
    if hashlib.sha256(raw).hexdigest() != PACK_SHA:
        raise ValueError("Original source pack receipt differs")
    pack = json.loads(raw)
    notices = {}
    directories = defaultdict(list)
    for rows, field in [(pack["rights_materials"], "text"), (pack["readme_license_sections"], "license_section")]:
        for path, row in rows.items():
            item = PurePosixPath(path)
            if row["sha256"] != pack["files"][path]:
                raise ValueError("Notice differs from pinned corpus member")
            notices[path] = {"sha256": row["sha256"], "terms": publisher_terms(item.name, row[field])}
            directories[str(item.parent)].append(path)
    scopes = {}
    paths = {}
    scope_counts = Counter()
    for path in sorted(pack["files"]):
        ancestors = list(reversed(PurePosixPath(path).parents))
        lineage = tuple(str(p) for p in ancestors if str(p) in directories)
        if not lineage or len(PurePosixPath(lineage[0]).parts) != 2:
            raise ValueError("No publisher notice available at source family root")
        scope_id = lineage[-1]
        if scope_id not in scopes:
            scopes[scope_id] = {"publisher_notice_lineage": [
                {"directory": name, "notice_paths": sorted(directories[name])} for name in lineage
            ]}
        paths[path] = scope_id
        scope_counts[scope_id] += 1
    for name, count in scope_counts.items():
        scopes[name]["source_files"] = count
    if len(paths) != 35515 or len({p.split("/")[1] for p in paths}) != 226:
        raise ValueError("Incomplete source-to-notice map")
    return {"schema": "airk.private.aws-source-notice-lineage/1", "pack_receipt_sha256": PACK_SHA,
            "source_revision": pack["commit"], "source_to_notice_scope": paths, "notice_scopes": scopes,
            "notices": notices, "redistribution_authorized_by_this_receipt": False,
            "interpretation": "Directory ancestry identifies publisher evidence. Preserve nested notices, embedded "
                              "attribution and separate documentation/sample-code terms when applicable."}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    output = parser.parse_args().output
    if output.resolve().parent != CAMPAIGN:
        raise ValueError("Expected checked private receipt path")
    data = collect()
    with output.open("x", encoding="utf-8") as file:
        json.dump(data, file, sort_keys=True, separators=(",", ":"))
    print(json.dumps({"files": len(data["source_to_notice_scope"]), "scopes": len(data["notice_scopes"]),
                      "notice_paths": len(data["notices"]), "terms": dict(Counter(
                          item["terms"] for item in data["notices"].values()))}, sort_keys=True))
