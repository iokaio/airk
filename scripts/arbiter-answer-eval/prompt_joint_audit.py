# SPDX-License-Identifier: Apache-2.0
"""Grade the exact wrapped transport bytes of incumbent and two 40+40 corroboration arms."""
import argparse
from collections import Counter
import json
from pathlib import Path

from prompt_need_audit import differences, digest, parse_location, read_json, score_variant, source_lines, totals, working_set
from prompt_range_audit import file_digest


def contains(outer, inner):
    path, a, b = parse_location(outer)
    file, first, last = parse_location(inner)
    return file == path and a <= first <= last <= b


def wire_variant(outcome, cap, snapshot, args, identity, scores):
    wire = outcome["wire_utf8"].encode()
    if not wire.endswith(b"\n") or len(wire) > cap or len(wire) != outcome["wire_bytes"] or digest(wire) != outcome["wire_sha256"]:
        raise ValueError("Actual frame length or digest differs")
    rpc = json.loads(wire)
    if rpc["jsonrpc"] != "2.0" or rpc["id"] != 1 or rpc["result"]["isError"] or len(rpc["result"]["content"]) != 1:
        raise ValueError("Invalid JSON-RPC result")
    body = json.loads(rpc["result"]["content"][0]["text"])
    if body["task"] != args["task"] or body["eligibility"] != identity:
        raise ValueError("Task or scoped publication differs")
    accepted = {}
    for row in outcome["accepted"]:
        src = {k: v for k, v in row.items() if k != "kind"}
        key = (row["need"], row["scored_at"])
        if row["kind"] not in ("existing", "new") or src not in scores or row["score"] <= .5 or key in accepted:
            raise ValueError("Unsupported or duplicate positive model judgment")
        accepted[key] = row
    passages, supports = [], []
    size = 0
    for p in body["passages"]:
        path, a, b = parse_location(p["at"])
        lines = source_lines(snapshot, path)
        if b > len(lines) or p["text"] != "\n".join(lines[a-1:b]) + "\n":
            raise ValueError("Wire text differs from its source range")
        sha = digest(p["text"].encode())
        if sha != p["text_sha256"] or p["workspace_sha256"] != file_digest(snapshot, path):
            raise ValueError("Incorrect source or returned text digest")
        if p["needs"] != [link["need"] for link in p["support"]]:
            raise ValueError("Passage tags differ from explicit links")
        for link in p["support"]:
            key = (link["need"], link["scored_at"])
            if key not in accepted or not contains(p["at"], link["scored_at"]):
                raise ValueError("Claimed source link does not match scored evidence")
            if accepted[key]["kind"] == "new" and not contains(p["at"], accepted[key]["returned_at"]):
                raise ValueError("New lead lacks its original whole source range")
            supports.append(dict(link, returned_at=p["at"]))
        passages.append({"at": p["at"], "needs": p["needs"], "sha256": sha})
        size += len(p["text"].encode())
    if Counter((p["need"], p["scored_at"]) for p in supports) != Counter(list(accepted)):
        raise ValueError("Accepted model links displaced in transport")
    if len(body["needs"]) != len(args["needs"]):
        raise ValueError("Unreported caller need")
    for n, need in enumerate(body["needs"], 1):
        where = [{"at": p["at"]} for p in passages if n in p["needs"]]
        status = "candidate_evidence" if where else "no_candidate_evidence" if accepted else "not_individually_scored"
        if need["need"] != n or need["text"] != args["needs"][n-1] or need["where"] != where or need["status"] != status:
            raise ValueError("Need disclosure differs from the actual emitted evidence")
    return {"budget": 14000, "passage_bytes": size, "passages": passages, "scored_support": supports, "reply_needs": body["needs"]}


def run(campaign, annotations, raw, paired_path, range_path, baseline_path):
    head, *rows = [json.loads(s) for s in raw.read_bytes().splitlines()]
    for key, file in [("paired_sha256", paired_path), ("ranges_sha256", range_path), ("baseline_sha256", baseline_path)]:
        if head[key] != digest(file.read_bytes()):
            raise ValueError("Parent receipt changed")
    paired = {r["task"]: r for s in paired_path.read_bytes().splitlines() if (r := json.loads(s)).get("type") == "task"}
    scored = {r["task"]: r for s in range_path.read_bytes().splitlines() if (r := json.loads(s)).get("type") == "task"}
    baseline = read_json(baseline_path)["tasks"]
    mapping = read_json(annotations)
    cases = {r["id"]: r for r in working_set(campaign)}
    if head["caps"] not in (["default"], [8192, 14000, 20000]):
        raise ValueError("Unrecognized cap cohort")
    summaries, details = {}, {}
    for row in rows:
        key = row["task"]
        if key in details or row["source"] != paired[key]["source"]:
            raise ValueError("Duplicate trial or changed source root")
        expected = cases[key]["task"]
        ref = mapping["tasks"][key]
        if digest(expected.read_bytes()) != ref["scenario_sha256"]:
            raise ValueError("Reference drift")
        notes = {n["id"]: n for n in read_json(expected)["notes"]}
        details[key] = {}
        identity = {"index_id": row["index"], "publication": row["publication"], "scope_fingerprint": row["scope_fingerprint"]}
        for cap in head["caps"]:
            for arm in ("baseline", "strict", "reserve"):
                name = f"{cap}/{arm}"
                outcome = row["outcomes"][name]
                verified = wire_variant(outcome, 131072 if cap == "default" else cap, Path(row["source"]), paired[key]["arguments"], identity,
                                        scored[key]["variants"]["range40"]["scores"])
                incumbent = baseline[key]["outcomes"][f"14000/{cap}/bundled/spare16"]
                if (arm == "baseline" and outcome["wire_sha256"] != incumbent["wire_sha256"]) or (
                        arm != "baseline" and outcome["baseline_sha256"] != incumbent["wire_sha256"]):
                    raise ValueError("Incumbent wire identity differs")
                protected = baseline[key]["outcomes"][f"14000/{cap}/bundled/original"] if arm == "reserve" else incumbent
                if not all(any(contains(v["at"], p["at"]) for v in verified["passages"]) for p in protected["passages"]):
                    raise ValueError("Protected source bytes displaced")
                score = score_variant(verified, Path(row["source"]), notes, ref["need_notes"])
                counts = totals(score)
                counts["wire_bytes"] = outcome["wire_bytes"]
                counts["accepted_links"] = len(outcome["accepted"])
                counts["accepted_new"] = sum(r["kind"] == "new" for r in outcome["accepted"])
                score["totals"] = counts
                details[key][name] = score
                score["against_baseline"] = differences(details[key][f"{cap}/baseline"], score)
                summaries.setdefault(name, Counter()).update(counts)
    if set(details) != set(cases):
        raise ValueError("Incomplete panel")
    return {"sha256": digest(raw.read_bytes()), "mapping_sha256": digest(annotations.read_bytes()), "tasks": details, "totals": summaries}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for k in ("campaign", "annotations", "raw", "paired_path", "range_path", "baseline_path"):
        parser.add_argument(k, type=Path)
    print(json.dumps(run(**vars(parser.parse_args())), sort_keys=True))
