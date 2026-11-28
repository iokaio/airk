# SPDX-License-Identifier: Apache-2.0
"""Post-process paired replay receipts against a separately frozen need/reference annotation.

Whole ranges measure delivery of reference text. A need with one matching reference has some
evidence, including when other facets are missing. Unmatched evidence can be relevant.
"""
import argparse
from collections import Counter, defaultdict
from functools import lru_cache
import hashlib
import json
from pathlib import Path, PurePosixPath
import re

from agent_cost_batch import working_set
from arbiter_pair_probe import PREFIX, SUFFIX, pair_text
from topical_index import defines_of, role_of


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_json(path):
    return json.loads(path.read_bytes())


def parse_location(at):
    match = re.fullmatch(r"([^:\\\x00]+):([1-9][0-9]*)-([1-9][0-9]*)", at)
    if not match:
        raise ValueError(f"Invalid location: {at}")
    path, first, last = match.groups()
    if PurePosixPath(path).is_absolute() or any(x in ("", ".", "..") for x in path.split("/")) or int(first) > int(last):
        raise ValueError(f"Invalid location: {at}")
    return path, int(first), int(last)


@lru_cache(maxsize=256)
def source_lines(snapshot, relative):
    file = (snapshot / relative).resolve(strict=True)
    if not file.is_relative_to(snapshot.resolve(strict=True)):
        raise ValueError("Source path outside frozen snapshot")
    return file.read_bytes().decode("utf-8", errors="replace").splitlines()


def verified_passages(variant, snapshot, need_count, *, expected_budget=14000):
    spans = []
    seen = set()
    size = 0
    for row in variant["passages"]:
        at = row["at"]
        if at in seen:
            raise ValueError(f"Duplicate delivered range: {at}")
        seen.add(at)
        path, first, last = parse_location(at)
        lines = source_lines(snapshot, path)
        if last > len(lines):
            raise ValueError(f"Range beyond source: {at}")
        data = ("\n".join(lines[first - 1:last]) + "\n").encode()
        if digest(data) != row["sha256"]:
            raise ValueError(f"Returned excerpt differs from declared range: {at}")
        owners = row["needs"]
        if len(set(owners)) != len(owners) or any(type(i) != int or not 1 <= i <= need_count for i in owners):
            raise ValueError(f"Invalid need attribution: {at}")
        size += len(data)
        spans.append((path, first, last, owners))
    if variant["budget"] != expected_budget or size != variant["passage_bytes"] or size > variant["budget"]:
        raise ValueError("Returned passage budget/size mismatch")
    if len({span[0] for span in spans}) > 6:
        raise ValueError("Returned file width exceeds six")
    return spans


def evidence_level(note, spans):
    """A whole reference range may be delivered as several overlapping or adjacent chunks."""
    best = 0
    for loc in [note, *note.get("alternatives", [])]:
        if not loc.get("path") or not loc.get("lines"):
            continue
        first, last = loc["lines"]
        intervals = sorted((max(a, first), min(b, last)) for p, a, b, _ in spans
                           if p == loc["path"] and a <= last and b >= first)
        covered_to = first - 1
        for a, b in intervals:
            if b - a + 1 >= min(5, last - first + 1):
                best = max(best, 2)
            else:
                best = max(best, 1)
            if a <= covered_to + 1:
                covered_to = max(covered_to, b)
        if covered_to == last:
            return "whole"
    return ("missing", "overlap", "rule3")[best]


def short_card_eligible(snapshot, note):
    for location in [note, *note.get("alternatives", [])]:
        if not location.get("lines") or not location.get("path"):
            continue
        path = location["path"]
        first, last = location["lines"]
        lines = source_lines(snapshot, path)
        if 1 <= first <= last <= len(lines) and len("\n".join(lines[first - 1:last])) <= 480:
            return True
    return False


