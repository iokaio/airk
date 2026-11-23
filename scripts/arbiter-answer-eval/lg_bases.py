# SPDX-License-Identifier: Apache-2.0
"""Watch, probe and summarize the CPU base lifetimes of the LG arms (one fixture root per arm).

Evaluation tooling, never product. A base is the closed ``fixture.py`` lifetime
``<series>-<arm>-base`` in the root ``R/fixture-<series>-<arm>``, which the
arm's CUDA lifetime restarts from (GPU_FIXTURE.md, "LG arms side by side").
This tool starts and stops nothing: the bases are started and stopped with
``fixture.py`` as GPU_FIXTURE.md shows.

* ``status`` prints one line per base: state, readiness and indexing times.
* ``wait --until ready|semantic|closed --seconds N`` waits on the lifetimes'
  own handles until every base has reached the state, one has failed, or the
  deadline passes (exit 0, 1, 2).
* ``probe`` runs, for every base that is ``ready``: the settled-index guard
  on the pair's repositories, one ``airk_status`` per index (files, chunks,
  vectors, pipeline digest), the leak check (``leak_check.py``), the
  credential probe on each index and the launcher's own ``leak-probe``. It
  writes the receipts into the root and ``base-probe.json`` beside them.
* ``summary --out FILE`` collects, per base: state, restartable, error, the
  handle path and, per index, the indexing seconds, file, chunk and vector
  counts, the publication token and the index pipeline digest.

``python -I -B lg_bases.py --series 06-gate-lg --arms C,L,F,C32,L32 status``
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import credential_probe  # noqa: E402
import leak_check  # noqa: E402
import settled  # noqa: E402

ARMS = ("C", "L", "F", "C32", "L32")
SCHEMA = "airk.private.answer-eval-lg-bases/1"
CONDITIONS = ("ready", "semantic", "closed")


def base_paths(r_root: Path, series: str, arm: str, lifetime: str | None = None) -> dict:
    """One arm's base. ``lifetime`` names it when it is not ``<series>-<arm>-base``: a base whose first
    lifetime ended on an error after its service settled is continued by a second lifetime that restarts
    from it (``--base L=06-gate-lg-L-base2``), and that one is then the arm's restart origin."""
    root = Path(r_root) / f"fixture-{series}-{arm}"
    name = lifetime or f"{series}-{arm}-base"
    return {"arm": arm, "root": root, "lifetime": name, "life": root / name, "handle": root / name / "handle.json"}


def named_bases(values: list[str]) -> dict[str, str]:
    """``--base ARM=LIFETIME`` options as a map; a repeated arm or a value without ``=`` is refused."""
    found: dict[str, str] = {}
    for value in values:
        arm, separator, lifetime = value.partition("=")
        common.require(bool(separator) and bool(arm) and bool(lifetime) and arm not in found,
                       f"--base takes ARM=LIFETIME once per arm, not {value!r}")
        found[arm] = lifetime
    return found


def read(path: Path):
    try:
        return json.loads(Path(path).read_bytes())
    except (OSError, ValueError):
        return None


def wait_verdict(handle: dict | None, ended: bool, condition: str) -> tuple[str, str | None]:
    """``done``, ``waiting`` or ``failed`` for one base, with the reason.

    ``ready``: both lexical passes are done; ``semantic``: both indexes are semantically ready; ``closed``:
    the lifetime closed cleanly. An error in the handle, or a start process that ended before the wanted
    state, is a failure; ``ended`` says whether the launcher wrote ``owned.json``."""
    common.require(condition in CONDITIONS, f"unknown condition {condition!r}")
    handle = handle or {}
    if handle.get("error"):
        return "failed", str(handle["error"])[:300]
    if condition == "ready":
        done = handle.get("state") == "ready"
    elif condition == "semantic":
        done = handle.get("state") == "ready" and bool(handle.get("semantic_ready_utc"))
    else:
        done = handle.get("state") == "closed" and handle.get("restartable") is True
    if done:
        return "done", None
    if handle.get("stop_reason") and condition != "closed":
        return "failed", "stopped: " + str(handle["stop_reason"])[:300]
    if ended:
        return "failed", f"the start process ended in state {handle.get('state')!r}"
    return "waiting", handle.get("state")


