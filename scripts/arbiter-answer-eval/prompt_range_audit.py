# SPDX-License-Identifier: Apache-2.0
"""Verify matched range-choice score receipts independently of nomination, reranking and rendering."""
import argparse
from functools import lru_cache
import json
import math
from pathlib import Path

from arbiter_pair_probe import PREFIX, SUFFIX, pair_text
from prompt_need_audit import (differences, digest, evidence_level, parse_location, read_json, score_variant,
                               source_lines, totals, working_set)
from topical_index import defines_of, role_of, words_of
from arbiter_loop_prototype import PLAIN_WORDS

INSTRUCTION = ("Given a coding agent's task, determine whether the supplied source provides direct evidence for this need. "
               "A caller, rule, example, or test may provide useful partial evidence.")


@lru_cache(maxsize=256)
def file_digest(snapshot, path):
    return digest((snapshot / path).read_bytes())


@lru_cache(maxsize=256)
def declarations(snapshot, path):
    return defines_of("\n".join(source_lines(snapshot, path)), role_of(path) in ("document", "guidance"))


def generated_sources(row, original, snapshot):
    """Admit only own-need seed hits in the original files, outside original nominated ranges."""
    pool = {entry["file"]: entry for entry in original["pool"]}
    needs = original["arguments"]["needs"]
    accepted = set()
    by_file_need = {}
    for proposal in row.get("generated", []):
        n, path, at, seed = (proposal[k] for k in ("need", "file", "at", "seed"))
        file, first, last = parse_location(at)
        if path not in pool or path != file or not 1 <= n <= len(needs) or not first <= seed <= last or last-first >= 60:
            raise ValueError("Invalid generated source identity")
        lines = source_lines(snapshot, path)
        if last > len(lines) or not ((set(words_of(needs[n-1])) & set(words_of(lines[seed-1]))) - PLAIN_WORDS):
            raise ValueError("Seed absent from caller and source")
        for earlier in pool[path]["locations"]:
            _, a, b = parse_location(earlier)
            if a <= seed <= b:
                raise ValueError("Generated seed is already in original ranges")
        used = by_file_need.setdefault((path, n), [])
        if len(used) >= 2 or any(a <= last and first <= b for a, b in used):
            raise ValueError("Generated windows must be distinct for each need and file")
        used.append((first, last))
        accepted.add((n, at))
    return accepted


def coverage_ceiling(notes, links, original_ranges, generated, scores, passages):
    def measure(ranges, n, refs):
        spans = [(*parse_location(at), []) for owner, at in ranges if owner in (None, n)]
        return {kind: [key for key in refs if evidence_level(notes[key], spans) in levels]
                for kind, levels in [("whole", ("whole",)), ("rule3", ("whole", "rule3"))]}
    base = {(None, at) for at in original_ranges}
    shared = {(None, at) for _, at in generated}
    score_ranges = {(s["need"], s["returned_at"]) for s in scores}
    scored_short = {(s["need"], s["scored_at"]) for s in scores}
    rows = []
    for n, linked in enumerate(links, 1):
        refs = [key for key in linked if notes[key].get("path") and notes[key].get("lines") and not notes[key].get("absence")]
        rows.append({"index": n, "scorable": bool(refs),
                     "original": measure(base, n, refs), "pooled": measure(base | shared, n, refs),
                     "own_generated_plus_original": measure(base | generated, n, refs),
                     "scored_any": measure({(None, at) for _, at in score_ranges}, n, refs),
                     "scored_own": measure(score_ranges, n, refs), "short_scored_own": measure(scored_short, n, refs),
                     "new_returned_any": measure({(None, p["at"]) for p in passages if p["at"] not in original_ranges}, n, refs)})
    return rows


