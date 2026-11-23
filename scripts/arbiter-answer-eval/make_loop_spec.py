# SPDX-License-Identifier: Apache-2.0
"""Pass specs and run plans for the loop: the LG gate series (plan 12.7) and the L7 baseline (plan 12.6, 12.8).

Formats are those of ``run_pass.py`` (pass spec) and ``run_plan.py`` (plan
steps). Every Answer arm sends the contract's default /3 budget
(``common.ANSWER_BUDGET_DEFAULT``) unless ``--budget`` overrides fields.

Gate arms (one series, both repositories of the loop pair, munarium and
bookbuilder, and their 20 development scenarios; no lifetime holds an airk
index):

| Arm | Configuration | Path | Focus | Context | Use |
| --- | --- | --- | --- | --- | --- |
| F | ``focus`` | focus | the gate's Focus route | none | 12.7.2 (judged) |
| C | ``compact`` | compact | the gate's Focus route | Compact FP16 CUDA | 12.7.1 and 12.7.2 (judged) |
| L | ``large`` | large | the gate's Focus route | Large FP16 CUDA | 12.7.1 (judged) |
| C32 | ``compactFp32`` | compact | the gate's Focus route | Compact FP32 CUDA | 12.7.3, replays only |
| L32 | ``largeFp32`` | large | the gate's Focus route | Large FP32 CUDA | 12.7.3, replays only |
| CF16 | ``compactFocusFp16`` | compact | Focus FP16 CUDA | Compact FP16 CUDA | 12.7.3, replays only |
| CF32 | ``compactFocusFp32`` | compact | Focus FP32 CUDA | Compact FP16 CUDA | 12.7.3, replays only |

The gate's Focus route is W8 at 1,024 tokens (changes entry 10); the Focus
precision arms compare FP32 with FP16 and FP16 with the W8 route of arm C.
A precision arm runs judged sessions only when ``gate.py precision`` escalates
it; its entry is then run with ``run_pass.py --only <name>``.

Arm identity: an entry's ``arm_check`` is checked on every replay and on the
digest call before any agent session (``run_pass``), from the reply's own
decisions (model, device, precision, top-score mode, no abstention). Gate
entries carry ``arm_stages`` (for the switch hook), not ``stages``: the CUDA
stage names are not what ``handle.stages()`` reports (recheck after L5).
Gate entries do not reuse sessions across arms (``reuse: false``): later
Answer calls in a session run the arm's own model.

The arms run side by side: each arm has its own fixture lifetime (fixture
root, service and control root), launched together by ``run_plan.py``'s
``lifetimes`` step after a device and host memory check, so no Context switch
is on the gate's path. The in-lifetime switch hook ``switch_context`` (plan
12.5.2 item 4) stays for single-lifetime use. The gate's concurrency test is
restored (the user widened LG to 24 hours; L2b wires the job table into the
live Answer path): 2 and 4 concurrent Answers per arm, one arm at a time.

``python -I -B make_loop_spec.py gate --series 06-gate-lg --lifetimes <L5C lifetimes.json>``
``python -I -B make_loop_spec.py l7 --series 07-loop-baseline --keep compact``
``python -I -B make_loop_spec.py template --out scripts/arbiter-answer-eval/l7_plan_template.json``
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import make_cs_spec  # noqa: E402
import run_session  # noqa: E402

# Profile names to recheck against fixture.py / gpu_fixture.py once L5 adds the FP16 CUDA rows.
PROFILES = {
    # The CUDA Focus seam is closed (GPU_FIXTURE.md, 22 November 2026): every gate arm runs Focus on CUDA, by
    # the pair's Focus row. The gate is CUDA only; the CPU twin (focus-static1024-cpu) is about 35 times slower
    # a pair and would be staged silently if this named it.
    "focus": "focus-1024-cuda",
    "focus_fp16": "focus-static1024-cuda-fp16",
    "focus_fp32": "focus-static1024-cuda-fp32",
    # The pair rows' names in GPU_FIXTURE.md; the FP32 rows are gpu_fixture.py's single-model CUDA rows.
    "compact": "context-compact-fp16-cuda",
    "compact_fp32": "context-compact-cuda",
    "large": "context-large-fp16-cuda",
    "large_fp32": "context-large-cuda",
}
JUDGED_ARMS = ("F", "C", "L")
PRECISION_ARMS = ("C32", "L32", "CF16", "CF32")
# (stage, FP32-side arm, FP16-side arm): 12.7.3's comparisons, one stage at a time.
PRECISION_PAIRS = (("compact", "C32", "C"), ("large", "L32", "L"), ("focus", "CF32", "CF16"), ("focus", "CF16", "C"))
GATE_PAIRS = (("C", "L"), ("F", "C"))


def _arm(arm: str, configuration: str, path: str, focus: str, context: str | None, check: dict, *,
         replay_only: bool = False, precision_stage: str | None = None) -> dict:
    # Every arm that stages Focus requires it to take part (loop_reply.stage_problems): on 23 November 2026 a
    # Focus-only arm whose search reads all failed ran no Focus call and passed the old check.
    check = dict(check, focus=True) if focus else dict(check)
    entry = {"arm": arm, "configuration": configuration, "path": path, "device": "gpu",
             "arm_stages": {"focus": focus, "context": context}, "arm_check": check, "reuse": False,
             "call_timeout": 900, "session_timeout": 3600}
    if replay_only:
        entry["replay_only"] = True
    if precision_stage:
        entry["precision_stage"] = precision_stage
    return entry


def gate_arms(profiles: dict) -> list[dict]:
    p = profiles
    compact16 = {"context": "context_compact", "device": "cuda", "precision": "float16", "mode": "top_score"}
    large16 = {"context": "context_large", "device": "cuda", "precision": "float16", "mode": "top_score"}
    return [
        _arm("F", "focus", "focus", p["focus"], None, {"context": None}),
        _arm("C", "compact", "compact", p["focus"], p["compact"], compact16),
        _arm("L", "large", "large", p["focus"], p["large"], large16),
        _arm("C32", "compactFp32", "compact", p["focus"], p["compact_fp32"], dict(compact16, precision="float32"),
             replay_only=True, precision_stage="compact"),
        _arm("L32", "largeFp32", "large", p["focus"], p["large_fp32"], dict(large16, precision="float32"),
             replay_only=True, precision_stage="large"),
        _arm("CF16", "compactFocusFp16", "compact", p["focus_fp16"], p["compact"], compact16,
             replay_only=True, precision_stage="focus"),
        _arm("CF32", "compactFocusFp32", "compact", p["focus_fp32"], p["compact"], compact16,
             replay_only=True, precision_stage="focus"),
    ]


def base_spec(series: str, r_root: Path, repos: list[str], note: str, budget: dict) -> dict:
    return {
        "series": series, "pilot": False, "note": note,
        "scenarios_dir": str(r_root / "scenarios"), "repos": repos,
        "scenario_ids": [f"{repo}-{slot}" for repo in repos for slot in sorted(common.SLOTS)],
        "freeze": str(r_root / "scenario-freeze.json"), "prompts_freeze": str(r_root / "design" / "prompts-freeze.json"),
        "settle_repos": repos, "handle": common.NEWEST, "claude": make_cs_spec.CLAUDE,
        "agent_model": common.AGENT_MODEL, "agent_effort": common.AGENT_EFFORT,
        "judge_model": common.JUDGE_MODEL, "judge_effort": common.JUDGE_EFFORT, "judges": ["j1", "j2"],
        "concurrency": 4, "judge_concurrency": 4, "prior_series": [],
        "answer_defaults": {"budget": dict(common.ANSWER_BUDGET_DEFAULT, **budget)},
    }


# Side-by-side arm lifetimes (coordinator, 22 Nov 2026): each arm runs in its own fixture lifetime, with its own
# service, control root (the service's runtime directory, so no shared port or socket) and fixture root, so the
# arms run at the same time instead of switching one lifetime. Device and host memory are declared per lifetime
# and checked before launch (run_plan ``lifetimes``). Figures are the measured FP16 peaks rounded up with the
# declared headroom (changes entry 17: Compact 8 + 2 GiB, Large 18 + 2 GiB) and the FP32 envelopes
# (GPU_FIXTURE.md: Large about 40 GiB host and a 44 GiB device envelope). ``start_args`` are the launcher's
# ``start`` options after ``--lifetime``; L5C supplies the real ones through ``--lifetimes <file>``.
LIFETIME_DEFAULTS = {
    "F": {"launcher": "fixture.py", "device_gib": 0, "host_gib": 8,
          "start_args": ["--image", "<cpu image>", "--restart-from", "<closed lifetime of this root>"]},
    "C": {"launcher": "gpu_fixture.py", "device_gib": 10, "host_gib": 16,
          "start_args": ["--image", "<cuda image>", "--model", "pair", "--pair", "<pair.json>",
                         "--pair-sha256", "<sha256>", "--restart-from", "<closed lifetime of this root>"]},
    "L": {"launcher": "gpu_fixture.py", "device_gib": 20, "host_gib": 40,
          "start_args": ["--image", "<cuda image>", "--model", "pair", "--pair", "<pair.json>",
                         "--pair-sha256", "<sha256>", "--restart-from", "<closed lifetime of this root>"]},
    "C32": {"launcher": "gpu_fixture.py", "device_gib": 16, "host_gib": 16,
            "start_args": ["--image", "<cuda image>", "--model", "compact", "--restart-from", "<closed lifetime>"]},
    "L32": {"launcher": "gpu_fixture.py", "device_gib": 44, "host_gib": 40,
            "start_args": ["--image", "<cuda image>", "--model", "large", "--restart-from", "<closed lifetime>"]},
    "CF16": {"launcher": "gpu_fixture.py", "device_gib": 13, "host_gib": 16,
             "start_args": ["--image", "<cuda image>", "--model", "pair", "--pair", "<pair.json with a Focus row>",
                            "--pair-sha256", "<sha256>", "--restart-from", "<closed lifetime>"]},
    "CF32": {"launcher": "gpu_fixture.py", "device_gib": 14, "host_gib": 16,
             "start_args": ["--image", "<cuda image>", "--model", "pair", "--pair", "<pair.json with a Focus row>",
                            "--pair-sha256", "<sha256>", "--restart-from", "<closed lifetime>"]},
}
# Launch phases: the judged arms together, then the precision arms in groups that fit 48 GiB.
JUDGED_PHASE = ("C", "L", "F")
# Each phase is followed by its comparisons, so a phase that cannot launch (the CUDA Focus seam is an open gap)
# stops the plan after the others have reported; the Focus phase is therefore last.
PRECISION_PHASES = (("C32",), ("L32",), ("CF16", "CF32"))
# Parallel headless sessions (item 6): every judged arm at once in one run_pass process, a few sessions per arm
# so each arm's service sees a few concurrent Answers (more queue as tickets through the job table).
# No account rate limit is known to the harness; 429s or overload surface as failed sessions, which the pass
# records and resumes. Lower these if they appear.
ARM_SESSIONS = 4
JUDGE_SESSIONS = 12


# The fixture's native dispatch census (fixture.py census_bound). Each LG gate lifetime passed the old fixed
# bound of 1,024 dispatches and none settled its close (23 November 2026), so a gate lifetime's bound comes from
# its plan: the Answers the plan sends to it, each allowed the budget's Focus and Context calls (one native
# dispatch per model call). Agent sessions choose their own number of Answer calls; SESSION_ANSWERS is the
# allowance per session (first and second), above the LG sessions' use. Past the bound the recorder fails with a
# message naming it, so a lifetime that needs more is seen, not silently cut.
SESSION_ANSWERS = 12
REPLAY_ANSWERS = 2
CENSUS_FLOOR = 1024
CENSUS_MAX = 65536


def lifetime_answers(scenarios: int, repos: int, concurrency_levels: list[int]) -> int:
    """The Answers a gate plan sends to one arm's lifetime: the smoke (one per repository), the replays, a first
    and a second session per scenario, and the concurrency runs."""
    return repos + scenarios * REPLAY_ANSWERS + scenarios * 2 * SESSION_ANSWERS + sum(concurrency_levels)


def dispatch_census(answers: int, budget: dict) -> int:
    """The census bound for a lifetime of ``answers`` Answers at ``budget``: each Answer may dispatch its
    ``focus_calls`` and ``context_calls``. Never below the old bound; refused above the service's ceiling."""
    per_answer = int(budget["focus_calls"]) + int(budget["context_calls"])
    common.require(per_answer > 0 and answers > 0, "a census needs Answers and model calls")
    census = max(CENSUS_FLOOR, answers * per_answer)
    common.require(census <= CENSUS_MAX, f"{answers} Answers at {per_answer} model calls each need a census of "
                                         f"{census}, above the fixture's {CENSUS_MAX}: split the lifetime")
    return census


