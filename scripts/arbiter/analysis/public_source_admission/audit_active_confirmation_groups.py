# SPDX-License-Identifier: Apache-2.0
"""Audit shared-source groups before assigning judged FP16 confirmation cases.

Run checked Python -I -B -S with --directory <campaign root>. Stdout is a
private receipt with case selectors and provisional connected components.
Membership records observed dependencies only. No scores or text are read.
"""

import argparse
from collections import defaultdict
from hashlib import sha256
import json
from pathlib import Path


TECHQA = "techqa-original-dev-ordered-menu-freeze-r1.json"
CLOUD = "techqa-style-ibm-mtrag-cloud-source-metadata-r1.json"
AWS = "aws-unique-source-corpus-r1/manifest.private.json"
BENCH = "repository-confirmation-multilang-r1/contextbench-14-declared-lines-r1.private.json"
DERIVED = "repository-confirmation-multilang-r1/contextbench-14-derived-lines-r1.private.json"
PINS = {
    TECHQA: "50f876ebb9cf3a8328314e7be3e01f343b2d1b67cd7979fe2aaceeb2c82064f7",
    CLOUD: "2bd2a972bf385d0732af3fa24a716b9f580e0ca8ec4887a2fcb47d8846493ee5",
    AWS: "ef2844ba2406de3b57ac1f5a6b13e582320bacae27779b704d38d4786375e882",
    BENCH: "a80f26b9026bf0ee3fdf3dc4f9c10523cb6bfa61b734dcd3d61f9b154636c66e",
    DERIVED: "84da2e363f508e2e1d138099b222f9a34a65c5cf17553b9c0f7786a52b5542b8",
}


def groups(cases):
    """Connect cases transitively across any shared source key."""
    case_ids = set()
    components = []
    for case, keys in cases:
        keys = set(keys)
        if case in case_ids or not keys or not all(keys):
            raise ValueError("duplicate case or missing source identity")
        case_ids.add(case)
        combined_cases, combined_keys = {case}, keys.copy()
        remaining = []
        for names, sources in components:
            if keys.isdisjoint(sources):
                remaining.append((names, sources))
            else:
                combined_cases.update(names)
                combined_keys.update(sources)
        components = [*remaining, (combined_cases, combined_keys)]
    return sorted((sorted(names) for names, _ in components), key=lambda names: (-len(names), names))


def contextbench_cases(source, derived):
    corrected = {(r["original"]["instance_id"], r["original"]["ordinal"])
                 for r in derived["rows"] if r["status"] == "strict_derived_match"}
    incomplete = set()
    repos = defaultdict(set)
    for row in source["rows"]:
        case = row["instance_id"]
        repos[case].add(row["repo"])
        if row["status"] not in {"exact_declared_lines", "exact_numbered_declared_lines"}:
            if (case, row["ordinal"]) not in corrected:
                incomplete.add(case)
    return repos, {case: repo for case, repo in repos.items() if case not in incomplete}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    opened = {}
    for name, pin in PINS.items():
        with (args.directory / name).open("rb") as stream:
            data = stream.read((16 << 20) + 1)
        if len(data) > 16 << 20 or sha256(data).hexdigest() != pin:
            raise ValueError("confirmation source metadata pin differs: " + name)
        opened[name] = json.loads(data)

    menus = opened[TECHQA]["menu_freeze"]
    active = {case: row for case, row in menus["cases"].items() if row["source_status"] == "exact_publisher_span"}
    cloud = opened[CLOUD]
    aws = opened[AWS]["cases"]
    bench, complete = contextbench_cases(opened[BENCH], opened[DERIVED])
    if tuple(map(len, (active, cloud["cases"], aws, bench, complete))) != (76, 25, 41, 14, 6):
        raise ValueError("active source case census differs")
    views = {
        "techqa_ordered_menu_content_sha256": groups(
            (case, [menus["documents"][i]["sha256"] for i in row["ordered_document_ids"]])
            for case, row in active.items()),
        "techqa_ordered_menu_document_ids": groups((k, v["ordered_document_ids"]) for k, v in active.items()),
        "ibm_published_positive_document_ids": groups(
            (r["task_id"], [cloud["published_passage_bindings"][p]["document_id"]
                            for p in r["published_positive_passage_pointers"]]) for r in cloud["cases"]),
        "aws_answer_document_sha256": groups((r["selector"]["row_index"], [r["document"]["sha256"]]) for r in aws),
        "aws_guide_family": groups((r["selector"]["row_index"], [r["document"]["family"]]) for r in aws),
        "contextbench_all_repository": groups(bench.items()),
        "contextbench_complete_sources_repository": groups(complete.items()),
    }
    print(json.dumps({
        "schema": "airk.private.confirmation-source-groups/1",
        "collector_sha256": sha256(Path(__file__).read_bytes()).hexdigest(),
        "inputs": PINS, "source_group_views": views,
        "cross_source_independence_verified": False, "paired_quality_measured": False,
        "sampling_or_power_plan_frozen": False,
    }, separators=(",", ":"), sort_keys=True))


if __name__ == "__main__":
    main()