def score_variant(variant, snapshot, notes, links, *, expected_budget=14000):
    spans = verified_passages(variant, snapshot, len(links), expected_budget=expected_budget)
    located = {key: n for key, n in notes.items() if n.get("path") and n.get("lines") and not n.get("absence")}
    levels = {key: evidence_level(note, spans) for key, note in located.items()}
    eligible = {key for key, note in located.items() if short_card_eligible(snapshot, note)}
    scored_spans = [(*parse_location(row["scored_at"]), [row["need"]]) for row in variant.get("scored_support", [])]
    needs = []
    for index, refs in enumerate(links, 1):
        positive = [key for key in refs if key in located]
        claimed = [span for span in spans if index in span[3]]
        scored = [span for span in scored_spans if index in span[3]]
        matched = [key for key in positive if levels[key] == "whole"]
        claim_matches = [key for key in positive if evidence_level(located[key], claimed) == "whole"]
        needs.append({"index": index, "reference_ids": positive, "nonpositive_or_unlocated": sorted(set(refs) - set(positive)),
                      "scorable": bool(positive), "short_card_eligible_refs": sorted(eligible.intersection(positive)), "whole_refs": matched,
                      "at_least_rule3_refs": [key for key in positive if levels[key] in ("whole", "rule3")],
                      "part_of_reference_returned": [key for key in positive if levels[key] != "missing"],
                      "attributed_ranges": len(claimed), "attributed_whole_refs": claim_matches,
                      "scored_excerpt_whole_refs": [key for key in positive if evidence_level(located[key], scored) == "whole"],
                      "scored_excerpt_rule3_refs": [key for key in positive if evidence_level(located[key], scored) in ("whole", "rule3")],
                      "no_candidate_despite_delivered_whole_ref": bool(variant.get("reply_needs") and
                          variant["reply_needs"][index - 1]["status"] == "no_candidate_evidence" and matched),
                      "attributed_without_reference_overlap": [
                          f"{p}:{a}-{b}" for p, a, b, owners in claimed if positive and
                          all(evidence_level(located[key], [(p, a, b, owners)]) == "missing" for key in positive)]})
    return {"needs": needs, "notes": levels, "essential": [key for key, n in located.items() if n["mark"] == "essential"],
            "unlocatable_notes": sorted(set(notes) - set(located))}


def totals(score):
    needs = score["needs"]
    return {"scorable_needs": sum(n["scorable"] for n in needs),
            "needs_with_a_short_card_eligible_reference": sum(bool(n["short_card_eligible_refs"]) for n in needs),
            "unscorable_needs": sum(not n["scorable"] for n in needs),
            "needs_with_any_whole_reference": sum(bool(n["whole_refs"]) for n in needs),
            "needs_with_every_mapped_reference": sum(bool(n["reference_ids"]) and n["whole_refs"] == n["reference_ids"] for n in needs),
            "needs_with_rule3_reference": sum(bool(n["at_least_rule3_refs"]) for n in needs),
            "whole_essential_notes": sum(score["notes"][key] == "whole" for key in score["essential"]),
            "located_essential_notes": len(score["essential"]),
            "needs_with_attributed_whole_reference": sum(bool(n["attributed_whole_refs"]) for n in needs),
            "needs_with_scored_excerpt_whole_reference": sum(bool(n["scored_excerpt_whole_refs"]) for n in needs),
            "no_candidate_despite_delivered_whole_reference": sum(n["no_candidate_despite_delivered_whole_ref"] for n in needs),
            "attributed_ranges_without_reference_overlap": sum(len(n["attributed_without_reference_overlap"]) for n in needs)}


def differences(base, trial):
    rows = {}
    for field in ("whole_refs", "at_least_rule3_refs"):
        before = {n["index"] for n in base["needs"] if n[field]}
        after = {n["index"] for n in trial["needs"] if n[field]}
        rows[field] = {"gained": sorted(after - before), "lost": sorted(before - after)}
    before = {key for key in base["essential"] if base["notes"][key] == "whole"}
    after = {key for key in trial["essential"] if trial["notes"][key] == "whole"}
    rows["whole_essential_notes"] = {"gained": sorted(after - before), "lost": sorted(before - after)}
    rows["mapped_whole_references"] = []
    for before_need, after_need in zip(base["needs"], trial["needs"], strict=True):
        previous, current = set(before_need["whole_refs"]), set(after_need["whole_refs"])
        if previous != current:
            rows["mapped_whole_references"].append({"need": after_need["index"],
                                                    "gained": sorted(current - previous), "lost": sorted(previous - current)})
    return rows