def with_census(row: dict, census: int) -> dict:
    """A lifetime row whose launcher start options carry ``--dispatch-census``; a row that names one keeps it
    only when it is at least the plan's."""
    args = list(row.get("start_args") or [])
    if "--dispatch-census" in args:
        named = int(args[args.index("--dispatch-census") + 1])
        common.require(named >= census, f"lifetime {row.get('lifetime')} names a dispatch census of {named}, "
                                        f"below the {census} its plan needs")
        return dict(row)
    common.require("--no-dispatch-observer" not in args, "a gate lifetime keeps the dispatch observer")
    return dict(row, start_args=args + ["--dispatch-census", str(census)], dispatch_census=census)


def gate_lifetimes(series: str, r_root: Path, overrides: dict | None = None) -> dict[str, dict]:
    """One lifetime per arm: its fixture root, lifetime name, launcher, start options, memory and stages."""
    out = {}
    for arm, defaults in LIFETIME_DEFAULTS.items():
        row = {**defaults, **((overrides or {}).get(arm) or {})}
        row.setdefault("fixtures", str(r_root / f"fixture-{series}-{arm}"))
        row.setdefault("lifetime", f"{series}-{arm}")
        row["arm"] = arm
        out[arm] = row
    return out


def stage_guard(fixtures: str, lifetime: str, arm_stages: dict) -> list[str] | None:
    """The command run_pass runs before each session of a CUDA arm (``gpu_fixture.py ensure``): the arm's stages
    must be active, and are re-enabled if a stage unloaded."""
    profiles = [p for p in (arm_stages.get("focus"), arm_stages.get("context")) if p]
    if not profiles:
        return None
    return [sys.executable, "-I", "-B", str(HERE / "gpu_fixture.py"), "--fixtures", str(fixtures), "ensure",
            "--lifetime", lifetime, "--profiles", ",".join(profiles)]


