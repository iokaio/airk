# SPDX-License-Identifier: Apache-2.0
"""Write the oracle fork collector's input for a live evaluation lifetime (Context trainer design, gate 1).

The input names, per repository, the live index id, the frozen source root, the live index's data
root (the collector opens it only as a read-only observer) and the live publication token, plus
one first ``airk_answer`` call per scenario. The calls are either the canonical first call (the
instruction verbatim, the change set's budget) or, with ``--series``, the first Answer call each
completed session recorded. The collector test pins the file by its framed digest, which this
script prints.

``python -I -B prepare_forks.py --handle <R>/fixture/<L>/handle.json --image-record <R>/fixture/images/<label>/images.json --out <R>/training/context-cand1/collector/input.json``
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
HARNESS = HERE.parents[2] / "arbiter-answer-eval"
if str(HARNESS) not in sys.path:
    sys.path.insert(0, str(HARNESS))

import common  # noqa: E402
import replay  # noqa: E402

INPUT_SCHEMA = "airk.arbiter.context-forks.input/1"
INPUT_DOMAIN = b"airk.arbiter.context-forks.input/1"
# Change set 1's Answer budget (decision 5, 22 Nov 2026; make_cs_spec.CS1_BUDGET).
CS1_BUDGET = {"calls": 12, "rounds": 3, "fanout": 12, "candidates": 32, "max_bytes": 524288,
              "input_tokens": 8192, "deadline_ms": 120000}
# Arguments the wrapper owns or the collector refuses (the index is the input's, per repository).
DROPPED = ("index", "project")
# Source that runs in the collector's own process: gathering, roster, packing and rendering. The
# inner reads run in the live service, so its search and store code are the image's by construction.
IN_PROCESS = ("crates/airk-kernel/src/", "crates/airk-core/src/", "crates/airk-retrieval/src/arbiter/",
              "crates/airk-mcp/src/")
COLLECTOR_OWN = ("crates/airk-mcp/src/context_action_capture_tests",)


def framed_digest(domain: bytes, fields: list[bytes]) -> str:
    """airk_core::identity::framed_digest: a prefix, then each field as a u64 big-endian length and bytes."""
    hash_ = hashlib.sha256(b"AIRK-DIGEST-1\0")
    for field in [domain, *fields]:
        hash_.update(len(field).to_bytes(8, "big"))
        hash_.update(field)
    return hash_.hexdigest()


def collection_name(data_root: Path) -> str:
    names = [p.name for p in (data_root / "collections-1.4").iterdir() if p.is_dir() and not p.name.startswith(".")]
    common.require(len(names) == 1, f"{data_root}: expected one collection, found {names}")
    return names[0]


def index_entry(repo: str, handle: common.Handle, indexes_dir: Path, token: str | None) -> dict:
    physical = (handle.aliases or {}).get(repo, repo)
    state = common.read_json(indexes_dir / f"{physical}.state.json")
    index_id = handle.index_id(repo)
    common.require(state["index_id"] == index_id,
                   f"{repo}: handle index {index_id} differs from {physical}.state.json {state['index_id']}")
    data_root = Path(state["data_dir"])
    common.require(data_root.is_dir(), f"{repo}: no index data root at {data_root}")
    if token is None:
        outcome = common.call_tool(handle, "airk_status", repo, {"response_mode": "full"}, 120.0,
                                   strip_env=common.AGENT_STRIPPED_ENV)
        body = common.envelope_body(outcome.envelope) if outcome.ok else None
        token = (body or {}).get("publication", {}).get("token") if isinstance(body, dict) else None
        common.require(isinstance(token, str), f"{repo}: live airk_status gave no publication token")
    collection = collection_name(data_root)
    policy_path = data_root / "collections-1.4" / collection / "source-policy.json"
    roots = (common.read_json(policy_path).get("policy") or {}).get("roots") or [] if policy_path.is_file() else []
    owners = [r["id"] for r in roots if r.get("owner")]
    common.require(len(owners) <= 1, f"{repo}: source policy names {len(owners)} owner roots")
    # The registered paths exactly as the service holds them: the collection owner binds both.
    return {"source_root_id": owners[0] if owners else None,
            "index_id": index_id, "root": state["root"], "data_root": state["data_dir"],
            "collection": collection, "binding": state["binding"],
            "publication_token": token}


def in_process(path: str) -> bool:
    name = path.rsplit("/", 1)[-1]
    return (path.startswith(IN_PROCESS) and not path.startswith(COLLECTOR_OWN)
            and not name.endswith("tests.rs") and "/tests/" not in path and path.endswith(".rs"))


def git(repo: Path, *args: str) -> str:
    env = dict(os.environ, GIT_OPTIONAL_LOCKS="0")
    return subprocess.run(["git", "-C", str(repo), *args], capture_output=True, check=True, env=env,
                          stdin=subprocess.DEVNULL).stdout.decode("utf-8")


def changed_paths(porcelain: str) -> set[str]:
    paths = set()
    for line in porcelain.splitlines():
        if len(line) > 3:
            path = line[3:].split(" -> ")[-1].strip().strip('"')
            paths.add(path)
    return paths


def source_check(record: dict, repo: Path, head: str, porcelain: str) -> dict:
    """The in-process source equals the image's: same HEAD, and every in-process file the image or the
    tree changed has the image's working-tree SHA-256 (``images.json`` ``changed_files``)."""
    changed = record.get("changed_files") or {}
    mismatches = []
    if head != record.get("git_head"):
        mismatches.append({"path": "HEAD", "image": record.get("git_head"), "tree": head})
    for path in sorted(p for p in set(changed) | changed_paths(porcelain) if in_process(p)):
        file = repo / path
        have = hashlib.sha256(file.read_bytes()).hexdigest() if file.is_file() else None
        if changed.get(path) != have:
            mismatches.append({"path": path, "image": changed.get(path), "tree": have})
    return {"image": record.get("label"), "git_head": record.get("git_head"),
            "checked": sum(1 for p in set(changed) | changed_paths(porcelain) if in_process(p)),
            "mismatches": mismatches}


