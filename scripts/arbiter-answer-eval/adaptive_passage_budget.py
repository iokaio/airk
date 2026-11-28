# SPDX-License-Identifier: Apache-2.0
"""Compare passage budgets on identical direct nominations without invoking a model."""
import argparse
from collections import Counter
from dataclasses import dataclass
import json
from pathlib import Path

from agent_cost_batch import working_set
import fixture
import prompt_need_audit as audit


@dataclass(frozen=True)
class Passage:
    at: str
    path: str
    first: int
    last: int
    text: str
    owner: int | None
    scored_at: str | None

    @property
    def size(self):
        return len(self.text.encode())


def allocation(stream, limit, priority=False, adaptive=False):
    """Allocate complete ranges. Return exact omissions for candidate and file-width accounting."""
    remaining = list(stream)
    chosen, omissions = [], []
    represented, paths, covered, seen_text = set(), set(), set(), set()
    spent = 0
    while remaining:
        if priority:
            i = next((i for i, p in enumerate(remaining) if p.owner and p.owner not in represented), 0)
        else:
            i = 0
        p = remaining.pop(i)
        lines = p.text.splitlines()
        informative = [(p.first + n, ' '.join(s.split())) for n, s in enumerate(lines) if s.strip()]
        fresh = sum((p.path, n) not in covered and s not in seen_text for n, s in informative)
        if p.path not in paths and len(paths) == 6:
            reason = "file_limit"
        elif spent + p.size > limit:
            reason = "byte_limit"
        elif adaptive and (not fresh or (p.owner in represented or p.owner is None) and 2 * fresh < len(informative)):
            reason = "low_novelty"
        else:
            spent += p.size
            paths.add(p.path)
            if p.owner:
                represented.add(p.owner)
            covered.update((p.path, n) for n, s in informative)
            seen_text.update(s for n, s in informative)
            chosen.append(p)
            continue
        omissions.append({"at": p.at, "reason": reason, "bytes": p.size, "need": p.owner})
    return {"passages": [{"at": p.at, "needs": [p.owner] if p.owner else [],
                          "sha256": audit.digest(p.text.encode())} for p in chosen],
            "scored_support": [{"need": p.owner, "scored_at": p.scored_at, "returned_at": p.at}
                               for p in chosen if p.owner], "passage_bytes": spent, "budget": limit,
            "omissions": omissions, "omission_counts": dict(Counter(o["reason"] for o in omissions))}


def input_stream(item, snapshot, need_count):
    direct = item["variants"]["direct"]
    pool = {x["file"]: x for x in item["pool"]}
    rosters = [direct["per_need"][str(i)] for i in range(1, need_count + 1)]
    head = [r[j]["file"] for j in range(max(map(len, rosters))) for r in rosters if j < len(r)]
    if len(pool) != 40 or len(head) != 40 or set(head) != pool.keys():
        raise ValueError("Frozen direct nomination pool mismatch")
    by_file = {p["file"]: (i, p) for i, row in enumerate(rosters, 1) for p in row}
    scores = {r["file"]: r for r in direct["scores"]}
    if len(scores) != 40 or len(direct["scores"]) != 40:
        raise ValueError("Missing or duplicate scored file")
    for roster in rosters:
        if roster != sorted(roster, key=lambda r: (-scores[r["file"]]["score"], scores[r["file"]]["pool_position"])):
            raise ValueError("Frozen direct ranks differ from scored order")
    passages = []
    for path in head:
        entry = pool[path]
        full = (snapshot / path).resolve(strict=True)
        if not full.is_relative_to(snapshot.resolve(strict=True)) or audit.digest(full.read_bytes()) != entry["source_sha256"]:
            raise ValueError("Source differs from scored file")
        owner, nomination = by_file[path]
        if (scores[path]["need"] != owner or nomination["scored_at"] != entry["scored_at"]
                or nomination["returned_at"] != entry["returned_at"]):
            raise ValueError("Scored nomination identity mismatch")
        if path not in head[:16]:
            continue
        lines = audit.source_lines(snapshot, path)
        for at in entry["locations"]:
            here, a, b = audit.parse_location(at)
            if here != path or b > len(lines):
                raise ValueError("Invalid nominated range")
            linked = at == nomination["returned_at"]
            passages.append(Passage(at, path, a, b, "\n".join(lines[a-1:b]) + "\n",
                                    owner if linked else None, nomination["scored_at"] if linked else None))
    # Same stable six-file spread as live_gather.focus. The remaining ten candidates
    # are eligible when a spread file cannot fit the cap.
    first, rest, seen = [], [], set()
    for p in passages:
        if p.path not in seen and len(seen) < 6:
            seen.add(p.path)
            first.append(p)
        else:
            rest.append(p)
    return first + rest


