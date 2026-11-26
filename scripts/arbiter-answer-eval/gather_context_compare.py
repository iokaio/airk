# SPDX-License-Identifier: Apache-2.0
"""Compare recorded Gather Context deliveries against independently frozen witnesses."""

from __future__ import annotations

import math
import re


REFERENCE_SCHEMA = "airk.private.gather-context-reference/1"
CAPTURE_SCHEMA = "airk.private.gather-context-capture/1"
COMPARISON_SCHEMA = "airk.private.gather-context-comparison/1"
ARMS = ("control", "candidate")
AUTHORS = ("openai", "anthropic")
DISPOSITIONS = {"completed", "partial", "cancelled", "timeout", "refused"}
_SHA256 = re.compile(r"[0-9a-f]{64}")
_ACTION_FIELDS = ("round", "need", "parent", "parent_action", "parent_citation", "citation",
                  "selector", "grounded_selector", "operation", "requested_read", "reason",
                  "outcome", "stop")


def _require(predicate: bool, reason: str) -> None:
    if not predicate:
        raise ValueError(reason)


def _rows(document: dict, label: str) -> dict[str, dict]:
    rows = document.get("cases")
    _require(isinstance(rows, list) and bool(rows), f"{label} requires a nonempty case roster")
    indexed = {}
    for row in rows:
        case_id = row.get("id") if isinstance(row, dict) else None
        _require(isinstance(case_id, str) and bool(case_id) and case_id not in indexed,
                 f"{label} has a missing or duplicate case id")
        indexed[case_id] = row
    return indexed


def _witness(row: dict, label: str) -> tuple:
    _require(isinstance(row, dict), f"{label} is not a witness object")
    path, start, end = (row.get(field) for field in ("path", "start_line", "end_line"))
    source, excerpt = (row.get(field) for field in ("source_sha256", "excerpt_sha256"))
    _require(isinstance(path, str) and bool(path) and "\\" not in path and ":" not in path
             and not ({"", ".", ".."} & set(path.split("/")))
             and type(start) is int and type(end) is int and 1 <= start <= end
             and isinstance(source, str) and _SHA256.fullmatch(source) is not None
             and isinstance(excerpt, str) and _SHA256.fullmatch(excerpt) is not None,
             f"{label} contains an invalid source path, range or digest")
    return path, start, end, source, excerpt


def _costs(row: dict, label: str) -> dict:
    costs = row.get("costs")
    _require(isinstance(costs, dict), f"{label} has no costs object")
    for value in costs.values():
        _require(value is None or (type(value) in (int, float) and 0 <= value
                                  and (type(value) is int or math.isfinite(value))),
                 f"{label} contains an invalid cost")
    return costs


def _arm_result(case: dict, row: dict, arm: str, expected: dict) -> dict:
    label = f"{arm}/{case['id']}"
    _require(all(row.get(key) == case[key] for key in ("repo", "publication", "index_id")),
             f"{label} scope or publication differs")
    _require(row.get("source_commit", case["source_commit"]) == case["source_commit"],
             f"{label} source commit differs")
    disposition = row.get("disposition")
    _require(isinstance(disposition, str) and disposition in DISPOSITIONS,
             f"{label} has an invalid disposition")
    delivered, actions = row.get("delivered_passages"), row.get("actions")
    _require(isinstance(delivered, list) and isinstance(actions, list)
             and all(isinstance(action, dict) for action in actions),
             f"{label} has no explicit delivered passages or action trace")

    witnesses = {_witness(passage, f"{label} delivered passage") for passage in delivered}
    covered = [note_id for note_id, witness in expected.items() if witness in witnesses]
    missing = [note_id for note_id in expected if note_id not in covered]
    return {
        "disposition": disposition,
        "verified_exact_witnesses": covered,
        "unverified_obligations": missing,
        "delivered_passages": delivered,
        "actions": actions,
        "costs": _costs(row, label),
    }