def handle_path(lifetime: dict) -> str:
    return str(Path(lifetime["fixtures"]) / lifetime["lifetime"] / "handle.json")


# Why the precision check (plan 12.7.3) is not part of the LG run (coordinator's decision, 23 November 2026).
PRECISION_DEFERRED = ("Deferred: the FP32 Context profiles declare only the one-turn task, so every card call in "
                      "an FP32 arm would be not dispatched and the arms would compare nothing. The check is replay "
                      "only and no gate of its own; it runs after L7, when FP32 card-task profiles exist. The "
                      "roots built for it (C32, L32) are kept as spare roots for the judged arms.")


def gate_spec(series: str, r_root: Path, profiles: dict, repos: list[str], budget: dict, note: str = "",
              lifetimes: dict | None = None, precision: bool = False) -> dict:
    """The LG series (plan 12.8). Each arm entry names its own lifetime's handle; the judged arms run their
    sessions together (``parallel_entries``); ``replay_only`` arms run no sessions unless named. The precision
    arms are entries only when ``precision`` is set (``PRECISION_DEFERRED``)."""
    spec = base_spec(series, r_root, repos, note or "LG: plan 12.7.1 and 12.7.2" + (", and 12.7.3" if precision
                                                                                     else ""), budget)
    lifetimes = lifetimes or gate_lifetimes(series, r_root)
    levels = [2, 4]
    census = dispatch_census(lifetime_answers(len(spec["scenario_ids"]), len(repos), levels),
                             spec["answer_defaults"]["budget"])
    lifetimes = {arm: with_census(row, census) for arm, row in lifetimes.items()}
    spec["gate"] = {"pairs": [list(p) for p in GATE_PAIRS],
                    "precision_pairs": [list(p) for p in PRECISION_PAIRS] if precision else [],
                    "precision": {"run": bool(precision), "reason": None if precision else PRECISION_DEFERRED},
                    # The job table is not part of the gate: concurrent Answers take the unqueued path. Every
                    # concurrent call is a different scenario's question (gate.concurrency).
                    "concurrency_path": "unqueued (no job table); a distinct scenario question per concurrent call",
                    "profiles": dict(profiles), "lifetimes": lifetimes, "concurrency_levels": levels,
                    "dispatch_census": census}
    spec["results"] = []
    for entry in gate_arms(profiles):
        if not precision and entry["arm"] not in JUDGED_ARMS:
            continue
        entry["handle"] = handle_path(lifetimes[entry["arm"]])
        if lifetimes[entry["arm"]].get("launcher") == "gpu_fixture.py":
            entry["stage_guard"] = stage_guard(lifetimes[entry["arm"]]["fixtures"], lifetimes[entry["arm"]]["lifetime"],
                                               entry["arm_stages"])
        entry["concurrency"] = ARM_SESSIONS
        spec["results"].append(entry)
    spec["parallel_entries"] = True
    spec["settle_per_root"] = True
    spec["concurrency"] = ARM_SESSIONS * len(JUDGED_ARMS)
    spec["judge_concurrency"] = JUDGE_SESSIONS
    spec["settle_repos"] = repos
    return spec