def source_cards(entry, snapshot):
    path = entry["file"]
    lines = source_lines(snapshot, path)
    if digest((snapshot / path).read_bytes()) != entry["source_sha256"].lower():
        raise ValueError(f"Candidate source file changed: {path}")
    parts = [path]
    for location in entry["locations"]:
        file, a, b = parse_location(location)
        if file != path or b > len(lines):
            raise ValueError(f"Invalid candidate location: {location}")
        excerpt = "\n".join(lines[a-1:b])[:480]
        parts.append(f"Lines {a}-{b} (excerpt):\n{excerpt}")
    file_card = "\n\n".join(parts)
    short = f"Path: {path}\nSource: no complete source lines available"
    if entry["scored_at"]:
        file, a, b = parse_location(entry["scored_at"])
        returned_file, first, last = parse_location(entry["returned_at"])
        if file != path or file != returned_file or not first <= a <= b <= last or entry["returned_at"] not in entry["locations"]:
            raise ValueError(f"Invalid excerpt origin: {path}")
        body = "\n".join(lines[a-1:b])
        if len(body) > 480:
            raise ValueError(f"Scored direct excerpt exceeds card budget: {path}")
        definitions = defines_of("\n".join(lines), role_of(path) in ("document", "guidance"))
        names = [name for name, indices in sorted(definitions.items()) if any(a <= i <= b and name in lines[i-1] for i in indices)]
        declaration = "\nDeclaration: " + ", ".join(names) if names else ""
        short = f"Path: {path}\nLines: {a}-{b}{declaration}\nSource:\n{body}"
    if digest(file_card.encode()) != entry["original_card_sha256"] or digest(short.encode()) != entry["direct_card_sha256"]:
        raise ValueError(f"Model card differs from frozen source: {path}")
    return file_card, short


def verify_model_inputs(item, snapshot):
    """Rebuild every pre-token-cap model prompt without access to any note, expected answer or rubric."""
    cards = {p["file"]: source_cards(p, snapshot) for p in item["pool"]}
    args = item["arguments"]
    for arm, variant in item["variants"].items():
        if arm == "deterministic":
            continue
        for row in variant["scores"]:
            needs = args["needs"] if arm == "control" else [args["needs"][row["need"]-1]]
            anchors = args.get("anchors", []) if arm != "no-anchors" else []
            query = "\n".join([args["task"], *needs, *anchors])
            if arm in ("labeled", "direct-wording", "direct"):
                query = f"Task: {args['task']}\nNeed: {needs[0]}\nExact strings: " + "\n".join(anchors)
            instruction = ("Given a coding agent's task, determine whether this repository file contains evidence needed to complete it."
                           if arm not in ("direct-wording", "direct") else
                           "Given a coding agent's task, determine whether the supplied source provides direct evidence for this need. "
                           "A caller, rule, example, or test may provide useful partial evidence.")
            card = cards[row["file"]][1 if arm == "direct" else 0]
            prompt = PREFIX + pair_text(instruction, query, card) + SUFFIX
            if digest(prompt.encode()) != row["rendered_sha256"]:
                raise ValueError(f"Model input differs from source-and-caller-only reconstruction: {item['task']} {arm}")


def pool_ceiling(pool, notes, links):
    """Optimistic source availability, before ordering and the six-file and byte ceilings."""
    files = {entry["file"] for entry in pool}
    spans = [(*parse_location(at), []) for entry in pool for at in entry["locations"]]
    result = []
    for index, refs in enumerate(links, 1):
        positive = [key for key in refs if notes[key].get("path") and notes[key].get("lines") and not notes[key].get("absence")]
        in_file = [key for key in positive if any(loc.get("path") in files for loc in [notes[key], *notes[key].get("alternatives", [])])]
        available = [key for key in in_file if evidence_level(notes[key], spans) == "whole"]
        result.append({"index": index, "scorable": bool(positive), "pooled_file_refs": in_file, "pooled_whole_refs": available})
    return result