def clean(args: dict) -> dict:
    return {k: v for k, v in args.items() if k not in DROPPED}


def canonical_calls(scenarios: list[dict], defaults: dict) -> list[dict]:
    calls = []
    for scenario in scenarios:
        _, args = replay.canonical_first_call("answer", scenario["instruction"], defaults)
        calls.append({"id": scenario["id"], "scenario_id": scenario["id"], "repository": scenario["repository"],
                      "arguments": clean(args)})
    return calls


def session_calls(series: Path, scenarios: dict, configurations: set[str], defaults: dict) -> list[dict]:
    calls = []
    for record_path in sorted(Path(series).glob("*/*/session.json")):
        record = common.read_json(record_path)
        scenario = scenarios.get(record.get("scenario_id"))
        if scenario is None or record.get("status") != "complete":
            continue
        if configurations and record.get("configuration", record.get("path")) not in configurations:
            continue
        first = next((t for t in common.read_jsonl(record_path.parent / "wrapper" / "turns.jsonl")
                      if t.get("turn") and t.get("tool") == "airk_answer"), None)
        if first is None:
            continue
        args = replay.apply_defaults("airk_answer", first.get("sent_arguments") or first["arguments"], defaults)
        name = f"{record_path.parent.parent.name}/{record_path.parent.name}"
        calls.append({"id": f"{scenario['id']}#{name}", "scenario_id": scenario["id"],
                      "repository": scenario["repository"], "arguments": clean(args)})
    return calls


