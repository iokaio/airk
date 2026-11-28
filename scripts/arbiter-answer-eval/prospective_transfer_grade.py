# SPDX-License-Identifier: Apache-2.0
"""Independently grade frozen transfer references against the bytes delivered in each MCP frame."""
import argparse
from collections import Counter
import json
from pathlib import Path

import common
import prompt_need_audit as audit
from prospective_transfer import digest, load_fixture


def grade_arm(repo, wire, labels, expected_needs):
    raw = wire["wire"].encode()
    if digest(raw) != wire["wire_sha256"] or len(raw) != wire["wire_bytes"]:
        raise ValueError("Changed wire")
    wrapped = json.loads(raw)["result"]
    if wrapped.get("isError"):
        raise ValueError("Transfer arm returned an error")
    body = json.loads(wrapped["content"][0]["text"])
    if [n["text"] for n in body["needs"]] != expected_needs:
        raise ValueError("Need identity changed")
    spans = []
    for row in body["passages"]:
        path, a, b = audit.parse_location(row["at"])
        if path not in repo.digests or not 1 <= a <= b <= len(repo.texts[path]):
            raise ValueError("Passage outside frozen scope")
        text = "\n".join(repo.texts[path][a-1:b])+"\n"
        if (text != row["text"] or digest(text.encode()) != row["text_sha256"]
                or repo.digests[path].hex() != row["workspace_sha256"]):
            raise ValueError("Returned bytes do not support citation")
        spans.append((path, a, b, row["needs"]))
    counts, by_need = Counter(), []
    for item in labels:
        i = item["need"]
        attributed = [s for s in spans if i in s[3]]
        status = body["needs"][i-1]["status"]
        if item.get("absent"):
            counts["absence_needs"] += 1
            linked = bool(attributed)
            counts["absence_with_link"] += linked
            counts["absence_explicit_no_candidate"] += status == "no_candidate_evidence" and not linked
            counts["absence_not_individually_scored"] += status == "not_individually_scored"
            by_need.append({"need":i,"absent":True,"linked":linked,"status":status})
            continue
        levels, owners = [], []
        for alternatives in item["required"]:
            for loc in alternatives:
                p, (a,b) = loc["path"], loc["lines"]
                if (repo.digests[p].hex() != loc["source_sha256"]
                        or digest(("\n".join(repo.texts[p][a-1:b])+"\n").encode()) != loc["excerpt_sha256"]):
                    raise ValueError("Reference bytes changed")
            note = {**alternatives[0], "alternatives":alternatives[1:]}
            levels.append(audit.evidence_level(note, spans))
            owners.append(audit.evidence_level(note, attributed))
        if not levels:
            raise ValueError("Positive need without reference")
        counts["positive_needs"] += 1
        counts["whole_needs"] += all(s == "whole" for s in levels)
        counts["attributed_whole_needs"] += all(s == "whole" for s in owners)
        counts["any_positive_overlap"] += any(s != "missing" for s in levels)
        by_need.append({"need":i,"reference_levels":levels,"attributed_levels":owners,"status":status})
    return {"counts": counts, "needs": by_need, "wire_bytes":len(raw), "logical_judgments":wire["logical_judgments"]}


def grade(panel, freeze_sha, receipt):
    repo, requests, manifest = load_fixture(panel, freeze_sha)
    refs = panel / "grader/references.json"
    if digest(refs.read_bytes()) != manifest["references_sha256"]:
        raise ValueError("Frozen grader changed")
    labels = {r["id"]: r for r in common.read_json(refs)}
    run, *rows = [json.loads(line) for line in receipt.read_bytes().splitlines()]
    if (run["freeze_sha256"].lower() != freeze_sha.lower() or [r["id"] for r in rows] != [q["id"] for q in requests]
            or len(labels) != len(requests)):
        raise ValueError("Unmatched panel")
    reports, totals = {}, {}
    for q, record in zip(requests, rows, strict=True):
        key = q["id"]
        if record["request_sha256"] != digest(common.canonical_json(q)):
            raise ValueError("Frozen request identity changed")
        if set(record["variants"]) != {"deterministic","bundled","finalist"}:
            raise ValueError("Incomplete comparison")
        reports[key] = {}
        for arm, raw in record["variants"].items():
            report = grade_arm(repo,raw,labels[key]["needs"],q["arguments"]["needs"])
            reports[key][arm] = report
            totals.setdefault(arm,Counter()).update(report["counts"])
    return {"freeze_sha256":freeze_sha,"receipt_sha256":digest(receipt.read_bytes()),"totals":totals,"tasks":reports}


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("panel",type=Path)
    parser.add_argument("receipt",type=Path)
    parser.add_argument("output",type=Path)
    parser.add_argument("--freeze-sha256",required=True)
    args=parser.parse_args()
    result=grade(args.panel,args.freeze_sha256,args.receipt)
    with args.output.open("x",encoding="utf-8") as stream:
        json.dump(result,stream,indent=1)
        stream.write("\n")
