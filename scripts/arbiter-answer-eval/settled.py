# SPDX-License-Identifier: Apache-2.0
"""Settled-index guard for judged passes (primary agent's decision 1, 21 Nov 2026).

A judged pass runs only when every index it reads reports:

* semantic readiness ``ready`` (``airk_status``),
* served mode ``hybrid`` (a one-hit ``airk_search`` with ``include_status``), and
* no ``index`` or ``vectors`` job queued or running (CLI ``--service-project
  <binding> progress``: ``observation.job``, ``pending_job``, ``queued_jobs``).

The publication token is taken at the start of the pass and again at the end.
A changed token invalidates the pass. ``snapshot`` is read-only: it starts,
cancels and changes nothing.
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402

BLOCKING_KINDS = ("index", "vectors")


def unfinished_jobs(progress: dict) -> list[dict]:
    observation = progress.get("observation") or {}
    jobs = [observation.get("job"), progress.get("pending_job"), *(progress.get("queued_jobs") or [])]
    seen, out = set(), []
    for job in jobs:
        if isinstance(job, dict) and job.get("state") in ("pending", "running") and job.get("id") not in seen:
            seen.add(job.get("id"))
            out.append({"id": job.get("id"), "kind": job.get("kind"), "state": job.get("state")})
    return out


def _progress(handle: common.Handle, binding: str, timeout_s: float) -> dict:
    command = [*handle.cli, "--output", "json", "--service-runtime-dir", handle.control,
               "--service-project", binding, "progress"]
    proc = subprocess.run(command, capture_output=True, stdin=subprocess.DEVNULL, env=common.tool_env(handle),
                          timeout=timeout_s, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    common.require(proc.returncode == 0, f"progress failed (exit {proc.returncode})")
    return json.loads(proc.stdout)


def evaluate(status: dict, search: dict, progress: dict | None) -> dict:
    """Pure decision over the three observations (unit-tested)."""
    semantic = (status.get("semantic_readiness") or {}).get("state")
    served = (search.get("execution") or {}).get("served_mode")
    token = (status.get("publication") or {}).get("token")
    search_token = (search.get("publication") or {}).get("token")
    jobs = unfinished_jobs(progress) if progress is not None else None
    problems = []
    if semantic != "ready":
        problems.append(f"semantic readiness is {semantic!r}, not 'ready'")
    if served != "hybrid":
        problems.append(f"served mode is {served!r}, not 'hybrid'")
    if jobs is None:
        problems.append("job progress unavailable")
    elif any(job["kind"] in BLOCKING_KINDS for job in jobs):
        problems.append(f"index/vectors jobs queued or running: {[j for j in jobs if j['kind'] in BLOCKING_KINDS]}")
    if not token:
        problems.append("no publication token")
    elif search_token and search_token != token:
        problems.append("status and search publication tokens differ")
    return {"settled": not problems, "problems": problems, "semantic_state": semantic, "served_mode": served,
            "execution_incomplete": (search.get("execution") or {}).get("incomplete"),
            "publication_token": token, "unfinished_jobs": jobs}


def snapshot(handle: common.Handle, repo: str, timeout_s: float = 120.0) -> dict:
    raw = common.read_json(handle.path)
    binding = handle.binding(repo) or ((raw.get("indexes") or {}).get(repo) or {}).get("binding")
    # Typed retries (selection_busy, ...) are honoured: the airk baseline's first start guard met one (22 Nov 2026).
    status_out, _ = common.call_tool_retrying(handle, "airk_status", repo, {"include_status": True}, timeout_s)
    search_out, _ = common.call_tool_retrying(handle, "airk_search", repo, {"query": "settled index check",
                                                                            "limit": 1, "include_status": True},
                                              timeout_s)
    status = common.envelope_body(status_out.envelope) if status_out.ok else {}
    search = common.envelope_body(search_out.envelope) if search_out.ok else {}
    progress = None
    if binding:
        try:
            progress = _progress(handle, binding, timeout_s)
        except (common.HarnessError, ValueError, subprocess.TimeoutExpired, OSError):
            progress = None
    result = evaluate(status if isinstance(status, dict) else {}, search if isinstance(search, dict) else {}, progress)
    result["call_errors"] = {name: {"error": out.error, "elapsed_ms": out.elapsed_ms,
                                    "stdout_head": out.stdout[:300].decode("utf-8", "replace")}
                             for name, out in (("status", status_out), ("search", search_out)) if not out.ok}
    result.update(at=common.now_iso(), repo=repo, index_id=handle.index_id(repo), binding=binding,
                  handle=str(handle.path), handle_sha256=handle.sha256, lifetime=raw.get("lifetime"))
    return result


def compare(start: dict, end: dict) -> dict:
    same = bool(start.get("publication_token")) and start.get("publication_token") == end.get("publication_token")
    return {"repo": start.get("repo"), "unchanged": same, "start_token": start.get("publication_token"),
            "end_token": end.get("publication_token"), "start_at": start.get("at"), "end_at": end.get("at")}


def check(handle_arg: str, fixture_dir: Path, repos: list[str], accept_queryable: bool) -> tuple[bool, list[dict]]:
    path = common.resolve_handle(handle_arg, fixture_dir)
    try:
        handle = common.load_handle(path, accept_queryable=accept_queryable)
    except common.HarnessError as error:  # e.g. a new lifetime still starting
        return False, [{"handle": str(path), "settled": False, "problems": [str(error)], "at": common.now_iso()}]
    snaps = []
    for repo in repos:
        began = time.monotonic()
        snap = snapshot(handle, repo)
        snap["elapsed_ms"] = int((time.monotonic() - began) * 1000)
        snaps.append(snap)
    return all(s["settled"] for s in snaps), snaps


def main(argv: list[str] | None = None) -> int:
    import argparse
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handle", required=True, help="a handle.json, or @newest (the newest fixture lifetime)")
    parser.add_argument("--fixture-dir", type=Path, default=common.DEFAULT_R / "fixture")
    parser.add_argument("--repos", default=",".join(common.REPOS))
    parser.add_argument("--accept-queryable", action="store_true")
    parser.add_argument("--wait", action="store_true", help="poll until settled (exit 0) or --timeout (exit 2)")
    parser.add_argument("--interval", type=float, default=60.0)
    parser.add_argument("--timeout", type=float, default=6 * 3600.0)
    parser.add_argument("--log", type=Path, help="append every poll here (JSONL)")
    args = parser.parse_args(argv)
    repos = args.repos.split(",")
    end = time.monotonic() + args.timeout
    while True:
        ok, snaps = check(args.handle, args.fixture_dir, repos, args.accept_queryable)
        if args.log:
            for snap in snaps:
                common.append_jsonl(args.log, snap)
        if ok or not args.wait:
            for snap in snaps:
                print(json.dumps(snap))
            return 0 if ok else 1
        if time.monotonic() >= end:
            for snap in snaps:
                print(json.dumps(snap))
            return 2
        time.sleep(args.interval)


if __name__ == "__main__":
    sys.exit(main())