def _divergence(control: dict, candidate: dict) -> dict | None:
    left, right = control["actions"], candidate["actions"]
    for index, (before, after) in enumerate(zip(left, right), 1):
        comparable_fields = [field for field in _ACTION_FIELDS if field in before and field in after]
        changed = [field for field in comparable_fields if before[field] != after[field]]
        if changed:
            return {"kind": "recorded_action", "position": index, "changed_fields": changed,
                    "control": {field: before[field] for field in changed},
                    "candidate": {field: after[field] for field in changed}}

    before, after = (arm["verified_exact_witnesses"] for arm in (control, candidate))
    if before != after:
        return {"kind": "delivered_witness_difference", "position": None,
                "control": before, "candidate": after}
    if control["disposition"] != candidate["disposition"]:
        return {"kind": "disposition_difference", "position": None,
                "control": control["disposition"], "candidate": candidate["disposition"]}
    return None


def compare(reference: dict, control: dict, candidate: dict) -> dict:
    """Account for every frozen case and compare exactly matching source witnesses."""
    _require(reference.get("schema") == REFERENCE_SCHEMA, "unknown reference schema")
    spec_sha = reference.get("spec_sha256")
    _require(isinstance(spec_sha, str) and _SHA256.fullmatch(spec_sha) is not None,
             "reference is missing the spec identity")
    frozen = _rows(reference, "reference")
    arms = {}
    for arm, capture in zip(ARMS, (control, candidate)):
        _require(capture.get("schema") == CAPTURE_SCHEMA and capture.get("arm") == arm,
                 f"{arm} capture schema or arm differs")
        _require(capture.get("spec_sha256") == spec_sha, f"{arm} capture spec differs")
        arms[arm] = _rows(capture, arm)
        _require(arms[arm].keys() == frozen.keys(), f"{arm} case roster differs")

    rows = []
    for case_id, case in frozen.items():
        author_paths = case.get("author_paths")
        _require(isinstance(author_paths, dict) and set(author_paths) == set(AUTHORS),
                 f"{case_id} requires both author's traces")
        obligations = case.get("obligations")
        _require(isinstance(obligations, list) and bool(obligations),
                 f"{case_id} requires independently frozen obligations")
        expected, alternatives = {}, {}
        for obligation in obligations:
            note_id = obligation["id"]
            _require(isinstance(note_id, str) and bool(note_id) and note_id not in expected,
                     f"{case_id} has an invalid or duplicate obligation")
            expected[note_id] = _witness(obligation["witness"], f"{case_id}/{note_id}")
            locations = obligation.get("alternate_locations_for_review", [])
            if locations:
                alternatives[note_id] = locations

        arm_rows = {arm: _arm_result(case, arms[arm][case_id], arm, expected) for arm in ARMS}
        costs = {}
        for name in sorted(set().union(*(arm_rows[arm]["costs"] for arm in ARMS))):
            before, after = (arm_rows[arm]["costs"].get(name) for arm in ARMS)
            costs[name] = {"control": before, "candidate": after,
                           "delta": after - before if before is not None and after is not None else None}
        rows.append({
            "id": case_id,
            "repo": case["repo"],
            "slot": case["slot"],
            "source_commit": case["source_commit"],
            "publication": case["publication"],
            "index_id": case["index_id"],
            "author_paths": {author: {
                "step_count": len(author_paths[author]["steps"]),
                "handoff": author_paths[author]["handoff"],
                "stop": author_paths[author]["stop"],
                "proof_limits": author_paths[author]["proof_limits"],
            } for author in AUTHORS},
            "obligations": obligations,
            "alternate_locations_for_review": alternatives,
            "arms": arm_rows,
            "cost_comparison": costs,
            "first_observed_divergence": _divergence(arm_rows["control"], arm_rows["candidate"]),
        })
    return {"schema": COMPARISON_SCHEMA, "spec_sha256": spec_sha,
            "coverage_basis": "exact_delivered_source_witness", "cases": rows}


