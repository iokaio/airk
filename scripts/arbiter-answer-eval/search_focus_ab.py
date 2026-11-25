# SPDX-License-Identifier: Apache-2.0
"""Compare public Search with Focus off and on across the judged CS1 controls."""

from __future__ import annotations

import argparse
import csv
import json
import math
from pathlib import Path
import re
from statistics import median
import subprocess
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import depth
import evidence
from initial_search_depth import under
import settled


SCHEMA = "airk.private.answer-eval-search-focus-ab/1"
SERIES = "08-windows-cuda-cs1-r2"
CUDA = "onnxruntime-cuda/1.30.0/device-execution/"
SHA = re.compile(r"[a-f0-9]{64}")
DEFAULT_LIMIT = 10
DEFAULT_MAX_BYTES = 8192
EXPECTED = {(repo, slot) for repo in common.REPOS for slot in common.SLOTS}


def digest(value) -> str:
    return common.sha256_bytes(common.canonical_json(value))


def pinned(path: Path, expected: str) -> bytes:
    raw = path.read_bytes()
    common.require(common.sha256_bytes(raw) == expected, f"{path.name}: SHA-256 differs")
    return raw


def write_new(path: Path, document: dict) -> None:
    with path.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(document, output, ensure_ascii=False, indent=2)
        output.write("\n")


def roster(rows: list[dict]) -> list[tuple[str, str]]:
    identity = [(r["repo"], r["slot"]) for r in rows]
    common.require(set(identity) == EXPECTED and len(identity) == len(EXPECTED),
                   "expected one row for each of the 20 CS1 Search scenarios")
    return identity


def freeze(pass_dir: Path, index_sha: str, results_sha: str) -> dict:
    pass_dir = pass_dir.resolve(strict=True)
    index = json.loads(pinned(pass_dir / "pass-index.json", index_sha))
    common.require(index.get("schema") == "airk.private.answer-eval-pass-index/1"
                   and index["series"] == SERIES, "expected judged Windows CUDA CS1 pass index")
    grade = list(csv.DictReader(pinned(pass_dir / "results.csv", results_sha).decode("utf-8").splitlines()))
    grades = {(g["repo"], g["slot"]): g for g in grade if g["configuration"] == "search"}
    selected = [r for r in index["results"] if r["path"] == "search" and r["sample"] == 1]
    roster(selected)
    common.require(set(grades) == EXPECTED and len([g for g in grade if g["configuration"] == "search"]) == len(EXPECTED),
                   "the CS1 Search grade roster differs")
    freeze_path = under(Path(index["freeze"]["path"]), pass_dir.parent.parent)
    frozen = json.loads(pinned(freeze_path, index["freeze"]["sha256"]))["files"]
    rows = []
    for row in selected:
        repo, slot = row["repo"], row["slot"]
        case_path = under(freeze_path.parent / "scenarios" / repo / f"{slot}.json", freeze_path.parent)
        scenario = common.load_scenario(case_path)
        common.require(scenario["id"] == row["scenario_id"]
                       and scenario["_sha256"] == frozen[case_path.relative_to(freeze_path.parent).as_posix()],
                       f"{repo}/{slot}: scenario freeze differs")
        directory = under(Path(row["session_dir"]), pass_dir.parent.parent / "sessions")
        session_path = under(directory / "session.json", directory)
        session_bytes = session_path.read_bytes()
        session = json.loads(session_bytes)
        judged = grades[(repo, slot)]
        common.require(judged["series"] == SERIES
                       and all(judged[k].isdigit() for k in ("judge1_grade", "judge2_grade", "adjudicated_grade"))
                       and session.get("scenario_id") == scenario["id"]
                       and session.get("scenario_sha256") == scenario["_sha256"]
                       and session.get("repo") == repo and session.get("slot") == slot
                       and session.get("path") == "search" and session.get("sample") == 1
                       and session.get("session_id") == judged["session_id"] and session.get("status") == "complete",
                       f"{repo}/{slot}: judged session identity differs")
        log_path = under(directory / "wrapper" / "turns.jsonl", directory)
        log_bytes = log_path.read_bytes()
        turns = [json.loads(line) for line in log_bytes.splitlines() if line.strip()]
        first = [t for t in turns if t.get("turn") == 1 and t.get("rejected") is False]
        common.require(len(first) == 1 and first[0].get("tool") == "airk_search"
                       and first[0].get("ok") is True and (first[0].get("first_call") or {}).get("rule") == "canonical",
                       f"{repo}/{slot}: successful first canonical Search absent")
        turn = first[0]
        args = turn["sent_arguments"]
        common.require(isinstance(args, dict) and args.get("query") == scenario["instruction"]
                       and turn["env_stripped"] == list(common.AGENT_STRIPPED_ENV),
                       f"{repo}/{slot}: exact first arguments or response environment differs")
        result = under(Path(turn["result_path"]), directory / "wrapper")
        retained = common.raw_body(pinned(result, turn["result_sha256"]))
        common.require(isinstance(retained, dict) and isinstance(retained.get("hits"), list),
                       f"{repo}/{slot}: retained first Search evidence missing")
        notes = [n for n in scenario["notes"] if n["mark"] == "essential"]
        common.require(len({n["id"] for n in notes}) == len(notes), f"{repo}/{slot}: duplicate essential ID")
        rows.append({"repo": repo, "slot": slot, "key": row["key"], "arguments": args,
                     "request_sha256": digest([repo, args, turn["env_stripped"]]),
                     "limit": args.get("limit", DEFAULT_LIMIT), "max_bytes": args.get("max_bytes", DEFAULT_MAX_BYTES),
                     "response_mode": turn["response_mode"], "essential": notes,
                     "source_sha256": {"scenario": scenario["_sha256"], "session": common.sha256_bytes(session_bytes),
                              "turn_log": common.sha256_bytes(log_bytes), "first_reply": turn["result_sha256"]},
                     "original_publication": (retained.get("publication") or {}).get("token")})
    return {"schema": SCHEMA, "kind": "cohort", "series": SERIES,
            "input_sha256": {"pass_index": index_sha, "judged_results": results_sha, "scenario_freeze": index["freeze"]["sha256"]},
            "rows": rows}


