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
prompt, the judge verdict schema (the rubric), the scenario freeze and the default Answer budget (what
every ``airk_answer`` call is sent, read from the Rust-emitted cases file) are compared with the previous
manifest. The line also names every changed harness file.

``python -I -B prompts_freeze.py --write <R>/design/prompts-freeze.json``
``python -I -B prompts_freeze.py --verify <R>/design/prompts-freeze.json``
``python -I -B prompts_freeze.py --refreeze <R>/design/prompts-freeze.json --reason "..."``
``python -I -B prompts_freeze.py --write-codex <fresh-manifest> --codex-inputs <pinned-inputs.json>``
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
CODEX_SCHEMA = "airk.private.answer-eval-codex-freeze/1"
CEILING_PURPOSE = "windows-stage-a-complete-evidence"
CODEX_PURPOSES = {"windows-stage-a": "gpt-6.2-sol", "windows-stage-a-capable-panel": "gpt-6.2-astra",
                  CEILING_PURPOSE: "gpt-6.2-sol"}
CODEX_PATHS = ("search", "parse", "answer")
CODEX_STAGES = ("baseline", "change-set-1", "change-set-2", "change-set-3")
CODEX_CONTROLS = (("search", "search", "none"), ("parse", "parse", "none"))
CODEX_ANSWER_ARMS = {"F": ("focus", "focus", "gpu"), "C": ("compact", "compact", "gpu"),
                     "A": ("arbiter", "arbiter", "gpu")}
CODEX_HARNESS_FILES = ("codex_isolation_probe.py", "make_cs_spec.py", "make_loop_spec.py",
                       "complete_evidence.py", "complete_evidence_pass.py")
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
            # The budget every airk_answer call is sent comes from the Rust-emitted cases file, which is not a
            # harness file: the freeze pins its bytes and the budget read from them.
            "answer_budget": common.answer_budget_source(),
            "files": {name: common.sha256_file(HERE / name) for name in HARNESS_FILES}}


def pinned_input(path: str, sha256: str | None = None) -> dict:
    source = Path(path).resolve(strict=True)
    common.require(source.is_file(), f"freeze input is not a file: {source}")
    actual = common.sha256_file(source)
    common.require(sha256 is None or actual == sha256, f"freeze input pin differs: {source.name}")
    return {"path": str(source), "sha256": actual}


def scenario_roster() -> set[str]:
    return {f"{repo}-{slot}" for repo in common.REPOS for slot in common.SLOTS}


def ceiling_inputs(pins: dict) -> dict:
    common.require(isinstance(pins, dict) and set(pins) == scenario_roster(),
                   "complete-evidence freeze requires exactly 20 scenario inputs")
    frozen = {}
    for repo in common.REPOS:
        identities = set()
        for slot in common.SLOTS:
            case = f"{repo}-{slot}"
            pin = pins[case]
            fields = {"manifest", "manifest_sha256", "source_root", "publication_token"}
            common.require(isinstance(pin, dict) and set(pin) == fields
                           and all(isinstance(pin[key], str) and pin[key] for key in fields),
                           f"{case}: invalid complete-evidence input")
            manifest = pinned_input(pin["manifest"], pin["manifest_sha256"])
            source_root = Path(pin["source_root"]).resolve(strict=True)
            common.require(source_root.is_dir(), f"{case}: complete-evidence source root is unavailable")
            identities.add((str(source_root), pin["publication_token"]))
            frozen[case] = {**pin, "manifest": manifest["path"], "source_root": str(source_root)}
        common.require(len(identities) == 1, f"{repo}: complete-evidence cases use different source identities")
    return frozen


