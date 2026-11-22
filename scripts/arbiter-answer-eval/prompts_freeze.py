# SPDX-License-Identifier: Apache-2.0
"""Freeze of the agent and judge prompts and the harness files (decision 5, 21 Nov 2026).

``write`` records the full SHA-256 (and text) of each agent system prompt (one
per path), the judge system prompt and the judge's verdict schema, plus the
SHA-256 of every runtime harness file. ``verify`` recomputes them and refuses
any difference, so every judged pass runs the frozen harness. ``fixture.py``,
its tests and its documentation belong to the fixture owner and are not part
of the freeze. Tests, the mock and this README are not runtime files.

``refreeze`` (from the CS3 re-freeze on) writes the new freeze, keeps the full previous and new
manifests beside the history as ``prompts-freeze-manifests/<sha256>.json``, and appends the history
line itself. Its ``prompts_changed`` is computed, never written by hand: the agent prompts, the judge
prompt, the judge verdict schema (the rubric) and the scenario freeze are compared with the previous
manifest. The line also names every changed harness file.

``python -I -B prompts_freeze.py --write <R>/design/prompts-freeze.json``
``python -I -B prompts_freeze.py --verify <R>/design/prompts-freeze.json``
``python -I -B prompts_freeze.py --refreeze <R>/design/prompts-freeze.json --reason "..."``
"""

from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402

SCHEMA = "airk.private.answer-eval-prompts-freeze/1"
# The loop (plan section 12) adds loop_reply.py (Answer /3 replies, used by evidence, coverage, replay, depth,
# run_pass and the wrapper) and gate.py (the LG tests, run by run_plan). The freeze written for the one-turn
# evaluation no longer verifies: it must be re-frozen (--refreeze) before the first judged loop pass (LG).
HARNESS_FILES = ("common.py", "coverage.py", "cs_replay.py", "depth.py", "evidence.py", "gate.py", "grade.py", "judge.py",
                 "loop_reply.py", "mcp_wrapper.py",
                 "pass_digests.py", "pass_report.py", "prompts_freeze.py", "rejudge.py", "replay.py", "rescore.py",
                 "run_baseline.py", "run_pass.py", "run_plan.py", "run_session.py", "search_replay.py", "settled.py",
                 "verify_restart.py")
SCENARIO_FREEZE = common.DEFAULT_R / "scenario-freeze.json"


def current() -> dict:
    import judge
    import run_session
    agent = {path: {"sha256": common.sha256_bytes(run_session.system_prompt(path).encode("utf-8")),
                    "text": run_session.system_prompt(path)} for path in sorted(common.PATH_TOOLS)}
    schema_text = json.dumps(judge.verdict_schema(), separators=(",", ":"), sort_keys=True)
    return {"agent_prompts": agent,
            "judge_prompt": {"sha256": common.sha256_bytes(judge.JUDGE_SYSTEM.encode("utf-8")), "text": judge.JUDGE_SYSTEM},
            "judge_schema": {"sha256": common.sha256_bytes(schema_text.encode("utf-8")), "text": schema_text},
            "agent_model": common.AGENT_MODEL, "agent_effort": common.AGENT_EFFORT,
            "judge_model": common.JUDGE_MODEL, "judge_effort": common.JUDGE_EFFORT, "turn_cap": common.TURN_CAP,
            "scenario_freeze": {"path": str(SCENARIO_FREEZE),
                                "sha256": common.sha256_file(SCENARIO_FREEZE) if SCENARIO_FREEZE.is_file() else None},
            "files": {name: common.sha256_file(HERE / name) for name in HARNESS_FILES}}