def stage(handle, profile: str | None, timeout: float) -> dict:
    proc = subprocess.run([*handle.cli, "--output", "json", "--service-runtime-dir", handle.control,
                           "arbiter", "status-legacy"], capture_output=True, stdin=subprocess.DEVNULL,
                          env=common.tool_env(handle), timeout=timeout,
                          creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    common.require(proc.returncode == 0, "native stage status failed")
    body = json.loads(proc.stdout)
    config = body["configuration"]
    for name, wanted in (("focus", profile), ("context", None)):
        actual, observed = config[name], body[name]
        common.require(actual["enabled"] is (wanted is not None)
                       and observed.get("queued") == 0 and observed.get("running") == 0
                       and observed.get("active_profile") == wanted
                       and (wanted is None or actual.get("selected_profile") == wanted),
                       f"unexpected live {name} configuration or native work")
    return {"revision": config["revision"], "focus": profile, "context": None,
            "status_sha256": common.sha256_bytes(proc.stdout)}


def hit_identity(hit: dict) -> str:
    return digest({"citation": hit["citation"], "text": hit["text"], "truncated": hit["truncated"]})


def focus_receipt(observation, profile, publication, index_id, revision, hits) -> dict:
    if observation is None:
        return {"state": "absent"}
    common.require(profile is not None, "off Search returned Focus activity")
    if observation["kind"] == "not_applied":
        common.require(observation.get("work_unknown") is False, "non-applied Focus has unknown native work")
        return {"state": "not_applied", "availability": observation["availability"],
                "candidate_count": observation["candidate_count"], "reason": observation["reason"], "observation_sha256": digest(observation)}
    common.require(observation["kind"] == "receipt", "unknown Search Focus observation")
    receipt = observation["receipt"]
    scope = receipt["scope"]
    original, ranked, scores = receipt["original_order"], receipt["ranked_order"], receipt["scores"]
    n = receipt["scored_count"]
    common.require(receipt["schema"] == "airk.arbiter.focus.ranking/1"
                   and scope["publication"]["token"] == publication and scope["index_id"] == index_id
                   and scope["configuration_revision"] == revision and 0 <= n == len(scores) <= len(original)
                   and len(original) == receipt["candidate_count"] == len(hits) == len(set(original))
                   and len(ranked) == len(original) and set(ranked) == set(original)
                   and [s["candidate"] for s in scores] == original[:n],
                   "Focus receipt lacks matched scope, complete candidates or scored prefix")
    result = {"state": "not_applied", "availability": receipt["availability"], "candidate_count": len(original),
              "scored_count": n, "reason": receipt.get("reason"), "observation_sha256": digest(observation)}
    if not receipt["applied"]:
        common.require(ranked == original, "unapplied Focus changed candidate order")
        return result
    identity, work = receipt["identity"], receipt["work"]
    expected_prefix = [s["candidate"] for s in sorted(scores, key=lambda s: -s["relevance"])]
    common.require(identity["profile"] == profile and identity["runtime"].startswith(CUDA)
                   and SHA.fullmatch(identity["runtime"][len(CUDA):]) is not None
                   and receipt["availability"] == "ready" and n > 0 and n == receipt["presented_count"]
                   and isinstance(work.get("inference_us"), int) and ranked == expected_prefix + original[n:],
                   "applied Focus lacks expected CUDA identity, stable ranking or native tail preservation")
    mapping = dict(zip(ranked, hits, strict=True))
    return {**result, "state": "cuda_applied", "identity": identity,
            "input_digest": receipt["input_digest"], "work": work, "unscored_tail_count": len(original) - n,
            "tail_preserved": True, "original_hit_sha256s": [mapping[item] for item in original],
            "scored_hits": [{"hit_sha256": mapping[score["candidate"]], "relevance": score["relevance"]}
                            for score in scores]}


def measure(request: dict, outcome, profile, publication, index_id, revision) -> dict:
    row = {k: request[k] for k in ("repo", "slot", "key", "request_sha256", "limit", "max_bytes", "response_mode")}
    row.update(ok=outcome.ok, elapsed_ms=outcome.elapsed_ms, stdout_bytes=len(outcome.stdout),
               stdout_sha256=common.sha256_bytes(outcome.stdout), stderr_sha256=common.sha256_bytes(outcome.stderr))
    if not outcome.ok:
        return {**row, "error_code": depth.error_code(outcome)}
    body = common.envelope_body(outcome.envelope)
    common.require(isinstance(body, dict) and isinstance(body.get("hits"), list)
                   and (body.get("publication") or {}).get("token") == publication
                   and body["execution"]["served_mode"] == "hybrid",
                   "Search did not return the pinned hybrid publication")
    hits = [hit_identity(hit) for hit in body["hits"]]
    common.require(len(hits) <= request["limit"], "Search returned more than its original limit")
    spans = [evidence.extract(hit) for hit in body["hits"]]
    receipt = focus_receipt(body.get("focus"), profile, publication, index_id, revision, hits)
    return {**row, "incomplete": body["execution"]["incomplete"], "truncated": body["truncated"],
            "hits_returned": len(hits), "hit_sha256s": hits, "focus": receipt,
            "served_mode": body["execution"]["served_mode"],
            "hit_citations": [hit["citation"] for hit in body["hits"]],
            "notes": [{"id": note["id"], **depth.ranks(spans, note)} for note in request["essential"]]}


def live_identity(handle, raw, repos) -> dict:
    image = raw["image"]
    profile = raw.get("search") or "none"
    dispatched_profile = raw.get("service_environment", {}).get("AIRK_ARBITER_EVALUATION_SEARCH", "none")
    common.require(profile == dispatched_profile, "Search profile differs from the dispatched native environment")
    identity = {"lifetime": raw["lifetime"], "instance": raw["instance"], "control": handle.control,
                "search_profile": profile,
                "git_head": image["git_head"], "service_sha256": image["service"]["sha256"],
                "cli_sha256": image["cli"]["sha256"], "handle_sha256": handle.sha256,
                "indexes": {}}
    for repo in repos:
        physical = (handle.aliases or {}).get(repo, repo)
        record = raw["indexes"].get(physical)
        common.require(isinstance(record, dict) and record["index_id"] == handle.index_id(repo)
                       and record["binding"] == handle.binding(repo), "Search index alias or binding differs")
        identity["indexes"][repo] = {"physical": physical, **{key: record[key] for key in
                                      ("index_id", "binding", "source_commit", "pipeline_digest", "publication_token")}}
    return identity


def collect(args) -> dict:
    common.require(math.isfinite(args.timeout) and args.timeout > 0, "invalid Search timeout")
    cohort = json.loads(pinned(args.cohort, args.cohort_sha256))
    common.require(cohort["schema"] == SCHEMA and cohort["kind"] == "cohort"
                   and cohort["series"] == SERIES and roster(cohort["rows"]), "invalid Search cohort")
    profile = args.focus_profile
    common.require((args.arm == "focus") == bool(profile) and (profile is None or SHA.fullmatch(profile) is not None),
                   "Focus arm requires the exact CUDA manifest SHA-256")
    handle = common.load_handle(args.handle)
    raw = json.loads(pinned(args.handle, args.handle_sha256))
    common.require(handle.sha256 == args.handle_sha256 and handle.state == "ready", "live fixture identity differs")
    repos = sorted({row["repo"] for row in cohort["rows"]})
    identity = live_identity(handle, raw, repos)
    tokens = {repo: identity["indexes"][repo]["publication_token"] for repo in repos}
    pre = stage(handle, profile, args.timeout)
    start = {repo: settled.snapshot(handle, repo, args.timeout) for repo in repos}
    common.require(all(start[repo]["settled"] and start[repo]["publication_token"] == tokens[repo]
                       for repo in repos), "Search starting publication or readiness differs")
    rows = []
    for request in cohort["rows"]:
        repo = request["repo"]
        common.require(request["request_sha256"] == digest([repo, request["arguments"], list(common.AGENT_STRIPPED_ENV)])
                       and request["response_mode"] == common.effective_response_mode(
                           handle, request["arguments"], common.AGENT_STRIPPED_ENV), "Search request or response mode differs")
        result = common.call_tool(handle, "airk_search", repo, request["arguments"], args.timeout,
                                  strip_env=common.AGENT_STRIPPED_ENV)
        rows.append(measure(request, result, profile, tokens[repo], handle.index_id(repo), pre["revision"]))
    end = {repo: settled.snapshot(handle, repo, args.timeout) for repo in repos}
    post = stage(handle, profile, args.timeout)
    common.require(pre["revision"] == post["revision"] and common.sha256_file(handle.path) == handle.sha256
                   and all(end[repo]["settled"] and settled.compare(start[repo], end[repo])["unchanged"]
                           for repo in repos), "Search stage, fixture handle or settled publication changed")
    return {"schema": SCHEMA, "kind": args.arm, "cohort_sha256": args.cohort_sha256, "identity": identity,
            "stage_before": pre, "stage_after": post, "rows": rows}


def compare(off: dict, focus: dict) -> dict:
    common.require(off["schema"] == focus["schema"] == SCHEMA and off["kind"] == "off" and focus["kind"] == "focus"
                   and off["cohort_sha256"] == focus["cohort_sha256"] and off["identity"] == focus["identity"]
                   and roster(off["rows"]) == roster(focus["rows"]), "arms differ in cohort, fixture, publication or order")
    rows = []
    for left, right in zip(off["rows"], focus["rows"], strict=True):
        common.require(all(left[k] == right[k] for k in ("key", "request_sha256", "limit", "max_bytes", "response_mode")),
                       "arm request identity differs")
        row = {k: left[k] for k in ("repo", "slot", "limit", "max_bytes", "response_mode", "request_sha256")}
        row["off_ms"], row["focus_ms"] = left["elapsed_ms"], right["elapsed_ms"]
        row["matched"] = left["ok"] and right["ok"] and not (left["incomplete"] or right["incomplete"])
        if row["matched"]:
            receipt = right["focus"]
            row.update(off_hits=left["hits_returned"], focus_hits=right["hits_returned"],
                       off_truncated=left["truncated"], focus_truncated=right["truncated"],
                       focus_state=receipt["state"], unscored_tail_count=receipt.get("unscored_tail_count"),
                       tail_preserved=receipt.get("tail_preserved"),
                       same_hit_multiset=sorted(left["hit_sha256s"]) == sorted(right["hit_sha256s"]),
                       unranked_order_matches_off=None if "original_hit_sha256s" not in receipt
                       else receipt["original_hit_sha256s"] == left["hit_sha256s"])
            common.require(left["focus"]["state"] == "absent" and [x["id"] for x in left["notes"]] ==
                           [x["id"] for x in right["notes"]], "off Focus or compared note identity differs")
            row["notes"] = [{"id": a["id"], "off_rank": a["rule2"], "focus_rank": b["rule2"]}
                            for a, b in zip(left["notes"], right["notes"], strict=True)]
        rows.append(row)
    matched = [row for row in rows if row["matched"]]
    eligible = [row for row in matched if row["off_hits"] or row["focus_hits"]]
    applied = [row for row in eligible if row["focus_state"] == "cuda_applied"]
    notes = [note for row in matched for note in row["notes"]]
    shared = [n for n in notes if n["off_rank"] is not None and n["focus_rank"] is not None]
    focus_profile = (focus.get("stage_before") or {}).get("focus")
    return {"schema": SCHEMA, "kind": "comparison", "cohort_sha256": off["cohort_sha256"], "identity": off["identity"],
            **({"focus_profile": focus_profile} if focus_profile else {}),
            "complete": len(matched) == len(rows) and len(applied) == len(eligible) and bool(applied),
            "counts": {"planned": len(rows), "matched": len(matched), "focus_eligible": len(eligible),
                       "cuda_applied": len(applied), "applied_with_nonempty_tail": sum(r["unscored_tail_count"] > 0 for r in applied),
                       "applied_original_order_matches_off": sum(r["unranked_order_matches_off"] for r in applied),
                       "essential_matched_pool": len(notes), "essential_off_covered": sum(n["off_rank"] is not None for n in notes),
                       "essential_focus_covered": sum(n["focus_rank"] is not None for n in notes),
                       "off_only": sum(n["off_rank"] is not None and n["focus_rank"] is None for n in notes),
                       "focus_only": sum(n["off_rank"] is None and n["focus_rank"] is not None for n in notes),
                       "both_ranked_higher_with_focus": sum(n["focus_rank"] < n["off_rank"] for n in shared),
                       "both_ranked_lower_with_focus": sum(n["focus_rank"] > n["off_rank"] for n in shared),
                       "both_ranked_same": sum(n["focus_rank"] == n["off_rank"] for n in shared)},
            "median_ms": ({"off": median(r["off_ms"] for r in matched), "focus": median(r["focus_ms"] for r in matched),
                           "paired_focus_minus_off": median(r["focus_ms"] - r["off_ms"] for r in matched)} if matched else None),
            "scope": "Fixed first Search requests only. Rule 2 matches returned citations to frozen essentials. "
                     "No judgments of follow-up calls, session finishes, or candidates beyond the returned pool. "
                     "Latency is elapsed CLI time after live guard probes, so run order and cache can affect it.",
            "rows": rows}


def valid_rank(value, limit: int) -> bool:
    return value is None or (type(value) is int and 1 <= value <= limit)


def report_rows(report: dict) -> list[dict]:
    rows = report["rows"]
    roster(rows)
    common.require(report["schema"] == SCHEMA and report["kind"] == "comparison"
                   and report["complete"] is True and all(r["matched"] is True for r in rows)
                   and isinstance(report.get("arm_sha256"), dict)
                   and set(report["arm_sha256"]) == {"off", "focus"}
                   and all(SHA.fullmatch(pin) for pin in report["arm_sha256"].values()),
                   "expected an authenticated complete Focus off/on comparison")
    for row in rows:
        limit = row["limit"]
        ids = [note["id"] for note in row["notes"]]
        common.require(type(limit) is int and limit > 0 and ids and len(ids) == len(set(ids))
                       and all(type(row[f"{arm}_ms"]) is int and row[f"{arm}_ms"] >= 0 for arm in ("off", "focus"))
                       and all(valid_rank(note[f"{arm}_rank"], limit)
                               for note in row["notes"] for arm in ("off", "focus")),
                       "invalid matched rank, essential roster or elapsed time")
    return rows


def arm_metrics(ranks: list[int | None], times: list[int]) -> dict:
    return {"covered": sum(rank is not None for rank in ranks),
            "top3": sum(rank is not None and rank <= 3 for rank in ranks),
            "mean_inverse_rank": sum(1 / rank for rank in ranks if rank is not None) / len(ranks),
            "median_ms": median(times), "p95_ms": sorted(times)[math.ceil(0.95 * len(times)) - 1]}


def matched_effects(reference: list[dict], current: list[dict]) -> dict:
    groups = {"all": (reference, current)}
    groups.update({repo: ([r for r in reference if r["repo"] == repo],
                          [r for r in current if r["repo"] == repo]) for repo in common.REPOS})
    effects = {}
    for scope, (before, after) in groups.items():
        arms = {}
        for arm in ("off", "focus"):
            ranks = [[note[f"{arm}_rank"] for row in rows for note in row["notes"]] for rows in (before, after)]
            times = [[row[f"{arm}_ms"] for row in rows] for rows in (before, after)]
            old, new = [arm_metrics(r, t) for r, t in zip(ranks, times, strict=True)]
            pairs = list(zip(*ranks, strict=True))
            shared = [(a, b) for a, b in pairs if a is not None and b is not None]
            arms[arm] = {"reference": old, "current": new,
                         "gained": sum(a is None and b is not None for a, b in pairs),
                         "lost": sum(a is not None and b is None for a, b in pairs),
                         "rank_up": sum(a > b for a, b in shared),
                         "rank_down": sum(a < b for a, b in shared),
                         "rank_same": sum(a == b for a, b in shared),
                         "delta": {key: new[key] - old[key] for key in old},
                         "median_paired_ms_delta": median(b - a for a, b in zip(*times, strict=True))}
        focus_delta = {key: arms["focus"]["delta"][key] - arms["off"]["delta"][key]
                       for key in ("covered", "top3", "mean_inverse_rank")}
        focus_delta["median_paired_ms_delta"] = median(
            (b["focus_ms"] - b["off_ms"]) - (a["focus_ms"] - a["off_ms"])
            for a, b in zip(before, after, strict=True))
        effects[scope] = {"requests": len(after), "essentials": sum(len(r["notes"]) for r in after),
                          **arms, "focus_interaction": focus_delta}
    return effects


def changed_notes(reference: list[dict], current: list[dict]) -> list[dict]:
    changes = []
    for before, after in zip(reference, current, strict=True):
        for a, b in zip(before["notes"], after["notes"], strict=True):
            if a != b:
                changes.append({"repo": after["repo"], "slot": after["slot"], "note": b["id"],
                                "reference": {k: a[k] for k in ("off_rank", "focus_rank")},
                                "current": {k: b[k] for k in ("off_rank", "focus_rank")}})
    return changes


def cumulative(step: int, reports: dict, pins: dict, experimental_index: bool, original_focus=None) -> dict:
    common.require(1 <= step <= 6 and (not experimental_index or step == 6), "experimental indexes are allowed only at step 6")
    common.require((step == 1) == (pins["prior"] == pins["original"]) and pins["current"] not in (pins["prior"], pins["original"]),
                   "step 1 reuses the original as its prior; later steps require a distinct prior and current")
    rows = {name: report_rows(report) for name, report in reports.items()}
    original = reports["original"]
    profile = original.get("focus_profile")
    if profile is None:
        common.require(isinstance(original_focus, dict) and original_focus["schema"] == SCHEMA
                       and original_focus["kind"] == "focus" and original_focus["identity"] == original["identity"]
                       and original_focus["cohort_sha256"] == original["cohort_sha256"],
                       "the original comparison requires its authenticated native Focus arm")
        profile = original_focus["stage_before"]["focus"]
        common.require(original_focus["stage_after"]["focus"] == profile, "original native Focus changed during collection")
    common.require(isinstance(profile, str) and SHA.fullmatch(profile) is not None, "invalid native Focus profile")
    source = original["identity"]["indexes"]
    common.require(set(source) == set(common.REPOS), "original index roster differs")
    match_keys = ("repo", "slot", "request_sha256", "limit", "max_bytes", "response_mode")
    frozen = [(tuple(row[k] for k in match_keys), [note["id"] for note in row["notes"]]) for row in rows["original"]]
    index_changes = {}
    for name, report in reports.items():
        focus = report.get("focus_profile", profile if report is original or pins[name] == pins["original"] else None)
        common.require(focus == profile and report["cohort_sha256"] == original["cohort_sha256"],
                       f"{name}: frozen Search cohort or native Focus profile differs")
        observed = [(tuple(row[k] for k in match_keys), [note["id"] for note in row["notes"]]) for row in rows[name]]
        common.require(observed == frozen, f"{name}: exact Search request settings, order or essential roster differs")
        indexes = report["identity"]["indexes"]
        common.require(set(indexes) == set(source) and all(indexes[repo]["source_commit"] == source[repo]["source_commit"]
                                                           for repo in source), f"{name}: repository source revisions differ")
        differences = {repo: [key for key in sorted(set(source[repo]) | set(indexes[repo]))
                              if source[repo].get(key) != indexes[repo].get(key)] for repo in source}
        differences = {repo: keys for repo, keys in differences.items() if keys}
        common.require(not differences or (experimental_index and name == "current"),
                       f"{name}: index binding, pipeline or publication differs from original")
        index_changes[name] = differences
    if experimental_index:
        common.require(any("publication_token" in keys for keys in index_changes["current"].values()),
                       "step 6 experimental comparison requires a different publication")
    return {"schema": SCHEMA, "kind": "cumulative", "step": step, "cohort_sha256": original["cohort_sha256"],
            "focus_profile": profile, "experimental_index": experimental_index, "index_changes": index_changes,
            "input_sha256": pins, "run_identities": {name: r["identity"] for name, r in reports.items()},
            "comparisons": {name: {"metrics": matched_effects(rows[name], rows["current"]),
                                   "changed_notes": changed_notes(rows[name], rows["current"])}
                            for name in ("original", "prior")},
            "scope": "Matched first Search replies. Focus interaction is the Focus-on change minus the Focus-off change. "
                     "Mean inverse rank counts a missing essential as zero. Paired time deltas include CLI and depend on "
                     "cache state and fixed arm order. Experimental index results also include changing publications."}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    freeze_cmd, collect_cmd, compare_cmd, cumulative_cmd = (
        commands.add_parser(name) for name in ("freeze", "collect", "compare", "cumulative"))
    for cmd in (freeze_cmd, collect_cmd, compare_cmd, cumulative_cmd):
        cmd.add_argument("--output", type=Path, required=True)
    freeze_cmd.add_argument("--pass-dir", type=Path, required=True)
    for flag in ("--pass-index-sha256", "--results-sha256"):
        freeze_cmd.add_argument(flag, required=True)
    cumulative_cmd.add_argument("--step", type=int, required=True)
    cumulative_cmd.add_argument("--experimental-index", action="store_true")
    cumulative_cmd.add_argument("--original-focus-arm", type=Path)
    for cmd, name in ((collect_cmd, "cohort"), (collect_cmd, "handle"), (compare_cmd, "off"), (compare_cmd, "focus"),
                      (cumulative_cmd, "original"), (cumulative_cmd, "prior"), (cumulative_cmd, "current")):
        cmd.add_argument(f"--{name}", type=Path, required=True)
        cmd.add_argument(f"--{name}-sha256", required=True)
    collect_cmd.add_argument("--arm", choices=("off", "focus"), required=True)
    collect_cmd.add_argument("--focus-profile")
    collect_cmd.add_argument("--timeout", type=float, default=30)
    args = parser.parse_args(argv)
    common.require(args.output.parent.is_dir() and not args.output.exists(), "use a fresh output in an existing private directory")
    if args.command == "freeze":
        value = freeze(args.pass_dir, args.pass_index_sha256, args.results_sha256)
    elif args.command == "collect":
        value = collect(args)
    elif args.command == "compare":
        value = compare(json.loads(pinned(args.off, args.off_sha256)), json.loads(pinned(args.focus, args.focus_sha256)))
        value["arm_sha256"] = {"off": args.off_sha256, "focus": args.focus_sha256}
    else:
        pins = {name: getattr(args, f"{name}_sha256") for name in ("original", "prior", "current")}
        reports = {name: json.loads(pinned(getattr(args, name), pin)) for name, pin in pins.items()}
        native = None
        if args.original_focus_arm is not None:
            pins["original_focus_arm"] = reports["original"]["arm_sha256"]["focus"]
            native = json.loads(pinned(args.original_focus_arm, pins["original_focus_arm"]))
        value = cumulative(args.step, reports, pins, args.experimental_index, native)
    write_new(args.output, value)
    return 2 if args.command == "compare" and not value["complete"] else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except common.HarnessError as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
