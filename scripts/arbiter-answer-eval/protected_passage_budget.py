# SPDX-License-Identifier: Apache-2.0
"""Protected 14k baseline plus new original nominations for unrepresented needs, no inference."""
import argparse
from collections import Counter
import json
from pathlib import Path

import adaptive_passage_budget as adaptive
from agent_cost_batch import working_set
import passage_reply_budget as reply
import prompt_need_audit as audit


def add_nominations(seed, candidates, cap, render_frame):
    wire, chosen = render_frame(seed)
    if chosen != seed:
        raise ValueError("Original returned passages no longer fit")
    represented = {p.owner for p in chosen if p.owner}
    paths = {p.path for p in chosen}
    points = {(p.path, n) for p in chosen for n in range(p.first, p.last + 1)}
    used = sum(p.size for p in chosen)
    omissions = []
    for p in candidates:
        if p in seed:
            continue
        if not p.owner:
            reason = "no_scored_need"
        elif p.owner in represented:
            reason = "need_already_represented"
        elif p.path not in paths and len(paths) == 6:
            reason = "file_limit"
        elif not any(s.strip() and (p.path, p.first + i) not in points for i, s in enumerate(p.text.splitlines())):
            reason = "no_new_location"
        elif used + p.size > min(cap, 20000):
            reason = "passage_limit"
        else:
            proposal = [*chosen, p]
            attempt, accepted = render_frame(proposal)
            if accepted == proposal and attempt is not None and len(attempt) <= cap:
                chosen, wire = accepted, attempt
                used += p.size
                paths.add(p.path)
                represented.add(p.owner)
                points.update((p.path, n) for n in range(p.first, p.last + 1))
                continue
            reason = "wire_limit"
        omissions.append({"at": p.at, "reason": reason})
    return wire, chosen, omissions


def grade(wire, snapshot, notes, links):
    body = json.loads(json.loads(wire)["result"]["content"][0]["text"])
    rows = [{"at": p["at"], "needs": p["needs"], "sha256": audit.digest(p["text"].encode())} for p in body["passages"]]
    size = sum(len(p["text"].encode()) for p in body["passages"])
    result = audit.score_variant({"passages": rows, "budget": 20000, "passage_bytes": size,
                                  "reply_needs": body["needs"]}, snapshot, notes, links, expected_budget=20000)
    return {"wire_sha256": audit.digest(wire), "wire_bytes": len(wire), "passage_bytes": size,
            "passages": rows, "omitted_count": body["omitted_count"], "totals": audit.totals(result),
            "whole_positive": sorted(k for k, level in result["notes"].items() if level == "whole"),
            "needs_with_whole": [n["index"] for n in result["needs"] if n["whole_refs"]]}


def compare(campaign, direct_path, controls_path, reference_path):
    controls = audit.read_json(controls_path)
    ref = audit.read_json(reference_path)
    tasks = {r["id"]: r for r in working_set(campaign)}
    rows = {}
    for line in direct_path.read_bytes().splitlines():
        item = json.loads(line)
        if item["type"] != "task":
            continue
        key = item["task"]
        if key in rows:
            raise ValueError("Repeated task")
        meta = tasks[key]
        if audit.digest(meta["task"].read_bytes()) != ref["tasks"][key]["scenario_sha256"]:
            raise ValueError("Frozen grading input changed")
        args = item["arguments"]
        snapshot = Path(item["source"])
        expected = meta["snapshot"] if meta["repository"] != "bookbuilder" else campaign / "agent-cost/E2E-r1/snapshots/bookbuilder"
        if snapshot.resolve(strict=True) != expected.resolve(strict=True):
            raise ValueError("Unexpected source root")
        pool = adaptive.input_stream(item, snapshot, len(args["needs"]))
        by_at = {p.at: p for p in pool}
        task = audit.read_json(meta["task"])
        revision = audit.read_json(meta["snapshot"].with_suffix(".json"))["commit"]
        if revision != task.get("source_commit", task.get("commit")):
            raise ValueError("Snapshot mismatch")
        notes = {n["id"]: n for n in task["notes"]}
        result = {}
        for limit in (8192, 14000, 20000):
            cap = reply.effective_wire_limit(args, limit)
            def frame(picks):
                return reply.render(args["task"], args["needs"], revision, picks, len(pool), cap)
            grades = {}
            seeds = {}
            for name in ("static14", "static20"):
                stock = controls["tasks"][key][name][str(limit)]
                picks = [by_at[p["at"]] for p in stock["passages"]]
                wire, baseline = frame(picks)
                if picks != baseline or audit.digest(wire) != stock["wire_sha256"]:
                    raise ValueError(f"Frozen wire differs for {key} at {limit}")
                seeds[name] = baseline
                grades[name] = grade(wire, snapshot, notes, ref["tasks"][key]["need_notes"])
            wire, chosen, omitted = add_nominations(seeds["static14"], pool, cap, frame)
            actual = grade(wire, snapshot, notes, ref["tasks"][key]["need_notes"])
            old = grades["static14"]
            if (old["passages"] != actual["passages"][:len(old["passages"])] or len(wire) > cap
                    or actual["omitted_count"] != len(omitted) or not set(old["whole_positive"]) <= set(actual["whole_positive"])):
                raise ValueError("Failed wire, baseline-preservation or omission invariant")
            actual["added"] = [p.at for p in chosen[len(seeds["static14"]):]]
            actual["omissions"] = omitted
            actual["omission_counts"] = dict(Counter(x["reason"] for x in omitted))
            grades["protected"] = actual
            result[str(limit)] = grades
        rows[key] = result
    if rows.keys() != tasks.keys() or controls["tasks"].keys() != rows.keys():
        raise ValueError("Incomplete panel")
    return {"selector_sha256": audit.digest(Path(__file__).read_bytes()), "direct_sha256": audit.digest(direct_path.read_bytes()),
            "control_sha256": audit.digest(controls_path.read_bytes()), "reference_sha256": audit.digest(reference_path.read_bytes()),
            "tasks": rows}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("campaign", "direct", "wire_controls", "reference", "output"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    result = compare(args.campaign, args.direct, args.wire_controls, args.reference)
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, indent=1)
        stream.write("\n")