def verify_pairing(item, calls, historical):
    key = item["task"]
    args = item["arguments"]
    if args != calls[key]:
        raise ValueError(f"Caller inputs changed: {key}")
    pool = [p["file"] for p in item["pool"]]
    if pool != [p for p, _ in historical["scores"]] or not 0 < len(pool) <= 40 or len(set(pool)) != len(pool):
        raise ValueError(f"Control pool changed: {key}")
    variants = item["variants"]
    differences = {}
    for arm, old in [("control", "qwen"), ("deterministic", "control")]:
        previous, current = historical[old], variants[arm]
        differences[arm] = {k: {p: [previous[k].get(p), current[k].get(p)] for p in sorted(set(previous[k]) | set(current[k]))
                                if previous[k].get(p) != current[k].get(p)} for k in ("notes", "ranks")}
        differences[arm]["passage_byte_delta"] = current["passage_bytes"] - previous["bytes"]
    prev = historical["scores"]
    current = [(row["file"], row["score"]) for row in variants["control"]["scores"]]
    ordered = lambda scores: [file for file, _ in sorted(scores, key=lambda r: -r[1])]
    differences["control"]["same_pool_order"] = ordered(current) == ordered(prev)
    differences["control"]["max_abs_score_delta"] = max(abs(a[1] - b[1]) for a, b in zip(current, prev, strict=True))
    differences["control"]["clipped_delta"] = variants["control"]["clipped"] - historical["clipped"]
    seen_inputs = {}
    for arm, variant in variants.items():
        if arm == "deterministic":
            continue
        scores = variant["scores"]
        expected = len(pool) * (len(args["needs"]) if arm == "per-need" else 1)
        if len(scores) != expected or variant["pairs"] != expected or not 0 <= variant["clipped"] <= expected:
            raise ValueError(f"Pair count/clipping invalid: {key} {arm}")
        for row in scores:
            model_input = row["rendered_sha256"]
            previous = seen_inputs.setdefault(model_input, row["score"])
            if previous != row["score"]:
                raise ValueError(f"Same rendered input yielded different selection scores: {key} {arm}")
        if "standalone_timing" in variant and not variant["standalone_timing"]["same_per_need_ranks"]:
            raise ValueError(f"Uncached fixed allocation changed per-need ranks: {key}")
        actual = [(r["need"], r["file"]) for r in scores]
        if arm == "control":
            spec = [(None, file) for file in pool]
        else:
            spec = [(n + 1, file) for n in range(len(args["needs"])) for i, file in enumerate(pool)
                    if arm == "per-need" or i % len(args["needs"]) == n]
        if actual != spec:
            raise ValueError(f"Pairs do not follow frozen pool allocation: {key} {arm}")
        support = []
        for passage in variant["passages"]:
            if arm == "direct":
                path, _, _ = parse_location(passage["at"])
                i = pool.index(path)
                entry = item["pool"][i]
                owner = i % len(args["needs"]) + 1
                expected_owners = [owner] if entry["returned_at"] == passage["at"] else []
                if passage["needs"] != expected_owners:
                    raise ValueError(f"Inaccurate direct range attribution: {key}")
                if expected_owners:
                    sp, a, b = parse_location(entry["scored_at"])
                    rp, c, d = parse_location(entry["returned_at"])
                    if sp != rp or not c <= a <= b <= d:
                        raise ValueError(f"Scored range outside returned context: {key}")
                    support.append((owner, entry["scored_at"], entry["returned_at"]))
            elif passage["needs"]:
                raise ValueError(f"File score attributed to an individual returned passage: {key} {arm}")
        if arm == "direct":
            actual_support = [(r["need"], r["scored_at"], r["returned_at"]) for r in variant["scored_support"]]
            if sorted(support) != sorted(actual_support) or len(variant["reply_needs"]) != len(args["needs"]):
                raise ValueError(f"Direct support or reported need inventory differs: {key}")
            for n, reply in enumerate(variant["reply_needs"], 1):
                ranges = [at for owner, _, at in support if owner == n]
                if (reply["need"] != n or reply["text"] != args["needs"][n-1] or
                        [r["at"] for r in reply["where"]] != ranges or
                        reply["status"] != ("candidate_evidence" if ranges else "no_candidate_evidence")):
                    raise ValueError(f"Invalid evidence/absence claim: {key} {n}")

    return differences