def build(args: argparse.Namespace) -> dict:
    handle = common.load_handle(common.resolve_handle(args.handle, args.r / "fixture"))
    repos, slots = args.repos.split(","), args.slots.split(",")
    scenarios = [common.load_scenario(p) for p in common.scenario_files(args.scenarios_dir, repos, slots)]
    common.require(scenarios, "no scenarios selected")
    defaults = json.loads(args.answer_defaults)
    tokens = dict(item.split("=", 1) for item in args.publication) if args.publication else {}
    if args.series:
        calls = session_calls(args.series, {s["id"]: s for s in scenarios},
                              set(filter(None, args.configurations.split(","))), defaults)
    else:
        calls = canonical_calls(scenarios, defaults)
    common.require(calls, "no calls prepared")
    used = sorted({c["repository"] for c in calls})
    record = common.read_json(args.image_record)
    repo = common.REPO_ROOT
    check = source_check(record, repo, git(repo, "rev-parse", "HEAD").strip(),
                         git(repo, "status", "--porcelain=v1", "-uall"))
    common.require(not check["mismatches"] or args.allow_source_mismatch,
                   f"in-process source differs from image {record.get('label')}: {check['mismatches'][:5]}")
    provenance = {"handle": str(handle.path), "handle_sha256": handle.sha256,
                  "image_record": str(args.image_record), "image_record_sha256": common.sha256_file(args.image_record),
                  "source_check": check, "allow_source_mismatch": bool(args.allow_source_mismatch),
                  "series": str(args.series) if args.series else None, "configurations": args.configurations,
                  "prepared": common.now_iso()}
    return {"schema": INPUT_SCHEMA, "layers": args.layers, "cli": list(handle.cli), "control": handle.control,
            "environment": dict(handle.env), "call_timeout_ms": int(args.call_timeout * 1000),
            "assessment_input_tokens": args.assessment_input_tokens,
            "fork_ordinals": [int(o) for o in args.fork_ordinals.split(",")],
            "allow_live_focus": False,
            "indexes": {repo: index_entry(repo, handle, args.indexes_dir, tokens.get(repo))
                        for repo in used},
            "calls": calls, "provenance": provenance}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handle", required=True, help="handle.json, or @newest")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--r", type=Path, default=common.DEFAULT_R)
    parser.add_argument("--scenarios-dir", type=Path, default=common.DEFAULT_R / "scenarios")
    parser.add_argument("--indexes-dir", type=Path, default=common.DEFAULT_R / "fixture" / "indexes")
    parser.add_argument("--repos", default=",".join(common.REPOS))
    parser.add_argument("--slots", default=",".join(common.SLOTS))
    parser.add_argument("--series", type=Path, help="session series folder; default: canonical first calls")
    parser.add_argument("--configurations", default="", help="with --series: result names to keep")
    parser.add_argument("--answer-defaults", default=json.dumps({"budget": CS1_BUDGET}))
    parser.add_argument("--layers", default="all,-focus")
    parser.add_argument("--fork-ordinals", default="1")
    parser.add_argument("--assessment-input-tokens", type=int, default=2048)
    parser.add_argument("--call-timeout", type=float, default=300.0, help="seconds per inner CLI read")
    parser.add_argument("--publication", action="append", help="repo=token; skips the live airk_status read")
    parser.add_argument("--image-record", type=Path, required=True,
                        help="images.json of the image the lifetime runs (R/fixture/images/<label>/images.json)")
    parser.add_argument("--allow-source-mismatch", action="store_true",
                        help="record, do not refuse, in-process source that differs from the image")
    args = parser.parse_args(argv)
    common.require(not args.out.exists(), f"{args.out} exists")
    document = build(args)
    data = (json.dumps(document, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    common.write_bytes(args.out, data)
    print(json.dumps({"input": str(args.out), "calls": len(document["calls"]),
                      "sha256": hashlib.sha256(data).hexdigest(),
                      "AIRK_CONTEXT_FORKS_INPUT_DIGEST": framed_digest(INPUT_DOMAIN, [data])}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