def result_name(entry: dict) -> str:
    return run_session.result_name(entry["configuration"], entry.get("device", "none"))


def _arm_lifetime(lifetime: dict, arms: dict, repos: list[str]) -> dict:
    stages = arms[lifetime["arm"]]["arm_stages"]
    return {**lifetime, "repositories": ",".join(repos),
            "stages": [{"stage": k, "profile": v} for k, v in (("focus", stages["focus"]),
                                                                ("context", stages["context"])) if v]}


def gate_plan(spec_path: Path, spec: dict, r_root: Path) -> list[dict]:
    """run_plan steps for LG: the three judged arms side by side in their own lifetimes (memory checked before
    launch), their replays in parallel, all judged sessions at once, grading, second sessions on close calls,
    the gate tests; then the concurrency test; then performance; then, only when the spec runs them, the
    precision arms one phase at a time, each with its comparisons. The concurrency test (2 and 4 concurrent
    Answers per arm, a different scenario's question in every concurrent call, on the unqueued path) runs one
    arm at a time after every judged and second session has finished and before the lifetimes stop, so it
    cannot disturb a judged session and the arms do not share the device while it measures throughput."""
    series_dir = r_root / "passes" / spec["series"]
    gate_dir = series_dir / "gate"
    arms = {e["arm"]: e for e in spec["results"]}
    lifetimes = spec["gate"]["lifetimes"]
    repos = spec["repos"]

    def launch(names):
        return {"lifetimes": {"start": [_arm_lifetime(lifetimes[n], arms, repos) for n in names],
                              "reserve_device_gib": 4, "reserve_host_gib": 16,
                              "receipt": str(gate_dir / "lifetimes" / f"launch-{'-'.join(names)}.json")}}

    def replays(names):
        return {"parallel": [{"gate": {"command": "replays", "spec": str(spec_path), "arm": n,
                                       "out": str(gate_dir / n)}} for n in names]}

    def smoke(names):
        # One live Answer per repository and arm before any replay or session (gate.py smoke): a search record
        # without a failure, a Focus call, and model decisions where a Context model is staged.
        return {"parallel": [{"gate": {"command": "smoke", "spec": str(spec_path), "arm": n,
                                       "out": str(gate_dir / n)}} for n in names]}

    def stop(names):
        return {"stop_lifetimes": {"stop": [_arm_lifetime(lifetimes[n], arms, repos) for n in names]}}

    judged = ",".join(result_name(arms[a]) for a in JUDGED_PHASE)
    steps: list[dict] = [launch(JUDGED_PHASE), smoke(JUDGED_PHASE), replays(JUDGED_PHASE)]
    levels = ",".join(str(n) for n in spec["gate"].get("concurrency_levels") or [2, 4])
    steps += [{"sessions": {"spec": str(spec_path), "groups": [["@any", "@any", judged]]}},
              {"grade": {"spec": str(spec_path)}}]
    for a, b in GATE_PAIRS:
        steps.append({"gate": {"command": "outcomes", "pass_dir": str(series_dir), "a": result_name(arms[a]),
                               "b": result_name(arms[b]), "out": str(gate_dir / f"outcomes-{a}-{b}.json")}})
    steps.append({"second": {"spec": str(spec_path), "groups": [["@any", "@any", judged]],
                             "jobs": [str(gate_dir / f"outcomes-{a}-{b}.second-jobs.json") for a, b in GATE_PAIRS]}})
    for a, b in GATE_PAIRS:
        steps.append({"gate": {"command": "outcomes", "pass_dir": str(series_dir), "a": result_name(arms[a]),
                               "b": result_name(arms[b]), "out": str(gate_dir / f"outcomes-{a}-{b}.json")}})
        steps.append({"gate": {"command": "agreement", "a": str(gate_dir / a), "b": str(gate_dir / b),
                               "spec": str(spec_path), "out": str(gate_dir / f"agreement-{a}-{b}.json")}})
    for arm in JUDGED_PHASE:
        steps.append({"gate": {"command": "concurrency", "spec": str(spec_path), "arm": arm, "levels": levels,
                               "out": str(gate_dir / "concurrency" / arm)}})
    steps.append(stop(JUDGED_PHASE))
    steps.append({"gate": {"command": "performance",
                           "arm_dir": [f"{x}={gate_dir / x}" for x in JUDGED_ARMS],
                           "concurrency": [f"{x}={gate_dir / 'concurrency' / x}" for x in JUDGED_ARMS],
                           "out": str(gate_dir / "performance.json")}})
    done = set(JUDGED_PHASE)
    for phase in PRECISION_PHASES if (spec["gate"].get("precision") or {}).get("run") else ():
        steps += [launch(phase), smoke(phase), replays(phase), stop(phase)]
        done |= set(phase)
        for stage, fp32, fp16 in PRECISION_PAIRS:
            if (fp32 in phase or fp16 in phase) and {fp32, fp16} <= done:
                steps.append({"gate": {"command": "precision", "fp32": str(gate_dir / fp32),
                                       "fp16": str(gate_dir / fp16), "stage": stage, "spec": str(spec_path),
                                       "out": str(gate_dir / f"precision-{stage}-{fp32}-{fp16}.json")}})
    return steps


