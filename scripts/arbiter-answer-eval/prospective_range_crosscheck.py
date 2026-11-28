# SPDX-License-Identifier: Apache-2.0
"""Independently check published range-challenger wires against a frozen transfer fixture."""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path

import common
from prospective_transfer import load_fixture
from prospective_transfer_crosscheck import coverage, read, sha
from prospective_transfer_range import DIRECT, reconstruct
from prompt_need_audit import parse_location
from range_choice import nominate


def add_span(rows, at):
    path, a, b = parse_location(at)
    rows[path].update(range(a, b+1))


def check(panel, raw):
    freeze = "d8f98ab911bbcc4b8f10926973947e02e692a7f8bea5b2baa6ef6129dcfbd2a8"
    baseline = panel / "receipts/actor.jsonl"
    head, *rows = [json.loads(s) for s in raw.read_bytes().splitlines()]
    previous, *prior = [json.loads(s) for s in baseline.read_bytes().splitlines()]
    if (head["freeze_sha256"] != freeze or head["baseline_sha256"] != sha(baseline.read_bytes()) or
            head["server_sha256"] != previous["server_sha256"] or
            head["selector_sha256"] != sha(Path(__file__).with_name("range_choice.py").read_bytes())):
        raise ValueError("Mismatched panel, selector, scorer or primary actor")
    repo, cases, manifest = load_fixture(panel, freeze)
    labels = panel / "grader/references.json"
    if sha(labels.read_bytes()) != manifest["references_sha256"]:
        raise ValueError("Reference set changed")
    refs = {r["id"]: r["needs"] for r in read(labels)}
    inventory = read(panel / "actors/source.json")
    totals, cases_out = Counter(), {}
    for req, primary, r in zip(cases, prior, rows, strict=True):
        key, args = req["id"], req["arguments"]
        if key != r["id"] or key != primary["id"] or r["request_sha256"] != primary["request_sha256"]:
            raise ValueError("Request identity changed")
        pool = reconstruct(repo, args, primary)
        proposed = nominate(repo, pool, args, 80)
        pairs = [(n.query, n.document) for n in proposed]
        if (sha(common.canonical_json([DIRECT, pairs])) != r["inputs_sha256"] or len(pairs) != r["logical_judgments"]
                or r["pool_input_sha256"] != primary["variants"]["bundled"]["batches"][0]["input_sha256"]):
            raise ValueError("Model input reconstruction failed")
        wire = r["wire"].encode()
        if sha(wire) != r["wire_sha256"] or len(wire) != r["wire_bytes"] or len(wire) > args.get("budget", {}).get("max_bytes", 131072):
            raise ValueError("Wire integrity or limit failure")
        rpc = json.loads(wire)
        if rpc["result"]["isError"] or len(rpc["result"]["content"]) != 1:
            raise ValueError("Error envelope or extra content")
        body = json.loads(rpc["result"]["content"][0]["text"])
        identity = manifest["source_manifest_sha256"]
        if body["task"] != args["task"] or body["eligibility"] != {"index_id": "offline-transfer",
                "publication": "offline-fixture-sha256:"+identity, "scope_fingerprint": identity}:
            raise ValueError("Binding changed")
        levels = ["delivered", "attributed", "protected", "scored"]
        observed = {n: {k: defaultdict(set) for k in levels} for n in range(1, len(args["needs"])+1)}
        counts, size = Counter(), 0
        for p in body["passages"]:
            path, a, b = parse_location(p["at"])
            text = "\n".join(repo.texts[path][a-1:b])+"\n"
            if (b > len(repo.texts[path]) or text != p["text"] or sha(text.encode()) != p["text_sha256"]
                    or inventory[path] != p["workspace_sha256"]):
                raise ValueError("Incorrect source, returned range or hash")
            size += len(text.encode())
            for need in observed.values():
                add_span(need["delivered"], p["at"])
            if p["needs"] != [link["need"] for link in p["support"]]:
                raise ValueError("Need attribution mismatch")
            for link in p["support"]:
                n, scored = link["need"], link["scored_at"]
                seeds = [x.source.at for x in proposed if x.need == n and x.scored_at == scored
                         and x.source.path == path and a <= x.source.first and x.source.first+len(x.source.lines)-1 <= b]
                if not seeds:
                    raise ValueError("Scored and protected ranges not in the returned source")
                add_span(observed[n]["attributed"], p["at"])
                # Guarantee only the intersection if multiple nominations have an identical scored interval.
                for line in set.intersection(*(set(range(parse_location(s)[1], parse_location(s)[2]+1)) for s in seeds)):
                    observed[n]["protected"][path].add(line)
                add_span(observed[n]["scored"], scored)
                counts["links"] += 1
        if size != r["source_bytes"] or size > 14000 or len({parse_location(p["at"])[0] for p in body["passages"]}) > 6:
            raise ValueError("Source payload limits exceeded")
        outcomes = []
        for label, need in zip(refs[key], body["needs"], strict=True):
            n = label["need"]
            where = [{"at": p["at"]} for p in body["passages"] if n in p["needs"]]
            if need != {"need": n, "text": args["needs"][n-1], "where": where,
                        "status": "candidate_evidence" if where else "no_candidate_evidence"}:
                raise ValueError("Need disclosure mismatch")
            if label.get("absent"):
                counts["absence_needs"] += 1
                counts["absent_with_link"] += bool(where)
                counts["links_on_absence"] += len(where)
                outcomes.append({"need": n, "absent": True, "links": len(where)})
                continue
            counts["positive"] += 1
            out = {"need": n}
            for level in levels:
                families = [[coverage(observed[n][level], loc, repo.texts, inventory) for loc in alternatives]
                            for alternatives in label["required"]]
                whole = all(any(full for full, _ in f) for f in families)
                overlap = any(part for f in families for _, part in f)
                counts[level+"_whole"] += whole
                counts[level+"_overlap"] += overlap
                out[level] = "whole" if whole else "partial" if overlap else "missing"
            outcomes.append(out)
        counts.update({"judgments": len(pairs), "source_bytes": size, "wire_bytes": len(wire)})
        totals.update(counts)
        cases_out[key] = {"counts": counts, "outcomes": outcomes}
    return {"actor_sha256": sha(raw.read_bytes()), "baseline_sha256": head["baseline_sha256"], "tasks": cases_out, "totals": totals}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("panel", type=Path)
    parser.add_argument("raw", type=Path)
    args = parser.parse_args()
    print(json.dumps(check(args.panel, args.raw), sort_keys=True))
