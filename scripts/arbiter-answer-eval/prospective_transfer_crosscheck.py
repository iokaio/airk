# SPDX-License-Identifier: Apache-2.0
"""Independent transport, evidence and model-cost cross-check of a sealed source-manifest transfer."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(path):
    return json.loads(path.read_bytes())


def coverage(observed, location, source, manifest):
    path = location["path"]
    first, last = location["lines"]
    lines = source[path]
    reference = ("\n".join(lines[first-1:last]) + "\n").encode()
    if not 1 <= first <= last <= len(lines) or sha(reference) != location["excerpt_sha256"] or manifest[path] != location["source_sha256"]:
        raise ValueError("Changed reference excerpt")
    required = set(range(first, last+1))
    seen = observed.get(path, set())
    return required <= seen, bool(required & seen)


def check(panel, freeze, lock, actor):
    freeze_path = panel / "freeze.json"
    lock_path = panel / "receipts/policy-lock.json"
    if sha(freeze_path.read_bytes()) != freeze or sha(lock_path.read_bytes()) != lock:
        raise ValueError("Design or finalist lock changed")
    config = read(freeze_path)
    manifest_path = panel / "actors/source.json"
    calls_path = panel / "actors/calls.json"
    refs_path = panel / "grader/references.json"
    for path, key in [(manifest_path, "source_manifest_sha256"), (calls_path, "requests_sha256"), (refs_path, "references_sha256")]:
        if sha(path.read_bytes()) != config[key]:
            raise ValueError("Design asset changed")
    manifest = read(manifest_path)
    source = {}
    for path, expected in manifest.items():
        raw = (panel / "source" / path).read_bytes()
        if sha(raw) != expected:
            raise ValueError("Unmatched source")
        source[path] = raw.decode("utf-8", errors="replace").splitlines()
    calls = {r["id"]: r for r in read(calls_path)}
    refs = {r["id"]: r["needs"] for r in read(refs_path)}
    header, *rows = [json.loads(s) for s in actor.read_bytes().splitlines()]
    if header["freeze_sha256"].lower() != freeze or header["policy_lock_sha256"].lower() != lock:
        raise ValueError("Actor ran a different finalist or panel")
    ids = [row["id"] for row in rows]
    if len(ids) != len(set(ids)) or set(ids) != calls.keys() or set(ids) != refs.keys():
        raise ValueError("Transfer inventory mismatch")
    result, totals = {}, {}
    for row in rows:
        key = row["id"]
        args = calls[key]["arguments"]
        arms = row["variants"]
        if set(arms) != {"deterministic", "bundled", "finalist"} or arms["bundled"]["batches"] != arms["finalist"]["batches"]:
            raise ValueError("Prompts or scoring batches changed between renderer arms")
        physical = row["shared_physical_judgments"]
        if not 0 < physical <= 40 or sum(b["pairs"] for b in arms["bundled"]["batches"]) != physical:
            raise ValueError("Unexpected shared judgments")
        result[key] = {}
        by_arm = {}
        for arm, raw in arms.items():
            if raw["logical_judgments"] != (0 if arm == "deterministic" else physical):
                raise ValueError("Incorrect logical model usage")
            wire = raw["wire"].encode()
            if (len(wire) != raw["wire_bytes"] or sha(wire) != raw["wire_sha256"] or
                    len(wire) > args.get("budget", {}).get("max_bytes", 131072)):
                raise ValueError("Wire cap or digest failed")
            envelope = json.loads(wire)["result"]
            if envelope["isError"] or len(envelope["content"]) != 1:
                raise ValueError("Transfer returned a tool error")
            body = json.loads(envelope["content"][0]["text"])
            if body["task"] != args["task"] or body["eligibility"] != {
                    "index_id": "offline-transfer", "publication": "offline-fixture-sha256:" + config["source_manifest_sha256"],
                    "scope_fingerprint": config["source_manifest_sha256"]}:
                raise ValueError("Task or source scope changed")
            if [n["text"] for n in body["needs"]] != args["needs"] or any(
                    n["status"] != "not_individually_scored" or n["where"] for n in body["needs"]):
                raise ValueError("Bundled arm makes an unsupported need attribution")
            observed = {}
            byte_count = 0
            for chunk in body["passages"]:
                path, span = chunk["at"].rsplit(":", 1)
                start, end = map(int, span.split("-"))
                if not 1 <= start <= end <= len(source[path]):
                    raise ValueError("Invalid displayed range")
                text = "\n".join(source[path][start-1:end]) + "\n"
                if (text != chunk["text"] or sha(text.encode()) != chunk["text_sha256"] or
                        manifest[path] != chunk["workspace_sha256"] or chunk["needs"] or chunk["support"]):
                    raise ValueError("Unverified text or an unsupported individual link")
                byte_count += len(text.encode())
                observed.setdefault(path, set()).update(range(start, end+1))
            if byte_count != raw["source_bytes"] or byte_count > 14000 or len(observed) > 6:
                raise ValueError("Passage limits exceeded")
            by_arm[arm] = observed
            counts = Counter()
            outcomes = []
            for need in refs[key]:
                if need.get("absent"):
                    counts["untagged_absence_needs"] += 1
                    outcomes.append("unscored_absence")
                    continue
                families = [[coverage(observed, loc, source, manifest) for loc in group] for group in need["required"]]
                if not families or any(not group for group in families):
                    raise ValueError("Positive need lacks reference evidence")
                whole = all(any(full for full, overlap in group) for group in families)
                partial = any(overlap for group in families for full, overlap in group)
                counts["positive_needs"] += 1
                counts["complete"] += whole
                counts["any_overlap"] += partial
                outcomes.append("whole" if whole else "partial" if partial else "missing")
            counts["source_bytes"] = byte_count
            counts["wire_bytes"] = len(wire)
            result[key][arm] = {"counts": counts, "outcomes": outcomes}
            totals.setdefault(arm, Counter()).update(counts)
        if any(not lines <= by_arm["finalist"].get(path, set()) for path, lines in by_arm["bundled"].items()):
            raise ValueError("Finalist removed a delivered baseline source line")
    return {"design_sha256": freeze, "policy_lock_sha256": lock, "actor_sha256": sha(actor.read_bytes()),
            "physical_judgments": sum(r["shared_physical_judgments"] for r in rows), "tasks": result, "totals": totals}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("panel", type=Path)
    parser.add_argument("freeze")
    parser.add_argument("lock")
    parser.add_argument("actor", type=Path)
    print(json.dumps(check(**vars(parser.parse_args())), sort_keys=True))
