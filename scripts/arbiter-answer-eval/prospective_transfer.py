# SPDX-License-Identifier: Apache-2.0
"""Run a local Gather transfer panel on a verified independent fixture; never read grader labels."""
import argparse
from functools import cache
import hashlib
import json
from pathlib import Path
from time import perf_counter
from unittest.mock import patch

import common
import finalist_gather_contract as contract
from finalist_joint_evidence import Evidence
import live_gather
import qwen_file_trial


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load_fixture(panel, expected):
    manifest_path = panel / "freeze.json"
    if digest(manifest_path.read_bytes()) != expected.lower():
        raise ValueError("Unexpected transfer design")
    manifest = common.read_json(manifest_path)
    calls, inventory = panel / "actors/calls.json", panel / "actors/source.json"
    if (digest(calls.read_bytes()) != manifest["requests_sha256"]
            or digest(inventory.read_bytes()) != manifest["source_manifest_sha256"]):
        raise ValueError("Actor inputs differ from the frozen design")
    files = common.read_json(inventory)
    source = panel / "source"
    if set(files) != {path for path, _ in live_gather.topical_index.text_files(source)}:
        raise ValueError("Fixture scope changed")
    repo = live_gather.WorkspaceRepository(source).refresh(set(files))
    if {p: h.hex() for p, h in repo.digests.items()} != files:
        raise ValueError("Frozen source bytes changed")
    rows = common.read_json(calls)
    if len(rows) != manifest["requests"] or len({r["id"] for r in rows}) != len(rows):
        raise ValueError("Unexpected caller inventory")
    for r in rows:
        contract.reply_cap(r["arguments"])
    return repo, rows, manifest


def deterministic(repo, args, identity):
    trace = {}
    with patch.object(repo, "search", cache(repo.search)):
        brief = qwen_file_trial.focused(repo, live_gather.gather(repo, args, trace), trace)
    proposals = [Evidence(p["at"], p["text"]) for p in brief["passages"]]
    return render_outcome(repo, args, proposals, len(trace["all_locations"]), identity, contract.render, [])


def render_outcome(repo, args, proposals, count, identity, renderer, batches):
    metrics = {"publication": "offline-fixture-sha256:" + identity, "scope_fingerprint": identity}
    begin = perf_counter()
    wire, kept = renderer(args, repo, "offline-transfer", metrics, proposals, count, 1, "line", attribution="bundled_file")
    if wire is None or len(wire) > contract.reply_cap(args):
        raise ValueError("Invalid transfer wire budget")
    body = json.loads(json.loads(wire)["result"]["content"][0]["text"])
    if [p["at"] for p in body["passages"]] != [p.at for p in kept]:
        raise ValueError("Wire and selection disagree")
    return {"wire_sha256": digest(wire), "wire": wire.decode(), "wire_bytes": len(wire),
            "source_bytes": sum(len(p["text"].encode()) for p in body["passages"]), "batches": batches,
            "logical_judgments": sum(b["pairs"] for b in batches), "render_seconds": perf_counter()-begin}


def compare_case(repo, args, scorer, identity, propose, spare):
    batches = []
    def measured(instruction, pairs):
        pairs = list(pairs)
        t = perf_counter()
        scores = scorer(instruction, pairs)
        qwen_file_trial.reorder(range(len(pairs)), scores)
        batches.append({"pairs": len(pairs), "input_sha256": digest(common.canonical_json([instruction, pairs])),
                        "scores_sha256": digest(common.canonical_json(scores)), "seconds": perf_counter()-t})
        return scores
    base = deterministic(repo, args, identity)
    start = perf_counter()
    proposals, count, policy = propose(repo, args, measured)
    selection_seconds = perf_counter() - start
    if policy.get("attribution") != "bundled_file" or not policy.get("spare_context"):
        raise ValueError("Finalist differs from frozen bundled plus spare policy")
    if not batches or not 0 < sum(b["pairs"] for b in batches) <= 40:
        raise ValueError("Bundled policy did not use its genuine scored file pool")
    arms = {"deterministic": base,
            "bundled": render_outcome(repo, args, proposals, count, identity, contract.render, batches),
            "finalist": render_outcome(repo, args, proposals, count, identity, spare, batches)}
    return {"variants": arms, "selection_sha256": digest(common.canonical_json(policy)),
            "shared_physical_judgments": sum(b["pairs"] for b in batches),
            "common_selection_seconds": selection_seconds}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("panel", type=Path)
    ap.add_argument("--freeze-sha256", required=True)
    ap.add_argument("--check-only", action="store_true")
    ap.add_argument("--server", type=Path)
    ap.add_argument("--policy-lock", type=Path, help="Frozen module-name to SHA-256 mapping from finalist owner")
    ap.add_argument("--output", type=Path)
    args = ap.parse_args()
    repo, cases, freeze = load_fixture(args.panel, args.freeze_sha256)
    if args.check_only:
        print(f"Verified {len(cases)} requests, {len(repo.texts)} frozen source files")
        return
    if args.server is None or args.policy_lock is None or args.output is None:
        ap.error("--server, --policy-lock and --output required for a full run")
    lock = common.read_json(args.policy_lock)
    required = {"range_choice_live.py", "finalist_spare_context.py", "finalist_joint_evidence.py", "passage_reply_budget.py", "finalist_gather_contract.py", "qwen_file_trial.py",
                "live_gather.py", "topical_index.py", "prospective_transfer.py"}
    if not required <= lock.keys():
        raise ValueError("Incomplete finalist implementation lock")
    for name, expected in lock.items():
        if Path(name).name != name or digest(Path(__file__).with_name(name).read_bytes()) != expected.lower():
            raise ValueError("Finalist source differs from its lock")
    import arbiter_judge_service
    from finalist_spare_context import render as spare
    from range_choice_live import propose
    score = arbiter_judge_service.verified_client(args.server)
    with args.output.open("x", encoding="utf-8") as f:
        def emit(row):
            print(json.dumps(row, ensure_ascii=True), file=f, flush=True)
        emit({"type": "run", "freeze_sha256": args.freeze_sha256, "policy_lock_sha256": digest(args.policy_lock.read_bytes()),
              "run_code_sha256": digest(Path(__file__).read_bytes()), "server_sha256": digest(args.server.read_bytes()),
              "scope": "Offline source-manifest eligibility. No native index, paid caller or agent cost."})
        for r in cases:
            result = compare_case(repo, r["arguments"], score, freeze["source_manifest_sha256"], propose, spare)
            emit({"type": "case", "id": r["id"], **result, "request_sha256": digest(common.canonical_json(r))})


if __name__ == "__main__":
    main()