def codex_current(inputs: dict) -> dict:
    import judge

    fields = {"purpose", "scenario_freeze", "executable", "executable_sha256", "model_catalog",
              "model_catalog_sha256", "tool_catalogs"}
    ceiling = isinstance(inputs, dict) and inputs.get("purpose") == CEILING_PURPOSE
    common.require(isinstance(inputs, dict) and set(inputs) == (fields | {"complete_evidence"} if ceiling else fields)
                   and all(isinstance(inputs[field], str) and inputs[field] for field in fields - {"tool_catalogs"})
                   and inputs["purpose"] in CODEX_PURPOSES, "invalid Codex freeze inputs")
    catalog_inputs = inputs["tool_catalogs"]
    common.require(isinstance(catalog_inputs, dict) and set(catalog_inputs) == set(CODEX_PATHS),
                   "Codex freeze requires exactly Search, Parse and Answer catalogs")
    catalogs = {}
    for name in CODEX_PATHS:
        pin = catalog_inputs[name]
        common.require(isinstance(pin, dict) and set(pin) == {"path", "sha256"}
                       and all(isinstance(pin[field], str) and pin[field] for field in pin),
                       f"invalid Codex tool catalog pin for {name}")
        catalog = pinned_input(pin["path"], pin["sha256"])
        tools = common.load_catalog(Path(catalog["path"]))
        common.require(set(common.PATH_TOOLS[name]) <= tools.keys(), f"Codex catalog is missing {name} tools")
        catalogs[name] = catalog
    record = current()
    schema_text = json.dumps(judge.codex_verdict_schema(), separators=(",", ":"), sort_keys=True)
    extra_files = {name: common.sha256_file(HERE / name) for name in CODEX_HARNESS_FILES}
    record.update({
        "provider": "codex",
        "purpose": inputs["purpose"],
        "runner": "codex-app-server-0.171.0-contract",
        "executable": pinned_input(inputs["executable"], inputs["executable_sha256"]),
        "model_catalog": pinned_input(inputs["model_catalog"], inputs["model_catalog_sha256"]),
        "tool_catalogs": catalogs,
        "agent_model": CODEX_PURPOSES[inputs["purpose"]],
        "agent_effort": "medium",
        "judge_model": "gpt-6.2-astra",
        "judge_effort": "medium",
        "scoring_judges": ["j1", "j2"],
        "judge_schema": {"sha256": common.sha256_bytes(schema_text.encode("utf-8")), "text": schema_text},
        "scenario_freeze": pinned_input(inputs["scenario_freeze"]),
        "files": {**record["files"], **extra_files},
    })
    if ceiling:
        record["complete_evidence"] = ceiling_inputs(inputs["complete_evidence"])
        record["diagnostic_judge"] = {"model": "gpt-6.2-sol", "effort": "medium"}
    return record


def write_codex(path: Path, inputs: dict) -> dict:
    common.require(not path.exists(), "Codex freeze requires a new manifest path")
    record = {"schema": CODEX_SCHEMA, "frozen_at": common.now_iso(), **codex_current(inputs)}
    with path.open("x", encoding="utf-8") as output:
        json.dump(record, output, indent=2, ensure_ascii=False)
        output.write("\n")
    return record


def verify_codex(frozen: dict) -> None:
    executable, model_catalog = frozen.get("executable") or {}, frozen.get("model_catalog") or {}
    inputs = {
        "purpose": frozen.get("purpose"),
        "scenario_freeze": (frozen.get("scenario_freeze") or {}).get("path"),
        "executable": executable.get("path"),
        "executable_sha256": executable.get("sha256"),
        "model_catalog": model_catalog.get("path"),
        "model_catalog_sha256": model_catalog.get("sha256"),
        "tool_catalogs": frozen.get("tool_catalogs"),
    }
    if inputs["purpose"] == CEILING_PURPOSE:
        inputs["complete_evidence"] = frozen.get("complete_evidence")
    actual = codex_current(inputs)
    saved = {key: value for key, value in frozen.items() if key not in ("schema", "frozen_at")}
    changed = sorted(key for key in actual.keys() | saved.keys() if actual.get(key) != saved.get(key))
    common.require(not changed, f"Codex evaluation differs from its freeze: {changed}")


def verify_stage_a_spec(spec: dict, manifest: dict, provenance: dict, catalog_hashes: dict) -> None:
    common.require(manifest.get("schema") == CODEX_SCHEMA and manifest.get("provider") == "codex",
                   "Windows Stage A requires a Codex freeze")
    common.require(all(spec.get(key) == manifest[key] for key in
                       ("purpose", "agent_model", "agent_effort", "judge_model", "judge_effort"))
                   and spec.get("judges") == manifest["scoring_judges"],
                   "Windows Stage A roles differ from the Codex freeze")
    common.require(provenance["driver"] == manifest["runner"]
                   and provenance["executable"] == manifest["executable"]["path"]
                   and provenance["executable_sha256"] == manifest["executable"]["sha256"]
                   and provenance["model_catalog_sha256"] == manifest["model_catalog"]["sha256"]
                   and spec["codex_config"]["model_catalog"] == manifest["model_catalog"]["path"],
                   "Windows Stage A runner differs from the Codex freeze")
    ceiling = spec["purpose"] == CEILING_PURPOSE
    expected_stages = (("complete-evidence",) if ceiling else CODEX_STAGES if spec["purpose"] == "windows-stage-a"
                       else (CODEX_STAGES[0], CODEX_STAGES[-1]))
    common.require(spec.get("stage") in expected_stages, "Codex phase differs from its Windows Stage A purpose")
    arms = ("F", "C", "A") if spec["purpose"] == "windows-stage-a" else ("C",)
    expected_paths = {} if ceiling else {None: CODEX_CONTROLS}
    expected_paths.update({"C": (("compact", "answer", "gpu"),)} if ceiling else
                          {arm: (CODEX_ANSWER_ARMS[arm],) for arm in arms})
    scenario_ids = scenario_roster()
    common.require(set(spec.get("repos", [])) == set(common.REPOS)
                   and set(spec.get("scenario_ids", [])) == scenario_ids and not spec.get("slots"),
                   "Windows Stage A requires all 20 frozen development scenarios")
    remaining = {path for paths in expected_paths.values() for path in paths}
    for result in spec["results"]:
        path = (result.get("configuration"), result.get("path"), result.get("device"))
        arm = result.get("arm")
        common.require(path in remaining and path in expected_paths.get(arm, ()) and result.get("reuse") is False,
                       "Windows Stage A result arms differ from the frozen role")
        remaining.remove(path)
        stages = result.get("stages")
        common.require(result.get("require_live_stages") is True and not result.get("repos")
                       and not result.get("slots")
                       and (stages == {"focus": None, "context": None} if arm is None
                            else stages == result.get("arm_stages") and isinstance(stages, dict)
                            and bool(stages.get("focus")) == (arm in ("F", "C"))
                            and bool(stages.get("context")) == (arm in ("A", "C"))),
                       "Windows Stage A stage settings or scenario scope differs")
        pin = manifest["tool_catalogs"]["answer" if arm else result["path"]]
        common.require(str(Path(result["catalog"]).resolve()) == pin["path"]
                       and catalog_hashes[result["catalog"]] == pin["sha256"],
                       f"Windows Stage A {path[0]} catalog differs from the Codex freeze")
    common.require(not remaining, "Windows Stage A is missing frozen result arms")