def replay(campaign, receipt, reference, caller_limit):
    mapping = audit.read_json(reference)
    calls_file = campaign / 'replay/first-calls-r1.json'
    calls = audit.read_json(calls_file)["calls"]
    if audit.digest(calls_file.read_bytes()) != mapping["calls_sha256"]:
        raise ValueError("Caller manifest changed")
    tasks = {x["id"]: x for x in working_set(campaign)}
    rows = {}
    for line in receipt.read_bytes().splitlines():
        item = json.loads(line)
        if item["type"] != "task":
            continue
        key = item["task"]
        meta = tasks[key]
        annotation = mapping["tasks"][key]
        if (key in rows or item["arguments"] != calls[key] or item["task_sha256"] != annotation["scenario_sha256"]
                or audit.digest(meta["task"].read_bytes()) != item["task_sha256"]):
            raise ValueError("Task, scenario or call identity mismatch")
        snapshot = meta["snapshot"] if meta["repository"] != "bookbuilder" else campaign / "agent-cost/E2E-r1/snapshots/bookbuilder"
        if snapshot.resolve(strict=True) != Path(item["source"]).resolve(strict=True):
            raise ValueError("Unexpected source root")
        for _, path in fixture.EXCLUSIONS.get(meta["repository"], []):
            if (snapshot / path).exists():
                raise ValueError("Ineligible snapshot")
        needs = item["arguments"]["needs"]
        stream = input_stream(item, snapshot, len(needs))
        baseline = allocation(stream, 14000)
        recorded = item["variants"]["direct"]
        if any(baseline[k] != recorded[k] for k in ("passages", "passage_bytes", "budget")):
            raise ValueError(f"Stock allocator mismatch for {key}")
        notes = {n["id"]: n for n in audit.read_json(meta["task"])["notes"]}
        variants = {}
        scores = {}
        for arm, cap, pri, adapt in (("static14", 14000, False, False), ("static20", 20000, False, False),
                                      ("priority20", 20000, True, False), ("adaptive", 20000, True, True)):
            limit = min(cap, caller_limit)
            result = allocation(stream, limit, pri, adapt)
            scored = audit.score_variant(result, snapshot, notes, annotation["need_notes"], expected_budget=limit)
            scores[arm] = scored
            variants[arm] = dict(result, totals=audit.totals(scored))
        for arm, result in variants.items():
            result["vs_static14"] = audit.differences(scores["static14"], scores[arm])
            result["vs_static20"] = audit.differences(scores["static20"], scores[arm])
        variants["adaptive"]["vs_priority20"] = audit.differences(scores["priority20"], scores["adaptive"])
        rows[key] = {"stratum": meta["set"], "candidate_ranges": len(stream),
                     "direct_pool_sha256": audit.digest(json.dumps(item["pool"],sort_keys=True).encode()), "arms": variants}
    if set(rows) != set(tasks) or set(rows) != set(mapping["tasks"]):
        raise ValueError("Incomplete paired panel")
    return {"receipt_sha256": audit.digest(receipt.read_bytes()), "reference_sha256": audit.digest(reference.read_bytes()),
            "allocator_sha256": audit.digest(Path(__file__).read_bytes()), "caller_passage_limit": caller_limit, "tasks": rows}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("campaign", type=Path)
    ap.add_argument("receipt", type=Path)
    ap.add_argument("reference", type=Path)
    ap.add_argument("output", type=Path)
    ap.add_argument("--caller-passage-limit", type=int, required=True)
    args = ap.parse_args()
    if args.caller_passage_limit <= 0:
        ap.error("positive caller passage limit required")
    result = replay(args.campaign, args.receipt, args.reference, args.caller_passage_limit)
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, ensure_ascii=True, indent=1)
        stream.write("\n")


if __name__ == "__main__":
    main()