def index_row(repo: str, record: dict, state: dict, status: dict | None) -> dict:
    """One index's facts from the handle record, the repository state file and a probe's ``airk_status``."""
    indexed = (state or {}).get("indexed") or {}
    status = status or {}
    readiness = status.get("semantic_readiness") or {}
    return {"repository": repo, "index_id": record.get("index_id"), "source_commit": record.get("source_commit"),
            "indexing_seconds": indexed.get("seconds"), "index_job_seconds": indexed.get("job_seconds"),
            "index_pass_queued_by": indexed.get("queued_by"),
            "denies": record.get("denies"),
            "trims": [{k: t.get(k) for k in ("kind", "path", "outcome", "before_index", "idle_wait_seconds",
                                             "removed_artifacts")} | {"busy_refusals": len(t.get("busy_refusals") or [])}
                      for t in (state or {}).get("trims") or []],
            "files": status.get("files", record.get("files")), "chunks": status.get("chunks"),
            "indexed_vectors": status.get("indexed_vectors"),
            "semantic_state": readiness.get("state"), "semantic_completed": readiness.get("completed_artifacts"),
            "semantic_selected": readiness.get("selected_artifacts"),
            "publication_token": (status.get("publication") or {}).get("token") or record.get("publication_token"),
            "pipeline_digest": readiness.get("pipeline_digest") or record.get("pipeline_digest"),
            "served_mode": record.get("served_mode"), "semantic_ready_utc": record.get("semantic_ready_utc"),
            "data_bytes": record.get("data_bytes")}


def summary_row(paths: dict, handle: dict | None, states: dict, probe: dict | None) -> dict:
    """One base's facts. ``restart_origin`` is true only for a closed, restartable lifetime without an error."""
    handle = handle or {}
    statuses = (probe or {}).get("status") or {}
    indexes = {repo: index_row(repo, record, states.get(repo) or {}, statuses.get(repo))
               for repo, record in (handle.get("indexes") or {}).items()}
    return {"arm": paths["arm"], "root": str(paths["root"]), "lifetime": paths["lifetime"],
            "handle": str(paths["handle"]), "state": handle.get("state"), "restartable": handle.get("restartable"),
            "error": handle.get("error"), "stop_reason": handle.get("stop_reason"),
            "restart_blockers": handle.get("restart_blockers"), "service_exit_code": handle.get("service_exit_code"),
            "restart_origin": (handle.get("state") == "closed" and handle.get("restartable") is True
                               and not handle.get("error")),
            "image": common.image_identity(handle.get("image")),
            "fixture_py_sha256": (handle.get("helpers") or {}).get("fixture.py"),
            "started_utc": handle.get("started_utc"), "ready_utc": handle.get("ready_utc"),
            "semantic_ready_utc": handle.get("semantic_ready_utc"), "closed_utc": handle.get("closed_utc"),
            "indexes": indexes, "holds_airk_index": common.FORBIDDEN_LOOP_INDEX in indexes,
            "probe": None if probe is None else {k: probe.get(k) for k in ("at", "passed", "settled", "leak_check",
                                                                           "fixture_leak_probe", "credentials")}}


def states_of(root: Path) -> dict:
    return {path.name.split(".")[0]: read(path) for path in sorted((root / "indexes").glob("*.state.json"))
            if path.name != "minilm.state.json"}


def status_body(handle: common.Handle, repo: str) -> dict | None:
    outcome, _ = common.call_tool_retrying(handle, "airk_status", repo, {"response_mode": "full"}, 120.0)
    body = common.envelope_body(outcome.envelope) if outcome.ok else None
    return body if isinstance(body, dict) else None