def gate_report_inputs(spec: dict, r_root: Path) -> dict:
    """The manifest ``gate_report.py`` reads (all private paths under R)."""
    gate_dir = r_root / "passes" / spec["series"] / "gate"
    return {"schema": "airk.private.answer-eval-gate-report-inputs/1", "series": spec["series"],
            "pass_dir": str(r_root / "passes" / spec["series"]),
            "arms": {e["arm"]: str(gate_dir / e["arm"]) for e in spec["results"]},
            "agreement": {f"{a}-{b}": str(gate_dir / f"agreement-{a}-{b}.json") for a, b in GATE_PAIRS},
            "outcomes": {f"{a}-{b}": str(gate_dir / f"outcomes-{a}-{b}.json") for a, b in GATE_PAIRS},
            "precision": [str(gate_dir / f"precision-{s}-{x}-{y}.json")
                          for s, x, y in spec["gate"].get("precision_pairs") or []],
            "precision_note": (spec["gate"].get("precision") or {}).get("reason"),
            "concurrency_path": spec["gate"].get("concurrency_path"),
            "performance": str(gate_dir / "performance.json"),
            "lifetimes": str(gate_dir / "lifetimes")}


# ---------------------------------------------------------------- L7


def l7_spec(series: str, r_root: Path, profiles: dict, repos: list[str], keep: list[str], budget: dict,
            note: str = "") -> dict:
    """The new baseline of L7 (plan 12.6, 12.8): Parse, Search, the Focus-only loop and the loop on every CUDA
    route the gate decisions keep. CUDA only: no CPU cells."""
    for name in keep:
        common.require(name in ("compact", "large"), f"unknown kept route {name!r}")
    spec = base_spec(series, r_root, repos, note or "L7 baseline: the loop on CUDA, Parse and Search", budget)
    arms = {e["arm"]: e for e in gate_arms(profiles)}
    results = [{"configuration": "search", "path": "search", "device": "none"},
               {"configuration": "parse", "path": "parse", "device": "none"},
               {**arms["F"], "reuse": True}]
    for name in keep:
        results.append({**arms[{"compact": "C", "large": "L"}[name]], "reuse": True})
    spec["results"] = results
    return spec


