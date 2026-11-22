# SPDX-License-Identifier: Apache-2.0
"""Pass specs and run plans for the loop: the LG gate series (plan 12.7) and the L7 baseline (plan 12.6, 12.8).

Formats are those of ``run_pass.py`` (pass spec) and ``run_plan.py`` (plan
steps). Every Answer arm sends the contract's default /3 budget
(``common.ANSWER_BUDGET_DEFAULT``) unless ``--budget`` overrides fields.

Gate arms (one series, both repositories, the 20 development scenarios):

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

The in-lifetime Context switch is the named hook ``switch_context`` of
``run_plan.py`` (plan 12.5.2 item 4: disable the current Context, wait for the
release, enable the next profile, all through ordinary Configure via
``gpu_fixture.py stage``), with a receipt of release and warm time.

``python -I -B make_loop_spec.py gate --series 05-gate-lg``
``python -I -B make_loop_spec.py l7 --series 06-loop-baseline --keep compact``
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
    "focus": "focus-static1024-cuda-w8",
    "focus_fp16": "focus-static1024-cuda-fp16",
    "focus_fp32": "focus-static1024-cuda-fp32",
    "compact": "context-compact-cuda-fp16",
    "compact_fp32": "context-compact-cuda",
    "large": "context-large-cuda-fp16",
    "large_fp32": "context-large-cuda",
}
JUDGED_ARMS = ("F", "C", "L")
PRECISION_ARMS = ("C32", "L32", "CF16", "CF32")
# (stage, FP32-side arm, FP16-side arm): 12.7.3's comparisons, one stage at a time.
PRECISION_PAIRS = (("compact", "C32", "C"), ("large", "L32", "L"), ("focus", "CF32", "CF16"), ("focus", "CF16", "C"))
GATE_PAIRS = (("C", "L"), ("F", "C"))


def _arm(arm: str, configuration: str, path: str, focus: str, context: str | None, check: dict, *,
         replay_only: bool = False, precision_stage: str | None = None) -> dict:
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


def gate_spec(series: str, r_root: Path, profiles: dict, repos: list[str], budget: dict, note: str = "") -> dict:
    """The LG series (plan 12.8): the judged arms run sessions; ``replay_only`` arms are left out of the
    sessions stage by name (``run_plan`` sessions groups list only F, C and L)."""
    spec = base_spec(series, r_root, repos, note or "LG: plan 12.7.1, 12.7.2 and 12.7.3", budget)
    spec["gate"] = {"pairs": [list(p) for p in GATE_PAIRS], "precision_pairs": [list(p) for p in PRECISION_PAIRS],
                    "profiles": dict(profiles)}
    spec["results"] = gate_arms(profiles)
    return spec


def result_name(entry: dict) -> str:
    return run_session.result_name(entry["configuration"], entry.get("device", "none"))


def gate_plan(spec_path: Path, spec: dict, r_root: Path) -> list[dict]:
    """run_plan steps for LG, in the L5 design's order: arm C and arm L in one lifetime, then arm F, then the
    precision replays; then the judged sessions per arm, grading, the gate tests and the report inputs."""
    series_dir = r_root / "passes" / spec["series"]
    gate_dir = series_dir / "gate"
    arms = {e["arm"]: e for e in spec["results"]}
    steps: list[dict] = []
    switch_no = 0

    def switch(arm: str):
        nonlocal switch_no
        switch_no += 1
        stages = arms[arm]["arm_stages"]
        steps.append({"switch_context": {"focus": stages["focus"], "context": stages["context"],
                                         "repositories": ",".join(spec["repos"]),
                                         "receipt": str(gate_dir / "switches" / f"{switch_no:02d}-{arm}.json")}})

    for arm in ("C", "L", "F"):
        switch(arm)
        steps.append({"gate": {"command": "replays", "spec": str(spec_path), "arm": arm, "out": str(gate_dir / arm)}})
        steps.append({"gate": {"command": "concurrency", "spec": str(spec_path), "arm": arm, "levels": "2,4",
                               "out": str(gate_dir / "concurrency" / arm)}})
        steps.append({"sessions": {"spec": str(spec_path), "stager": "cuda", "groups": [
            [arms[arm]["arm_stages"]["focus"], arms[arm]["arm_stages"]["context"], result_name(arms[arm])]]}})
    for arm in PRECISION_ARMS:
        switch(arm)
        steps.append({"gate": {"command": "replays", "spec": str(spec_path), "arm": arm, "out": str(gate_dir / arm)}})
    steps.append({"grade": {"spec": str(spec_path)}})
    for a, b in GATE_PAIRS:
        steps.append({"gate": {"command": "outcomes", "pass_dir": str(series_dir), "a": result_name(arms[a]),
                               "b": result_name(arms[b]), "out": str(gate_dir / f"outcomes-{a}-{b}.json")}})
    groups = [[arms[x]["arm_stages"]["focus"], arms[x]["arm_stages"]["context"], result_name(arms[x])]
              for x in JUDGED_ARMS]
    steps.append({"second": {"spec": str(spec_path), "stager": "cuda", "groups": groups,
                             "jobs": [str(gate_dir / f"outcomes-{a}-{b}.second-jobs.json") for a, b in GATE_PAIRS]}})
    for a, b in GATE_PAIRS:
        steps.append({"gate": {"command": "outcomes", "pass_dir": str(series_dir), "a": result_name(arms[a]),
                               "b": result_name(arms[b]), "out": str(gate_dir / f"outcomes-{a}-{b}.json")}})
        steps.append({"gate": {"command": "agreement", "a": str(gate_dir / a), "b": str(gate_dir / b),
                               "spec": str(spec_path), "out": str(gate_dir / f"agreement-{a}-{b}.json")}})
    for stage, fp32, fp16 in PRECISION_PAIRS:
        steps.append({"gate": {"command": "precision", "fp32": str(gate_dir / fp32), "fp16": str(gate_dir / fp16),
                               "stage": stage, "spec": str(spec_path),
                               "out": str(gate_dir / f"precision-{stage}-{fp32}-{fp16}.json")}})
    steps.append({"gate": {"command": "performance",
                           "arm_dir": [f"{x}={gate_dir / x}" for x in (*JUDGED_ARMS, *PRECISION_ARMS)],
                           "concurrency": [f"{x}={gate_dir / 'concurrency' / x}" for x in JUDGED_ARMS],
                           "switch_dir": str(gate_dir / "switches"),
                           "out": str(gate_dir / "performance.json")}})
    return steps


def gate_report_inputs(spec: dict, r_root: Path) -> dict:
    """The manifest ``gate_report.py`` reads (all private paths under R)."""
    gate_dir = r_root / "passes" / spec["series"] / "gate"
    return {"schema": "airk.private.answer-eval-gate-report-inputs/1", "series": spec["series"],
            "arms": {e["arm"]: str(gate_dir / e["arm"]) for e in spec["results"]},
            "agreement": {f"{a}-{b}": str(gate_dir / f"agreement-{a}-{b}.json") for a, b in GATE_PAIRS},
            "outcomes": {f"{a}-{b}": str(gate_dir / f"outcomes-{a}-{b}.json") for a, b in GATE_PAIRS},
            "precision": [str(gate_dir / f"precision-{s}-{x}-{y}.json") for s, x, y in PRECISION_PAIRS],
            "performance": str(gate_dir / "performance.json"),
            "switches": str(gate_dir / "switches")}


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
        if name == "l7":
            sub.add_argument("--keep", required=True, help="CUDA routes the gate decisions keep: compact,large")
    sub = commands.add_parser("template")
    sub.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.command == "template":
        common.write_json(args.out, template())
        print(args.out)
        return 0
    profiles = dict(PROFILES, **json.loads(args.profiles))
    unknown = sorted(set(profiles) - set(PROFILES))
    common.require(not unknown, f"unknown profile keys {unknown}")
    repos = [r for r in args.repos.split(",") if r]
    budget = json.loads(args.budget)
    spec_path = args.r_root / "passes" / args.series / "spec.json"
    if args.command == "gate":
        spec = gate_spec(args.series, args.r_root, profiles, repos, budget, args.note)
        directory = _write(args.r_root, args.series, spec, gate_plan(spec_path, spec, args.r_root),
                           gate_report_inputs(spec, args.r_root))
    else:
        spec = l7_spec(args.series, args.r_root, profiles, repos, args.keep.split(","), budget, args.note)
        directory = _write(args.r_root, args.series, spec, l7_plan(spec_path, spec, args.r_root))
    print(directory)
    return 0


if __name__ == "__main__":
    sys.exit(main())