def probe_base(paths: dict, launcher_probe: bool = True, skip_reason: str | None = None) -> dict:
    """Every read-only check of one ready base; the receipts go into its root. ``launcher_probe`` false
    leaves out ``fixture.py leak-probe`` (the leak check covers the same ground) and records why."""
    root, handle_path = paths["root"], paths["handle"]
    handle = common.load_handle(handle_path)
    repos = list(common.REPOS)
    ok, snaps = settled.check(str(handle_path), root, repos, False)
    result = {"schema": SCHEMA, "arm": paths["arm"], "at": common.now_iso(), "handle": str(handle_path),
              "handle_sha256": handle.sha256,
              "settled": {"ok": ok, "repositories": {s.get("repo"): {k: s.get(k) for k in (
                  "settled", "problems", "semantic_state", "served_mode", "publication_token", "unfinished_jobs")}
                  for s in snaps}}}
    result["status"] = {leak_check.index_name(handle, repo): status_body(handle, repo) for repo in repos}
    leak_out = root / "leak-check.json"
    leak_code = leak_check.main(["--handle", str(handle_path), "--out", str(leak_out)])
    result["leak_check"] = {"exit_code": leak_code, "path": str(leak_out), "sha256": common.sha256_file(leak_out),
                            "passed": (read(leak_out) or {}).get("passed")}
    result["credentials"] = {}
    for repo in repos:
        index = leak_check.index_name(handle, repo)
        out = root / f"credential-probe-{index}.json"
        credential_probe.main(["--handle", str(handle_path), "--repo", repo, "--out", str(out)])
        report = read(out) or {}
        result["credentials"][index] = {"path": str(out), "sha256": common.sha256_file(out),
                                        "listed_paths": report.get("listed_paths"),
                                        "complete": report.get("complete"),
                                        "token_unchanged": report.get("token_unchanged"),
                                        "match_count": report.get("match_count"), "matches": report.get("matches")}
    launcher_ok = True
    if launcher_probe:
        command = [sys.executable, "-I", "-B", "-S", str(HERE / "fixture.py"), "--fixtures", str(root),
                   "leak-probe", "--lifetime", paths["lifetime"]]
        proc = subprocess.run(command, capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=3600,
                              cwd=str(common.REPO_ROOT), creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        last = proc.stdout.strip().splitlines()[-1] if proc.stdout.strip() else ""
        try:
            printed = json.loads(last)
        except ValueError:
            printed = {"stdout_tail": proc.stdout[-500:], "stderr_tail": proc.stderr[-1000:]}
        result["fixture_leak_probe"] = {"exit_code": proc.returncode, **printed}
        launcher_ok = proc.returncode == 0
    else:
        result["fixture_leak_probe"] = {"skipped": skip_reason or "not requested", "passed": None}
    result["passed"] = (ok and leak_code == 0 and launcher_ok
                        and all(row.get("complete") for row in result["credentials"].values()))
    common.write_json(root / "base-probe.json", result)
    return result


def disk_sample(args) -> dict | None:
    """Append the free bytes of the fixture roots' volume to ``--disk-log`` (the shared disk budget's watch)."""
    if not args.disk_log:
        return None
    import shutil
    row = {"utc": common.now_iso(), "command": args.command, "free_bytes": shutil.disk_usage(args.r_root).free,
           "floor_bytes": args.disk_floor}
    row["headroom_bytes"] = None if args.disk_floor is None else row["free_bytes"] - args.disk_floor
    common.append_jsonl(args.disk_log, row)
    return row


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    parser.add_argument("--series", required=True)
    parser.add_argument("--arms", default=",".join(ARMS))
    parser.add_argument("--base", action="append", default=[], metavar="ARM=LIFETIME",
                        help="an arm's base lifetime when it is not <series>-<arm>-base (a restarted base)")
    parser.add_argument("--disk-log", type=Path, help="append the volume's free bytes here at every run (JSONL)")
    parser.add_argument("--disk-floor", type=int, help="the volume's required free bytes, recorded with each sample")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("status")
    waiting = commands.add_parser("wait")
    waiting.add_argument("--until", choices=CONDITIONS, required=True)
    waiting.add_argument("--seconds", type=float, required=True)
    waiting.add_argument("--interval", type=float, default=5.0)
    probing = commands.add_parser("probe")
    probing.add_argument("--skip-launcher-probe", metavar="REASON",
                         help="leave out fixture.py's leak-probe and record this reason (the leak check stays)")
    summary = commands.add_parser("summary")
    summary.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    named = named_bases(args.base)
    arms = [arm for arm in args.arms.split(",") if arm]
    common.require(set(named) <= set(arms), f"--base names arms outside --arms: {sorted(set(named) - set(arms))}")
    bases = [base_paths(args.r_root, args.series, arm, named.get(arm)) for arm in arms]
    disk_sample(args)
    if args.command == "wait":
        end = time.monotonic() + args.seconds
        while True:
            rows = {b["arm"]: wait_verdict(read(b["handle"]), (b["life"] / "owned.json").is_file(), args.until)
                    for b in bases}
            failed = any(v[0] == "failed" for v in rows.values())
            done = all(v[0] == "done" for v in rows.values())
            late = time.monotonic() > end
            if done or failed or late:
                disk_sample(args)
                print(json.dumps({"until": args.until, "at": common.now_iso(), "deadline_passed": late and not done,
                                  "arms": {arm: {"verdict": v[0], "detail": v[1]} for arm, v in rows.items()}}))
                return 0 if done else 1 if failed else 2
            # Bounded sampling of the lifetimes' own handles, under the deadline above.
            time.sleep(args.interval)
    if args.command == "probe":
        passed = True
        for b in bases:
            result = probe_base(b, launcher_probe=not args.skip_launcher_probe,
                                skip_reason=args.skip_launcher_probe)
            passed = passed and result["passed"]
            print(json.dumps({"arm": b["arm"], "passed": result["passed"], "settled": result["settled"]["ok"],
                              "leak_check": result["leak_check"]["passed"],
                              "fixture_leak_probe": result["fixture_leak_probe"].get("passed"),
                              "credential_matches": {k: v["matches"] for k, v in result["credentials"].items()}}))
        return 0 if passed else 1
    rows = [summary_row(b, read(b["handle"]), states_of(b["root"]), read(b["root"] / "base-probe.json"))
            for b in bases]
    if args.command == "summary":
        common.write_json(args.out, {"schema": SCHEMA, "series": args.series, "generated": common.now_iso(),
                                     "bases": rows})
    for row in rows:
        print(json.dumps({"arm": row["arm"], "state": row["state"], "restart_origin": row["restart_origin"],
                          "error": row["error"], "probe_passed": (row["probe"] or {}).get("passed"),
                          "indexes": {repo: {k: index.get(k) for k in ("indexing_seconds", "files", "chunks",
                                                                       "semantic_state", "pipeline_digest")}
                                      for repo, index in row["indexes"].items()}}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