def l7_plan(spec_path: Path, spec: dict, r_root: Path) -> list[dict]:
    loops = [e for e in spec["results"] if e.get("arm")]
    # Search and Parse do not use the Arbiter stages: "@any" leaves the live stages as they are.
    steps: list[dict] = [{"sessions": {"spec": str(spec_path), "groups": [["@any", "@any", "search,parse"]]}}]
    steps.append({"sessions": {"spec": str(spec_path), "stager": "cuda", "groups": [
        [e["arm_stages"]["focus"], e["arm_stages"]["context"], result_name(e)] for e in loops]}})
    steps.append({"grade": {"spec": str(spec_path)}})
    steps.append({"second": {"spec": str(spec_path), "stager": "cuda", "groups": [
        [e["arm_stages"]["focus"], e["arm_stages"]["context"], result_name(e)] for e in loops]}})
    return steps


# ---------------------------------------------------------------- shakedown


def shakedown_spec(series: str, r_root: Path, out_root: Path, handle: str, scenario_ids: list[str],
                   profiles: dict, budget: dict, arm: str = "C") -> dict:
    """A pipeline test before the gate (GPU_FIXTURE.md, "Before the gate"): arm C alone, on one non-gate
    lifetime (or ``arm``, such as F), a few scenarios. Scenarios and the scenario freeze are R's own; the prompts freeze and every
    output live under ``out_root``, so nothing of a shakedown lands in ``R/passes``. Its verdicts mean nothing."""
    repos = sorted({s.split("-", 1)[0] for s in scenario_ids})
    common.require(repos and all(r in common.REPOS for r in repos), f"unknown repositories in {scenario_ids}")
    spec = base_spec(series, r_root, repos, "shakedown: a pipeline test, never a result", budget)
    spec["scenario_ids"] = list(scenario_ids)
    spec["prompts_freeze"] = str(out_root / "design" / "prompts-freeze.json")
    spec["settle_repos"] = repos
    entry = {e["arm"]: e for e in gate_arms(profiles)}[arm]
    spec["gate"] = {"pairs": [], "precision_pairs": [], "profiles": dict(profiles), "lifetimes": {},
                    "precision": {"run": False, "reason": "a shakedown runs no precision check"},
                    "concurrency_path": "unqueued (no job table); a distinct scenario question per concurrent call",
                    "concurrency_levels": [2]}
    handle_file = Path(handle)
    spec["results"] = [dict(entry, handle=handle, concurrency=1,
                            stage_guard=stage_guard(str(handle_file.parent.parent), handle_file.parent.name,
                                                    entry["arm_stages"]))]
    spec["concurrency"] = 1
    spec["judge_concurrency"] = 2
    return spec