def verify_range_variant(variant, original, snapshot, limit, generated=frozenset()):
    args, pool, scores = original["arguments"], original["pool"], variant["scores"]
    if variant["pairs"] != len(scores) or not 0 < len(scores) <= limit:
        raise ValueError("Invalid score count")
    if sum(row["tokens_before_cap"] > 1024 for row in scores) != variant["clipped"]:
        raise ValueError("Recorded clipping inconsistent with token lengths")
    proposals = {}
    for row in scores:
        n, rank = row["need"], row["pool_position"]
        if not 1 <= n <= len(args["needs"]) or not 1 <= rank <= len(pool):
            raise ValueError("Invalid candidate or need")
        entry = pool[rank-1]
        path, start, end = parse_location(row["scored_at"])
        origin, a, b = parse_location(row["returned_at"])
        if not (entry["file"] == row["file"] == path == origin and a <= start <= end <= b and
                (row["returned_at"] in entry["locations"] or (n, row["returned_at"]) in generated) and
                row["source_sha256"] == entry["source_sha256"]):
            raise ValueError("Score does not identify an eligible source interval")
        if not math.isfinite(row["score"]) or not 0 <= row["score"] <= 1:
            raise ValueError("Invalid model score")
        lines = source_lines(snapshot, path)
        if file_digest(snapshot, path) != entry["source_sha256"] or end > len(lines):
            raise ValueError("Scored interval exceeds source")
        text = "\n".join(lines[start-1:end])
        if len(text) > 480:
            raise ValueError("Excerpt exceeds 480 characters")
        symbols = [name for name, positions in sorted(declarations(snapshot, path).items())
                   if any(start <= i <= end and name in lines[i-1] for i in positions)]
        field = "\nDeclaration: " + ", ".join(symbols) if symbols else ""
        document = f"Path: {path}\nLines: {start}-{end}{field}\nSource:\n{text}"
        query = f"Task: {args['task']}\nNeed: {args['needs'][n-1]}\nExact strings: " + "\n".join(args.get("anchors", []))
        if digest((PREFIX + pair_text(INSTRUCTION, query, document) + SUFFIX).encode()) != row["rendered_sha256"]:
            raise ValueError("Model prompt differs from caller and source reconstruction")
        key = (n, row["returned_at"])
        if key in proposals:
            raise ValueError("Repeated need/range proposal")
        proposals[key] = row["scored_at"]
    returned = {(n, p["at"]): proposals[(n, p["at"])] for p in variant["passages"] for n in p["needs"]}
    support = [(s["need"], s["returned_at"], s["scored_at"]) for s in variant["scored_support"]]
    if sorted(support) != sorted((n, at, short) for (n, at), short in returned.items()):
        raise ValueError("Returned attribution and scored evidence differ")
    omissions = set()
    for omitted in variant["omissions"]:
        key = (omitted["need"], omitted["at"])
        if omitted["reason"] not in {"file_limit", "byte_limit"} or key in omissions:
            raise ValueError("Invalid omission")
        omissions.add(key)
    if set(returned) & omissions or set(returned) | omissions != set(proposals):
        raise ValueError("Each nomination must be either returned or accounted for")
    if len(variant["reply_needs"]) != len(args["needs"]):
        raise ValueError("Incomplete reply needs")
    for n, reply in enumerate(variant["reply_needs"], 1):
        locations = [{"at": p["at"]} for p in variant["passages"] if n in p["needs"]]
        if reply != {"need": n, "text": args["needs"][n-1], "where": locations,
                    "status": "candidate_evidence" if locations else "no_candidate_evidence"}:
            raise ValueError("Need status or citations disagree with returned evidence")


def audit_ranges(campaign, annotations, paired, grades, raw, allow_subset=False):
    base_grade = read_json(grades)
    original_hash = digest(paired.read_bytes())
    header, *baseline = [json.loads(line) for line in paired.read_bytes().splitlines()]
    original = {row["task"]: row for row in baseline}
    reference = read_json(annotations)
    if (original_hash != base_grade["receipt_sha256"] or base_grade["reference_sha256"] != digest(annotations.read_bytes())
            or set(original) != set(reference["tasks"]) or len(baseline) != len(original)):
        raise ValueError("Original receipts differ from authenticated independent grade")
    first, *rows = [json.loads(line) for line in raw.read_bytes().splitlines()]
    if first["model"] != header["model"] or first["paired_sha256"] != original_hash:
        raise ValueError("Model or parent identity differs from original")
    inventory = {row["id"]: row for row in working_set(campaign)}
    results = {}
    for row in rows:
        key = row["task"]
        if key in results or row["source"] != original[key]["source"]:
            raise ValueError("Duplicate result or source mismatch")
        scenario = inventory[key]["task"]
        if digest(scenario.read_bytes()) != reference["tasks"][key]["scenario_sha256"]:
            raise ValueError("Scenario digest mismatch")
        notes = {n["id"]: n for n in read_json(scenario)["notes"]}
        links = reference["tasks"][key]["need_notes"]
        variants = row["variants"]
        if set(variants) != {"range40", "range80"} or variants["range40"]["scores"] != variants["range80"]["scores"][:40]:
            raise ValueError("Variants must share identical 40-pair prefix")
        snapshot = Path(row["source"])
        generated = generated_sources(row, original[key], snapshot)
        original_ranges = {at for p in original[key]["pool"] for at in p["locations"]}
        scores = {}
        for arm, limit in [("range40", 40), ("range80", 80)]:
            v = variants[arm]
            snapshot = Path(row["source"])
            verify_range_variant(v, original[key], snapshot, limit, generated)
            score = score_variant(v, snapshot, notes, links)
            score["totals"] = totals(score)
            score["against_control"] = differences(base_grade["tasks"][key]["variants"]["control"], score)
            score["ceiling"] = coverage_ceiling(notes, links, original_ranges, generated, v["scores"], v["passages"])
            scores[arm] = score
        results[key] = {"set": inventory[key]["set"], "variants": scores}
    if not results or (not allow_subset and results.keys() != original.keys()):
        raise ValueError("Incomplete range trial")
    return {"paired_sha256": original_hash, "grade_sha256": digest(grades.read_bytes()),
            "raw_sha256": digest(raw.read_bytes()), "tasks": results}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("campaign", "annotations", "paired", "grades", "raw"):
        parser.add_argument(name, type=Path)
    parser.add_argument("--allow-subset", action="store_true")
    args = parser.parse_args()
    print(json.dumps(audit_ranges(**vars(args)), sort_keys=True))