def summarize_gaps(rows, arm):
    """Per-task opportunity ceiling and directional changes against the joined control."""
    by_set = defaultdict(Counter)
    gap_counts = Counter()
    for task in rows.values():
        score = task["variants"][arm]
        total = score["totals"]
        group = by_set[task["set"]]
        group["tasks"] += 1
        group.update(total)
        for name in ("whole_refs", "at_least_rule3_refs", "whole_essential_notes"):
            for change, values in score["against_control"][name].items():
                group[f"{name}_{change}"] += len(values)
        for need, upper in zip(score["needs"], task["pool_ceiling"], strict=True):
            if not need["scorable"] or need["whole_refs"]:
                continue
            if not upper["pooled_file_refs"]:
                stage = "no_mapped_reference_file_in_pool"
            elif not upper["pooled_whole_refs"]:
                stage = "mapped_file_present_but_no_whole_reference_in_candidate_ranges"
            else:
                stage = "whole_reference_available_but_not_returned"
            gap_counts[stage] += 1
    return {"by_set": dict(by_set), "uncovered_need_by_stage": gap_counts}


def audit(campaign, annotations, receipts, allow_subset):
    mapping = read_json(annotations)
    call_path = campaign / "replay/first-calls-r1.json"
    if digest(call_path.read_bytes()) != mapping["calls_sha256"]:
        raise ValueError("Frozen caller needs changed")
    calls = read_json(call_path)["calls"]
    tasks = {t["id"]: t for t in working_set(campaign)}
    if set(calls) != set(tasks) or set(tasks) != set(mapping["tasks"]):
        raise ValueError("Mismatched reference task inventory")
    historical = {r["task"]: r for raw in (campaign / "replay/qwen-file-top40-r1.jsonl").read_bytes().splitlines()
                  if "task" in (r := json.loads(raw))}
    rows = {}
    aggregates = defaultdict(Counter)
    arm_names = None
    for raw in receipts.read_bytes().splitlines():
        item = json.loads(raw)
        if item.get("type") != "task":
            continue
        key = item["task"]
        if key in rows:
            raise ValueError(f"Duplicated task: {key}")
        meta, annotation = tasks[key], mapping["tasks"][key]
        if digest(meta["task"].read_bytes()) != annotation["scenario_sha256"]:
            raise ValueError(f"Frozen reference changed: {key}")
        links = annotation["need_notes"]
        notes = {n["id"]: n for n in read_json(meta["task"])["notes"]}
        if len(links) != len(calls[key]["needs"]) or any(set(group) - notes.keys() for group in links):
            raise ValueError(f"Frozen note mapping invalid: {key}")
        if item["task_sha256"].lower() != annotation["scenario_sha256"]:
            raise ValueError(f"Replay reference identity mismatch: {key}")
        historical_delta = verify_pairing(item, calls, historical[key])
        snapshot = (campaign / "agent-cost/E2E-r1/snapshots/bookbuilder" if meta["repository"] == "bookbuilder"
                    else meta["snapshot"]).resolve(strict=True)
        if Path(item["source"]).resolve(strict=True) != snapshot:
            raise ValueError("Unexpected source root")
        verify_model_inputs(item, snapshot)
        variants = item["variants"]
        if arm_names is None:
            arm_names = set(variants)
        if set(variants) != arm_names:
            raise ValueError(f"Unpaired arms: {key}")
        scores = {arm: score_variant(variant, snapshot, notes, links) for arm, variant in variants.items()}
        for arm, score in scores.items():
            score["totals"] = totals(score)
            score["against_control"] = differences(scores["control"], score)
            aggregates[arm].update(score["totals"])
        rows[key] = {"set": meta["set"], "historical_delta": historical_delta, "variants": scores,
                     "pool_ceiling": pool_ceiling(item["pool"], notes, links)}
    if not rows or (not allow_subset and rows.keys() != tasks.keys()):
        raise ValueError("Incomplete paired replay")
    return {"reference_sha256": digest(annotations.read_bytes()), "receipt_sha256": digest(receipts.read_bytes()),
            "tasks": rows, "aggregates": dict(aggregates), "gaps": {arm: summarize_gaps(rows, arm) for arm in aggregates}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    parser.add_argument("annotations", type=Path)
    parser.add_argument("receipts", type=Path)
    parser.add_argument("--allow-subset", action="store_true")
    args = parser.parse_args()
    print(json.dumps(audit(args.campaign, args.annotations, args.receipts, args.allow_subset), sort_keys=True))


if __name__ == "__main__":
    main()