PLACEHOLDER_R = Path("<R>")


def template() -> dict:
    """The L7 plan template: the spec and plan for ``--keep compact,large`` with placeholders for R and the
    series; ``make_loop_spec.py l7`` writes the concrete files."""
    spec = l7_spec("<series>", PLACEHOLDER_R, PROFILES, list(common.REPOS), ["compact", "large"], {})
    spec_path = PLACEHOLDER_R / "passes" / "<series>" / "spec.json"
    return {"schema": "airk.private.answer-eval-l7-plan-template/1",
            "note": ("Generated by make_loop_spec.py template. Keep only the routes the gate decisions keep "
                     "(--keep); prompts-freeze.json must be re-frozen for the loop before the first judged pass."),
            "spec": json.loads(json.dumps(spec).replace("\\\\", "/")),
            "plan": json.loads(json.dumps(l7_plan(spec_path, spec, PLACEHOLDER_R)).replace("\\\\", "/"))}


def _write(r_root: Path, series: str, spec: dict, plan: list[dict], extra: dict | None = None) -> Path:
    directory = r_root / "passes" / series
    common.write_json(directory / "spec.json", spec)
    common.write_json(directory / "plan.json", plan)
    if extra is not None:
        common.write_json(directory / "gate-report-inputs.json", extra)
    return directory


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("gate", "l7"):
        sub = commands.add_parser(name)
        sub.add_argument("--series", required=True)
        sub.add_argument("--repos", default=",".join(common.REPOS))
        sub.add_argument("--budget", default="{}", help="JSON fields over the default /3 budget")
        sub.add_argument("--profiles", default="{}", help="JSON profile-name overrides (keys of PROFILES)")
        sub.add_argument("--note", default="")
        if name == "gate":
            sub.add_argument("--lifetimes", type=Path,
                             help="JSON {arm: {fixtures, lifetime, launcher, start_args, device_gib, host_gib}} overrides")
        if name == "l7":
            sub.add_argument("--keep", required=True, help="CUDA routes the gate decisions keep: compact,large")
    sub = commands.add_parser("template")
    sub.add_argument("--out", type=Path, required=True)
    sub = commands.add_parser("shakedown")
    sub.add_argument("--series", required=True)
    sub.add_argument("--out-root", type=Path, required=True, help="the shakedown's own r-root for run_pass and gate")
    sub.add_argument("--handle", required=True, help="the shakedown lifetime's handle.json")
    sub.add_argument("--scenarios", required=True, help="comma-separated scenario ids, e.g. munarium-A1")
    sub.add_argument("--arm", default="C", choices=JUDGED_ARMS)
    sub.add_argument("--budget", default="{}")
    args = parser.parse_args(argv)
    if args.command == "template":
        common.write_json(args.out, template())
        print(args.out)
        return 0
    if args.command == "shakedown":
        common.require(not (args.out_root / "passes" / args.series / "spec.json").exists(),
                       "the shakedown spec exists; delete it by its exact path first")
        spec = shakedown_spec(args.series, args.r_root, args.out_root, args.handle,
                              [s for s in args.scenarios.split(",") if s], dict(PROFILES), json.loads(args.budget), args.arm)
        directory = args.out_root / "passes" / args.series
        common.write_json(directory / "spec.json", spec)
        common.write_json(directory / "gate-report-inputs.json", gate_report_inputs(spec, args.out_root))
        print(directory)
        return 0
    profiles = dict(PROFILES, **json.loads(args.profiles))
    unknown = sorted(set(profiles) - set(PROFILES))
    common.require(not unknown, f"unknown profile keys {unknown}")
    repos = [r for r in args.repos.split(",") if r]
    budget = json.loads(args.budget)
    spec_path = args.r_root / "passes" / args.series / "spec.json"
    if args.command == "gate":
        overrides = common.read_json(args.lifetimes) if args.lifetimes else None
        if overrides:
            unknown_arms = sorted(set(overrides) - set(LIFETIME_DEFAULTS))
            common.require(not unknown_arms, f"unknown arms in --lifetimes: {unknown_arms}")
        spec = gate_spec(args.series, args.r_root, profiles, repos, budget, args.note,
                         gate_lifetimes(args.series, args.r_root, overrides))
        directory = _write(args.r_root, args.series, spec, gate_plan(spec_path, spec, args.r_root),
                           gate_report_inputs(spec, args.r_root))
    else:
        spec = l7_spec(args.series, args.r_root, profiles, repos, args.keep.split(","), budget, args.note)
        directory = _write(args.r_root, args.series, spec, l7_plan(spec_path, spec, args.r_root))
    print(directory)
    return 0


if __name__ == "__main__":
    sys.exit(main())
