# SPDX-License-Identifier: Apache-2.0
"""Audit and summarize the frozen six-task E2E panel, without running agents or grading answers."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
from statistics import mean

import agent_cost_batch
import agent_cost_run
import common
import session_verify
import tool_names


TASKS = ("munarium-E2E-B1", "munarium-E2E-N1", "munarium-E2E-N2",
         "bookbuilder-E2E-B1", "bookbuilder-E2E-N1", "bookbuilder-E2E-N2")
ARMS = {"plain": "plain", "native": "tool", "candidate": "live"}
MODEL = "claude-sonnet-5-5"
SKILL = "43f1b02dac291d5389205deb19756d28c3d7963b9a44669dc331ff95320dbfe0"
GATHER = "mcp__airk__gather_context"
METRICS = ("cost_usd", "cost_one_price", "round_trips", "reported_turns", "actor_wall_seconds",
           "api_seconds", "verifier_seconds", "gather_calls", "verifier_calls")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def checked(path, expected):
    require(digest(path) == expected.lower(), f"Digest mismatch: {path}")
    return common.read_json(path)


def unique(values, field):
    values = set(values)
    require(len(values) == 1, f"Unmatched {field}: {sorted(map(str, values))}")
    return values.pop()


def flag(command, key):
    require(command.count(key) == 1, f"Missing or duplicate {key}")
    return command[command.index(key) + 1]


def session_metrics(design, series, task, arm):
    tid = task["id"]
    base = series / "runs" / f"{tid}.{arm}"
    receipt = common.read_json(base / "session.json")
    trace = common.read_jsonl(base / "stream.jsonl")
    actual = agent_cost_run.summarize(trace)
    init = next(e for e in trace if e.get("subtype") == "init" and e.get("type") == "system")
    require(receipt["completed"] and not receipt["timed_out"] and receipt["exit_code"] == 0
            and receipt["arm"] == ARMS[arm], f"Unfinished or wrong arm: {base}")
    require(all(receipt[k] == actual[k] for k in ("completed", "round_trips", "reported_turns", "model_ms",
            "elapsed_ms", "cost_usd", "tokens", "tool_uses", "tools_offered", "permission_denials")),
            f"Transcript and receipt differ: {base}")
    require(receipt["model"] == init["model"] == MODEL, f"Unexpected model: {base}")
    solver_path = design / "solver" / f"{tid}.json"
    solver = checked(solver_path, task["solver_sha256"])
    require((base / "answer.md").read_text(encoding="utf-8") == actual["final_message"], f"Final answer differs: {base}")
    registered = receipt["task"]
    require(Path(registered["path"]).resolve() == solver_path.resolve()
            and registered["sha256"] == task["solver_sha256"].lower() and registered["repository"] == solver["repository"]
            and solver["source_commit"] == task["source_commit"] and solver["id"] == tid,
            f"Solver input mismatch: {base}")
    profile = receipt["verification"]
    checked(base / "verification-profile.json", profile["profile_sha256"])
    require(profile["surface_valid"] and not profile["incomplete_calls"], f"Invalid verification surface: {base}")
    runs = sorted((base / "verification").glob("*/result.json"), key=lambda p: int(p.parent.name))
    require([common.read_json(p) for p in runs] == profile["runs"], f"Verifier result mismatch: {base}")
    for path, run in zip(runs, profile["runs"]):
        require(digest(path.parent / "output.log") == run["sha256"], f"Verifier output mismatch: {path}")
    config = common.read_json(base / "mcp-config.json")["mcpServers"]
    require(set(config) == ({"verification"} if arm == "plain" else {"verification", "airk"}), f"MCP config: {base}")
    expected = set(agent_cost_run.FILE_TOOLS) | {session_verify.NAME}
    surface_sha = None
    scope = None
    if arm == "plain":
        require("skill" not in receipt and "native_mcp" not in receipt, f"Plain arm contamination: {base}")
    else:
        expected |= {"Skill", *(f"mcp__airk__{name}" for name in tool_names.mcp_tools())}
        require(receipt["product_surface_valid"] and receipt["skill"] == {"mode": "natural", "sha256": SKILL}
                and digest(base / "airkctl" / "skills" / "airk-tools" / "SKILL.md") == SKILL,
                f"Wrong skill: {base}")
        surface_sha = receipt["native_mcp"]["surface_sha256"]
        surface = checked(base / "native-surface.json", surface_sha)
        require(sorted(t["name"] for t in surface["tools"]) == sorted(tool_names.mcp_tools()),
                f"Wrong native catalog: {base}")
        prepared = receipt["prepared_workspace"]
        require(prepared["mcp_catalog_valid"] and Path(prepared["work"]).resolve() == (base / "work").resolve(),
                f"Unexpected prepared workspace: {base}")
        native = receipt["native_mcp"]
        scope = {"index": native["index"], "handle_sha256": native["handle_sha256"]}
        if task["kind"] == "research":
            import research_frozen_index
            proof = receipt["research_frozen_index"]
            before = proof["before"]
            if proof["valid"]:
                after = proof["after"]
                scope["postcheck"] = "at_session_close"
            else:
                path = base / "research-post-observation.json"
                later = common.read_json(path)
                require(proof.get("after") is None and proof.get("error") == "Native workspace path inventory failed"
                        and later["original_session_sha256"].lower() == digest(base / "session.json")
                        and later["original_valid"] is False and later["original_error"] == proof["error"]
                        and later["task"] == tid and later["before"] == before and later["matches_original_before"]
                        and later["proof_sha256_by_file"] == {name: digest(base / name) for name in
                                                             ("session.json", "stream.jsonl", "answer.md", "changes.patch")},
                        f"Unresolved original research postcheck: {base}")
                after = later["later"]
                scope.update(postcheck="later_observation", original_error=proof["error"],
                             sidecar_sha256=digest(path), observed_from=later["observed_from"], observed_to=later["observed_to"])
            require(proof["task"] == tid and proof["design_sha256"] == digest(design / "freeze.json")
                    and before["index"] == native["index"] == task["index_id"]
                    and before["handle_sha256"] == native["handle_sha256"]
                    and Path(before["work"]).resolve() == (base / "work").resolve()
                    and research_frozen_index.finish(before, after, actual["tool_uses"], receipt["changed_files"], actual["final_message"]),
                    f"Frozen research binding changed: {base}")
            scope.update({k: before[k] for k in ("publication", "scope_fingerprint", "eligible_digest", "tree_digest")})
        else:
            handle = checked(Path(native["fixture"]), native["handle_sha256"])
            qualification = handle["qualification"]
            parent = checked(Path(qualification["parent_handle"]), qualification["parent_sha256"])
            repo = task["repository"]
            source = parent["indexes"][repo]
            registration = handle["indexes"][repo]
            require(source["index_id"] == task["index_id"] and registration["index_id"] == native["index"]
                    and Path(registration["root"]).resolve() == (base / "work").resolve()
                    and source["source_commit"] == registration["source_commit"] == task["source_commit"]
                    and registration["served_mode"] in ("hybrid", "vector")
                    and registration["publication_token"] == qualification["inventory"]["publication"],
                    f"Engineering index provenance differs: {base}")
            scope.update(eligible_digest=qualification["eligible_digest"], publication=registration["publication_token"],
                         parent_handle_sha256=qualification["parent_sha256"], served_mode=registration["served_mode"],
                         pipeline_digest=registration["pipeline_digest"])
        cli = config["airk"]["args"]
        if arm == "candidate":
            require(flag(cli, "--live-layout") == "qwen-focus" and "--workspace-map" in cli
                    and Path(flag(cli, "--live-source")).resolve() == (base / "work").resolve(),
                    f"Wrong candidate treatment: {base}")
        else:
            require("--live-source" not in cli and "--oracle-task" not in cli, f"Wrong native treatment: {base}")
    require(set(actual["tools_offered"]) == expected, f"Unexpected client-visible tools: {base}")
    uses = actual["tool_uses"]
    gather_calls = uses.get(GATHER, 0)
    gather = receipt["gather_context"]
    verifier_runs = profile["runs"]
    log = base / "gather" / "calls.jsonl"
    logged = common.read_jsonl(log) if log.is_file() else []
    require(len(logged) == gather["calls"] and sum(bool(call["is_error"]) for call in logged) == gather["errors"]
            and round(sum(call["seconds"] for call in logged), 3) == gather["seconds"], f"Gather receipt mismatch: {base}")
    grade_key = digest(base / "answer.md")
    return {
        "task": tid, "arm": arm, "stratum": task["stratum"], "kind": task["kind"], "repository": task["repository"],
        "session_sha256": digest(base / "session.json"), "answer_sha256": grade_key, "patch_sha256": digest(base / "changes.patch"),
        "profile_sha256": profile["profile_sha256"], "surface_sha256": surface_sha, "skill_sha256": receipt.get("skill", {}).get("sha256"),
        "scope": scope, "solver_sha256": registered["sha256"], "system_prompt": flag(receipt["command"], "--system-prompt"),
        "max_turns": int(flag(receipt["command"], "--max-turns")), "model": receipt["model"], "client": receipt["claude"],
        "cost_usd": receipt["cost_usd"], "usage": agent_cost_batch.final_usage(base), "tokens": actual["tokens"],
        "round_trips": actual["round_trips"], "reported_turns": actual["reported_turns"],
        "actor_wall_seconds": receipt["wall_seconds"], "api_seconds": receipt["model_ms"] / 1000,
        "client_elapsed_seconds": receipt["elapsed_ms"] / 1000, "gather_calls": gather_calls,
        "gather_logged_calls": gather["calls"], "gather_logged_seconds": gather["seconds"] if gather["calls"] == gather_calls else None,
        "gather_logged_errors": gather["errors"] if gather["calls"] == gather_calls else None,
        "verifier_calls": uses.get(session_verify.NAME, 0), "verifier_executions": len(verifier_runs),
        "verifier_seconds": sum(r["seconds"] for r in verifier_runs),
        "verifier_exits": [r["exit_code"] for r in verifier_runs], "skill_calls": uses.get("Skill", 0),
        "airk_calls": sum(n for k, n in uses.items() if k.startswith("mcp__airk__")),
        "tool_calls": uses, "permission_denials": actual["permission_denials"],
    }


def attach_grade(row, series, task):
    path = series / "grading" / (row["answer_sha256"][:12] + ".json")
    grade = common.read_json(path)
    require(grade["answer_sha256"] == row["answer_sha256"] and grade["task"] == row["task"]
            and grade["rubric_sha256"] == task["grader_sha256"].lower(), f"Wrong grade: {path}")
    if task["kind"] == "coding":
        require(grade["patch_sha256"] == row["patch_sha256"], f"Wrong graded patch: {path}")
    require(type(grade["final_pass"]) is bool and (type(grade["first_pass"]) is bool or grade["first_pass"] is None),
            f"Ambiguous pass flag: {path}")
    require(len(grade["marks"]) == 4 and all(type(v) is int and v in (0, 1) for v in grade["marks"]),
            f"Unexpected rubric marks: {path}")
    row.update(grade_sha256=digest(path), final_pass=grade["final_pass"], first_pass=grade["first_pass"],
               first_pass_basis=grade["first_pass_basis"], marks=grade["marks"])


def summarize_rows(rows, sensitivity=False):
    """Frozen design weights the stratum means, independent of task count."""
    strata = {k: [r for r in rows if r["stratum"] == k] for k in ("broad", "narrow")}
    if sensitivity:
        require(all(strata.values()), "Cannot estimate a missing stratum")
    else:
        require([len(strata[k]) for k in ("broad", "narrow")] == [2, 4], "Incomplete task strata")
    fields = [m for m in METRICS if all(m in r for r in rows)]
    if "final_pass" in rows[0]:
        fields.append("final_pass")
        if all(r["first_pass"] is not None for r in rows):
            fields.append("first_pass")
    means = {k: {m: mean(r[m] for r in group) for m in fields} for k, group in strata.items()}
    return {
        "means": means, "weighted_mean": {m: (means["broad"][m] + 4 * means["narrow"][m]) / 5 for m in fields},
        "total": {m: sum(r[m] for r in rows) for m in fields}, "gather_sessions": sum(r["gather_calls"] > 0 for r in rows),
        "airk_sessions": sum(r["airk_calls"] > 0 for r in rows), "skill_sessions": sum(r["skill_calls"] > 0 for r in rows),
        "verifier_sessions": sum(r["verifier_calls"] > 0 for r in rows),
        "first_pass_unknown": sum(r.get("first_pass") is None for r in rows) if "first_pass" in rows[0] else None,
        "by_kind": {k: {"sessions": len(group), "cost_usd": sum(r["cost_usd"] for r in group),
                        "actor_wall_seconds": sum(r["actor_wall_seconds"] for r in group),
                        "verifier_seconds": sum(r["verifier_seconds"] for r in group)}
                    for k in ("coding", "research") if (group := [r for r in rows if r["kind"] == k])},
        "verifier_exits": dict(Counter(code for r in rows for code in r["verifier_exits"])),
    }


def paired_savings(rows, arm, reference="plain"):
    """Mean of per-task reductions, distinct from the ratio of weighted dollar means."""
    pairs = {(r["task"], r["arm"]): r for r in rows}
    fields = [m for m in ("cost_usd", "cost_one_price", "round_trips", "actor_wall_seconds") if all(m in r for r in rows)]
    by_task = {}
    for tid in (t for t in TASKS if (t, arm) in pairs):
        control, treatment = pairs[tid, reference], pairs[tid, arm]
        by_task[tid] = {f: 100 * (1 - treatment[f] / control[f]) for f in fields}
    strata = {s: [by_task[r["task"]] for r in rows if r["arm"] == arm and r["stratum"] == s]
              for s in ("broad", "narrow")}
    means = {s: {f: mean(r[f] for r in group) for f in fields} for s, group in strata.items()}
    return {"reference": reference, "treatment": arm, "by_task": by_task, "stratum_mean": means,
            "weighted_mean": {f: (means["broad"][f] + 4 * means["narrow"][f]) / 5 for f in fields}}


def paired_differences(rows, arms, reference, treatment, graded):
    by_id = {(r["task"], r["arm"]): r for r in rows}
    pairs = []
    for tid in (t for t in TASKS if (t, treatment) in by_id):
        before, after = by_id[tid, reference], by_id[tid, treatment]
        pair = {"task": tid, "stratum": before["stratum"], "reference_usd": before["cost_usd"],
                "treatment_usd": after["cost_usd"], "dollars_saved": before["cost_usd"] - after["cost_usd"],
                "round_trips_saved": before["round_trips"] - after["round_trips"]}
        if graded:
            pair.update(final_reference=before["final_pass"], final_treatment=after["final_pass"],
                        final_gain=int(after["final_pass"]) - int(before["final_pass"]),
                        rubric_marks_gain=sum(after["marks"]) - sum(before["marks"]),
                        first_pass_gain=None if None in (before["first_pass"], after["first_pass"])
                        else int(after["first_pass"]) - int(before["first_pass"]))
        pairs.append(pair)
    weighted = {a: arms[a]["weighted_mean"] for a in (reference, treatment)}
    result = {"reference": reference, "treatment": treatment, "pairs": pairs,
              "mean_paired_percent_reduction": paired_savings(rows, treatment, reference),
              "reduction_in_weighted_means_percent": {
                  k: 100 * (1 - weighted[treatment][k] / weighted[reference][k])
                  for k in ("cost_usd", "cost_one_price", "round_trips", "actor_wall_seconds") if k in weighted[treatment]}}
    if graded:
        result["paired_final_outcomes"] = {
            "both_pass": sum(p["final_reference"] and p["final_treatment"] for p in pairs),
            "gained": sum(p["final_gain"] == 1 for p in pairs), "lost": sum(p["final_gain"] == -1 for p in pairs),
            "both_fail": sum(not p["final_reference"] and not p["final_treatment"] for p in pairs)}
        result["weighted_final_pass_gain_points"] = 100 * (weighted[treatment]["final_pass"] - weighted[reference]["final_pass"])
    return result


def report(design, series, graded=True):
    manifest = common.read_json(design / "freeze.json")
    tasks = {t["id"]: t for t in manifest["tasks"]}
    require(len(manifest["tasks"]) == len(TASKS) and set(tasks) == set(TASKS), "Wrong frozen task panel")
    expected = {f"{t}.{a}" for t in TASKS for a in ARMS}
    found = {p.parent.name for p in (series / "runs").glob("*/session.json")}
    require(found == expected, f"Session coverage differs: missing={sorted(expected-found)}, extra={sorted(found-expected)}")
    rows = []
    for tid in TASKS:
        task = tasks[tid]
        require(task["stratum"] == ("broad" if tid.endswith("-B1") else "narrow"), f"Wrong stratum: {tid}")
        checked(design / "grading" / f"{tid}.json", task["grader_sha256"])
        for arm in ARMS:
            row = session_metrics(design, series, task, arm)
            if graded:
                attach_grade(row, series, task)
            rows.append(row)
    for field in ("system_prompt", "max_turns", "model", "client"):
        unique([r[field] for r in rows], field)
    unique([r["surface_sha256"] for r in rows if r["arm"] != "plain"], "product catalog and guidance")
    for repo in {t["repository"] for t in tasks.values()}:
        unique([r["profile_sha256"] for r in rows if r["repository"] == repo], f"{repo} verifier profile")
    for row in rows:
        row["system_prompt_sha256"] = hashlib.sha256(row.pop("system_prompt").encode()).hexdigest()
    fit = agent_cost_batch.one_price(rows)
    fit["used_for_comparison"] = fit["fitted"] and fit["largest_fit_error_usd"] < .00001
    if not fit["used_for_comparison"]:
        for row in rows:
            row.pop("cost_one_price", None)
    arms = {a: summarize_rows([r for r in rows if r["arm"] == a]) for a in ARMS}
    later = [r for r in rows if (r["scope"] or {}).get("postcheck") == "later_observation"]
    excluded = sorted({r["task"] for r in later})
    strict = [r for r in rows if r["task"] not in excluded]
    variation = {}
    if excluded:
        strict_arms = {a: summarize_rows([r for r in strict if r["arm"] == a], sensitivity=True) for a in ARMS}
        variation = {"excluded_tasks_from_all_arms": excluded, "arms": strict_arms, "comparisons": {
            f"{t}_vs_{b}": paired_differences(strict, strict_arms, b, t, graded)
            for b, t in (("plain", "native"), ("plain", "candidate"), ("native", "candidate"))}}
    return {
        "original_postcheck_incomplete": [{"task": r["task"], "arm": r["arm"], "scope": r["scope"]} for r in later],
        "strict_matched_sensitivity": variation,
        "schema": "airk.private.e2e-panel-performance/1", "freeze_sha256": digest(design / "freeze.json"),
        "cost_basis": "Actor-reported Claude costs including every turn in each submitted session. Common-cache-price figures are fitted separately.",
        "timing_basis": "Actor wall is subprocess elapsed time per session. API time is model-reported. Local verifier time is included in actor wall. Summed durations across concurrent sessions are not campaign wall time. Setup, indexing and independent grading are outside actor wall and agent spend.",
        "weighting": manifest["weighting"], "cache_price_fit": fit, "arms": arms,
        "comparisons": {f"{treatment}_vs_{reference}": paired_differences(rows, arms, reference, treatment, graded)
                        for reference, treatment in (("plain", "native"), ("plain", "candidate"), ("native", "candidate"))},
        "sessions": rows,
    }


def historical_spend(design, series):
    """Audit incurred actor spend at the historical panel stop, independent of product claims."""
    assert_ids = {f"{t}.{a}" for t in TASKS for a in ARMS if a == "plain" or
                  t not in ("munarium-E2E-B1", "bookbuilder-E2E-N1")}
    found = {p.parent.name for p in (series / "runs").glob("*/session.json")}
    require(found == assert_ids, "Unexpected stopped-panel receipts")
    rows = []
    for task in common.read_json(design / "freeze.json")["tasks"]:
        for arm in ARMS:
            if f"{task['id']}.{arm}" not in found:
                continue
            row = session_metrics(design, series, task, arm)
            attach_grade(row, series, task)
            group = {"plain": "plain", "native": "historical_native"}.get(arm)
            if arm == "candidate":
                if row["airk_calls"]:
                    require(row["tool_calls"].get("mcp__airk__search_passages") == row["airk_calls"],
                            "Unexpected candidate tool exposure")
                    group = "candidate_legacy_backed_ancillary_search"
                else:
                    require(row["tool_calls"] == {"Read": 1}, "Unexpected candidate local-file exposure")
                    group = "candidate_read_only"
            rows.append({k: row[k] for k in ("task", "arm", "session_sha256", "answer_sha256", "grade_sha256",
                                             "cost_usd", "tool_calls", "scope")} | {"spend_group": group})
    groups = {}
    for group in sorted({r["spend_group"] for r in rows}):
        amounts = [r["cost_usd"] for r in rows if r["spend_group"] == group]
        groups[group] = {"sessions": len(amounts), "actor_reported_usd": round(sum(amounts), 7)}
    return {"schema": "airk.private.historical-panel-spend/1", "session_count": len(rows),
            "scope": "Incurred actor spend for the stopped historical experiment. Four coding product sessions were never run. No product comparison or Qwen effectiveness estimate.",
            "actor_reported_usd": round(sum(r["cost_usd"] for r in rows), 7), "groups": groups, "sessions": rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--design", type=Path, required=True)
    parser.add_argument("--series", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--without-grades", action="store_true", help="measure completed sessions before independent grades are ready")
    parser.add_argument("--historical-spend", action="store_true", help="audit only the 14 incurred historical sessions")
    args = parser.parse_args()
    if args.historical_spend:
        data = historical_spend(args.design.resolve(), args.series.resolve())
    else:
        data = report(args.design.resolve(), args.series.resolve(), not args.without_grades)
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump(data, stream, indent=2)
        stream.write("\n")


if __name__ == "__main__":
    main()