def prompt_changes(previous: dict, now: dict) -> list[str]:
    """The prompt, rubric and scenario entries that differ between two manifests."""
    out = []
    for path in sorted(set(previous.get("agent_prompts") or {}) | set(now.get("agent_prompts") or {})):
        if ((previous.get("agent_prompts") or {}).get(path) or {}).get("sha256") != \
                ((now.get("agent_prompts") or {}).get(path) or {}).get("sha256"):
            out.append(f"agent prompt for {path}")
    for key, label in (("judge_prompt", "judge prompt"), ("judge_schema", "rubric (judge verdict schema)"),
                       ("scenario_freeze", "scenario freeze")):
        if (previous.get(key) or {}).get("sha256") != (now.get(key) or {}).get("sha256"):
            out.append(label)
    return out


def file_changes(previous: dict, now: dict) -> list[str]:
    names = set(previous.get("files") or {}) | set(now.get("files") or {})
    return sorted(n for n in names if (previous.get("files") or {}).get(n) != (now.get("files") or {}).get(n))


def keep_manifest(freeze: Path, directory: Path) -> Path:
    digest = common.sha256_file(freeze)
    target = directory / f"{digest}.json"
    if not target.is_file():
        directory.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(freeze, target)
    common.require(common.sha256_file(target) == digest, f"kept manifest {target} differs from its name")
    return target


def refreeze(path: Path, reason: str) -> dict:
    common.require(bool(reason.strip()), "a re-freeze needs a reason")
    manifests = path.parent / "prompts-freeze-manifests"
    previous, old = ({}, None)
    if path.is_file():
        previous = common.read_json(path)
        old = keep_manifest(path, manifests)
    record = write(path)
    new = keep_manifest(path, manifests)
    changes = prompt_changes(previous, record) if previous else None
    entry = {"at": common.now_iso(), "old_sha256": old.stem if old else None, "new_sha256": new.stem,
             "prompts_changed": bool(changes) if changes is not None else None, "prompt_changes": changes,
             "files_changed": file_changes(previous, record), "reason": reason,
             "old_manifest": str(old) if old else None, "new_manifest": str(new), "computed": True}
    common.append_jsonl(path.parent / "prompts-freeze-history.jsonl", entry)
    return entry


def differences(frozen: dict, now: dict) -> list[str]:
    out = []
    for path, entry in now["agent_prompts"].items():
        if (frozen.get("agent_prompts") or {}).get(path, {}).get("sha256") != entry["sha256"]:
            out.append(f"agent prompt for {path}")
    for key in ("judge_prompt", "judge_schema"):
        if (frozen.get(key) or {}).get("sha256") != now[key]["sha256"]:
            out.append(key)
    for key in ("agent_model", "agent_effort", "judge_model", "judge_effort", "turn_cap"):
        if frozen.get(key) != now[key]:
            out.append(key)
    for name, digest in now["files"].items():
        if (frozen.get("files") or {}).get(name) != digest:
            out.append(f"file {name}")
    return out


def write(path: Path) -> dict:
    record = {"schema": SCHEMA, "frozen_at": common.now_iso(), **current()}
    common.write_json(path, record)
    return record


def verify(path: Path) -> dict:
    frozen = common.read_json(path)
    common.require(frozen.get("schema") == SCHEMA, f"{path} is not a prompts freeze")
    diff = differences(frozen, current())
    common.require(not diff, f"harness differs from the prompts freeze {path}: {diff}")
    return {"path": str(path), "sha256": common.sha256_file(path), "frozen_at": frozen.get("frozen_at")}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", type=Path)
    group.add_argument("--verify", type=Path)
    group.add_argument("--refreeze", type=Path)
    parser.add_argument("--reason", default="")
    args = parser.parse_args(argv)
    if args.refreeze:
        print(json.dumps(refreeze(args.refreeze, args.reason), indent=1))
    elif args.write:
        record = write(args.write)
        print(json.dumps({"written": str(args.write), "sha256": common.sha256_file(args.write),
                          "agent": {k: v["sha256"] for k, v in record["agent_prompts"].items()},
                          "judge": record["judge_prompt"]["sha256"]}, indent=1))
    else:
        print(json.dumps(verify(args.verify)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