def validate_judgments(comparison: dict, judged: dict) -> dict[tuple[str, str], dict]:
    """Require an independently supplied disposition for every case and arm."""
    cases = _rows(comparison, "comparison")
    _require(judged.get("spec_sha256") == comparison.get("spec_sha256"),
             "judgment spec differs from the comparison")
    rows = judged.get("cases")
    _require(isinstance(rows, list), "judgments require a case and arm roster")
    expected = {(case_id, arm) for case_id in cases for arm in ARMS}
    outcomes = {}
    for row in rows:
        _require(isinstance(row, dict), "invalid judgment row")
        case_id, arm, status = (row.get(field) for field in ("id", "arm", "status"))
        finish, turns = row.get("correct_finish"), row.get("external_turns")
        _require(isinstance(case_id, str) and isinstance(arm, str) and (case_id, arm) in expected
                 and (case_id, arm) not in outcomes and isinstance(status, str) and bool(status)
                 and type(finish) is bool and (turns is None or type(turns) is int and turns >= 0)
                 and (not finish or turns is not None),
                 "invalid, duplicate or unexpected judgment")
        outcomes[case_id, arm] = row
    _require(outcomes.keys() == expected, "judgment case and arm roster differs")
    return outcomes


def _cell(value: object) -> str:
    return " ".join(str(value).split()).replace("|", r"\|")


def report(comparison: dict, *, judged: dict | None = None) -> str:
    """Render a private development report with explicit limits on its claims."""
    _require(comparison.get("schema") == COMPARISON_SCHEMA, "unknown comparison schema")
    cases = _rows(comparison, "comparison")
    outcomes = validate_judgments(comparison, judged) if judged is not None else None
    lines = [
        "# Gather Context development comparison", "",
        ("Independent judgments are present for both arms."
         if outcomes is not None else "Unjudged diagnostic. Correct finishes and product readiness are unassessed."),
        "",
        "Coverage counts exact source witnesses delivered to the caller. Alternate or broader ranges require source review.",
        "Native measurements keep their capture field names and units. Hosted external turns appear only in judgments.",
        "",
        "| Case | OpenAI steps | Anthropic turns | Control witnesses | Candidate witnesses | First observed difference |",
        "| --- | ---: | ---: | --- | --- | --- |",
    ]
    for case_id, case in cases.items():
        paths, arms = case["author_paths"], case["arms"]
        total = len(case["obligations"])
        cells = [case_id, paths["openai"]["step_count"], paths["anthropic"]["step_count"]]
        for arm in ARMS:
            row = arms[arm]
            cells.append(f'{len(row["verified_exact_witnesses"])}/{total} ({row["disposition"]})')
        difference = case["first_observed_divergence"]
        cells.append(difference["kind"].replace("_", " ") if difference else "Unavailable")
        lines.append("| " + " | ".join(_cell(cell) for cell in cells) + " |")

    for case_id, case in cases.items():
        lines.extend(["", f"## {case_id}", ""])
        for author in AUTHORS:
            trace = case["author_paths"][author]
            qualification = trace["proof_limits"].get("closing_qualification")
            lines.append(f"- {author.capitalize()} proof limit. {_cell(qualification or 'Unavailable')}")
        if case["alternate_locations_for_review"]:
            notes = ", ".join(case["alternate_locations_for_review"])
            lines.append(f"- Annotated alternatives awaiting source review. {_cell(notes)}.")
        for arm in ARMS:
            unresolved = case["arms"][arm]["unverified_obligations"]
            if unresolved:
                lines.append(f"- {arm.capitalize()} obligations without an exact delivered witness. "
                             f"{_cell(', '.join(unresolved))}.")
        for name, cost in case["cost_comparison"].items():
            before, after = (cost[arm] if cost[arm] is not None else "unavailable" for arm in ARMS)
            lines.append(f"- {_cell(name)}. Control {before}, candidate {after}.")
        if outcomes is not None:
            for arm in ARMS:
                outcome = outcomes[case_id, arm]
                result = "correct finish" if outcome["correct_finish"] else "no correct finish"
                turns = outcome["external_turns"]
                timing = f"{turns} external turns" if turns is not None else "external turns unavailable"
                lines.append(f"- {arm.capitalize()} judgment. {_cell(outcome['status'])}, {result}, {timing}.")
                for field, label in (("missing_facts", "Missing facts"),
                                     ("misleading_facts", "Misleading facts"), ("citations", "Citations")):
                    if field in outcome:
                        lines.append(f"  {_cell(label)}. {_cell(outcome[field])}.")
    return "\n".join(lines) + "\n"