def prompt_changes(previous: dict, now: dict) -> list[str]:
    """The prompt, rubric, scenario and default-budget entries that differ between two manifests."""
    out = []
    for path in sorted(set(previous.get("agent_prompts") or {}) | set(now.get("agent_prompts") or {})):
        if ((previous.get("agent_prompts") or {}).get(path) or {}).get("sha256") != \
                ((now.get("agent_prompts") or {}).get(path) or {}).get("sha256"):
            out.append(f"agent prompt for {path}")
    for key, label in (("judge_prompt", "judge prompt"), ("judge_schema", "rubric (judge verdict schema)"),
                       ("scenario_freeze", "scenario freeze")):
        if (previous.get(key) or {}).get("sha256") != (now.get(key) or {}).get("sha256"):
            out.append(label)
    if (previous.get("answer_budget") or {}).get("default_budget") != (now.get("answer_budget") or {}).get(
            "default_budget"):
        out.append("answer default budget")
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
        common.require(previous.get("schema") == SCHEMA, "legacy refreeze requires a legacy manifest")
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
    # The prompts freeze is made against one scenario freeze (its SHA-256; the recorded path is where it was
    # read from and is not compared). On 22 November 2026 the loop freeze replaced the one-turn freeze: a
    # prompts freeze made against the earlier scenarios must be redone, not reused.
    if (frozen.get("scenario_freeze") or {}).get("sha256") != (now.get("scenario_freeze") or {}).get("sha256"):
        out.append("scenario freeze")
    # A judged pass sends the budget the freeze recorded: a moved default (or a freeze from before the budget
    # was pinned) is refused, not sent silently.
    if frozen.get("answer_budget") != now.get("answer_budget"):
        out.append("answer default budget")
    return out


def write(path: Path) -> dict:
    if path.is_file():
        common.require(common.read_json(path).get("schema") != CODEX_SCHEMA, "Codex freeze cannot be overwritten")
    record = {"schema": SCHEMA, "frozen_at": common.now_iso(), **current()}
    common.write_json(path, record)
    return record


def verify(path: Path) -> dict:
    frozen = common.read_json(path)
    if frozen.get("schema") == CODEX_SCHEMA:
        verify_codex(frozen)
    else:
        common.require(frozen.get("schema") == SCHEMA, f"{path} is not a prompts freeze")
        diff = differences(frozen, current())
        common.require(not diff, f"harness differs from the prompts freeze {path}: {diff}")
    return {"path": str(path), "sha256": common.sha256_file(path), "frozen_at": frozen.get("frozen_at")}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", type=Path)
    group.add_argument("--verify", type=Path)
    group.add_argument("--write-codex", type=Path, help="write a new Windows Stage A freeze")
    parser.add_argument("--codex-inputs", type=Path, help="Stage A purpose and preapproved input pins")
    group.add_argument("--refreeze", type=Path)
    parser.add_argument("--reason", default="")
    args = parser.parse_args(argv)
    common.require(bool(args.write_codex) == bool(args.codex_inputs),
                   "--write-codex and --codex-inputs must be supplied together")
    if args.write_codex:
        write_codex(args.write_codex, common.read_json(args.codex_inputs))
        print(json.dumps(verify(args.write_codex)))
    elif args.refreeze:
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
