# SPDX-License-Identifier: Apache-2.0
"""Resume the verified two-task pilot using the unchanged scorer and a fresh merged output."""

import argparse
from functools import cache
import hashlib
import json
from pathlib import Path
from unittest.mock import patch

import prompt_need_audit as audit
import qwen_need_trial as trial
from qwen_replay_conformance import load_verified_judge, verify_control


PILOT_SHA = "e884a62d497f6a520667cd2ca33a481a99b0ace0caeac2e8cc29864382ceed36"
COMPLETED = ["bookbuilder-A1", "bookbuilder-C3"]


def require(ok, message):
    if not ok:
        raise ValueError(message)


def preflight(campaign, checkpoint, pilot):
    raw = pilot.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == PILOT_SHA and raw.endswith(b"\n"), "Pilot bytes changed")
    header, *completed = map(json.loads, raw.splitlines())
    directory = pilot.parent
    paused = trial.common.read_json(directory / "paused-by-request.json")
    require(paused["sha256"].lower() == PILOT_SHA and paused["completed"] == COMPLETED and
            [r["task"] for r in completed] == COMPLETED, "Wrong completed task inventory")
    admission = trial.common.read_json(directory / "input-admission.json")
    launch = trial.common.read_json(directory / "launch.json")
    for name, digest in launch["files"].items():
        require(trial.common.sha256_file(Path(__file__).with_name(name)).lower() == digest.lower(),
                f"Original scorer dependency changed: {name}")
    calls_path = campaign / "replay/first-calls-r1.json"
    old_path = campaign / "replay/qwen-file-top40-r1.jsonl"
    require(trial.common.sha256_file(calls_path).lower() == header["calls_sha256"].lower() == admission["first_calls_sha256"].lower()
            and trial.common.sha256_file(old_path).lower() == header["historical_sha256"].lower() == admission["retained_qwen_sha256"].lower(),
            "Frozen calls or Qwen control receipts changed")
    calls = trial.common.read_json(calls_path)["calls"]
    old = {r["task"]: r for r in trial.common.read_jsonl(old_path) if "task" in r}
    rows = {r["id"]: r for r in trial.agent_cost_batch.working_set(campaign)}
    require(len(rows) == 20 and sorted(rows) == header["tasks"] and sorted(rows)[:2] == COMPLETED and
            rows.keys() == calls.keys() == old.keys() == admission["frozen_tasks"].keys() and
            header["arms"] == list(trial.ARMS) and header["width"] == trial.legacy.WIDTH and
            header["excerpt_chars"] == trial.legacy.EXCERPT_CHARS and header["focus_bytes"] == trial.BUDGET and
            Path(header["model"]["checkpoint"]).resolve() == checkpoint.resolve(), "Experiment configuration differs")
    for task_id, row in rows.items():
        frozen = admission["frozen_tasks"][task_id]
        src = row["snapshot"]
        require(src.name == frozen["source_folder"] and trial.common.sha256_file(row["task"]).lower() == frozen["scenario_sha256"]
                and trial.common.sha256_file(campaign / "topical" / f"{src.name}.map.json").lower() == frozen["map_sha256"],
                f"Original task or map changed: {task_id}")
    for row in completed:
        key = row["task"]
        audit.verify_pairing(row, calls, old[key])
        source = campaign / "agent-cost/E2E-r1/snapshots/bookbuilder"
        require(Path(row["source"]).resolve() == source.resolve() and
                all(not (source / p).exists() for _, p in trial.fixture.EXCLUSIONS["bookbuilder"]),
                "Invalid sanitized pilot source")
        audit.verify_model_inputs(row, source)
    return raw, header, calls, old, rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    parser.add_argument("checkpoint", type=Path)
    parser.add_argument("--resume", required=True, type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--preflight-only", action="store_true")
    args = parser.parse_args()
    if args.preflight_only == bool(args.output):
        parser.error("Choose --preflight-only or a new --output")
    raw, header, calls, old, rows = preflight(args.campaign, args.checkpoint, args.resume)
    if args.preflight_only:
        print("Verified two pinned tasks and frozen inputs for eighteen remaining tasks.")
        return
    if args.output.exists():
        parser.error("Output must be new")
    judge, receipt = load_verified_judge(args.checkpoint)
    require(receipt == header["model"], "Loaded model and tokenizer differ from pilot")
    completed = set(COMPLETED)
    repos = {}
    with args.output.open("xb") as fp:
        fp.write(raw)
        fp.flush()
        for key in header["tasks"]:
            if key in completed:
                continue
            meta = rows[key]
            src = meta["snapshot"]
            if src not in repos:
                repos[src] = trial.safe_repository(args.campaign, meta)
            repo = repos[src]
            with patch.object(repo, "search", cache(repo.search)):
                pool, variants = trial.run_task(repo, calls[key], trial.common.read_json(meta["task"]), old[key],
                                                judge, header["arms"], verify_control, key, src)
            result = {"type": "task", "task": key, "arguments": calls[key], "task_sha256": trial.common.sha256_file(meta["task"]),
                      "historical_map_sha256": trial.common.sha256_file(args.campaign / "topical" / f"{src.name}.map.json"),
                      "actual_file_records_sha256": trial.sha(json.dumps(repo.index["files"], sort_keys=True, separators=(",", ":"))),
                      "source": str(repo.source), "pool": pool, "variants": variants}
            fp.write((json.dumps(result, ensure_ascii=True) + "\n").encode("utf-8"))
            fp.flush()
            completed.add(key)
    require(completed == set(header["tasks"]), "Incomplete merged panel")


if __name__ == "__main__":
    main()
