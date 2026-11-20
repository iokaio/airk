# SPDX-License-Identifier: Apache-2.0
"""Check the prospective CAL100 R6 common clock against immutable R5 evidence. Never launches a process."""
import argparse
import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[6]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
R5 = CAMPAIGN / "cal100-focus1024-capture-r5"
PLAN = "55a89e5219cf6de0c3b59cbaa8a4a8a43dc0d097276368cfb5a28ee2fe6536bf"
OUTER = "23aedfee1e92ef826714f37b852301894f048f4ad3964e11eb75e65fd7b4e276"
TIMING = "d2bb6cba0e02213b8040409435ab2e05c229bf0f4a4e2092320be54b00873ab5"
DEFAULT_MS = 10_800_000
EXTENDED_MS = 11_700_000
# The common clock can change only after the actual user decision is retained and its exact hash reviewed.
APPROVED_R6_DECISION_SHA256 = None


def require(condition, message):
    if not condition:
        raise ValueError(message)


def original(path, sha):
    data = path.read_bytes()
    require(hashlib.sha256(data).hexdigest() == sha, f"original bytes differ: {path}")
    return json.loads(data)


def selected_clock(decision):
    if decision is None:
        return DEFAULT_MS
    require(APPROVED_R6_DECISION_SHA256 is not None, "no reviewed R6 common-clock change")
    row = original(decision, APPROVED_R6_DECISION_SHA256)
    require(row == {"attempt": "R6", "common_ms": EXTENDED_MS, "predecessor_plan": PLAN,
                    "scope": "whole-attempt-clock-only"}, "different R6 decision scope")
    return EXTENDED_MS


def common_deadline(start_ms, common_ms, debit_ms):
    require(type(start_ms) is int and start_ms >= 0 and type(common_ms) is int
            and common_ms in (DEFAULT_MS, EXTENDED_MS) and type(debit_ms) is int
            and debit_ms == 545_806, "unexpected campaign limit or original setup debit")
    return start_ms + common_ms - debit_ms


def inspect(decision=None):
    selected = selected_clock(decision)
    plan = original(R5 / "lifetime-plan.json", PLAN)
    end = original(R5 / "outer-owned.json", OUTER)
    timing = original(CAMPAIGN / "cal100-focus1024-timing-ab-r2-postclosure-analysis-r1.json", TIMING)
    require(end["exit_code"] == 1 and end["owned_descendants_settled"] is True
            and end["owned_job_forced_termination"] is False and end["owned_job_final_active_processes"] == 0
            and end["owned_job_final_member_pids"] == [], "incomplete R5 owner did not settle naturally")
    anchor = plan["installation"]["terminal"]
    require(Path(anchor["path"]) == CAMPAIGN / "cal100-focus1024-capture-r3/outer-owned.json",
            "different original R3 outer owner")
    debit = math.ceil(original(Path(anchor["path"]), anchor["sha256"])["elapsed_seconds"] * 1000)
    require(common_deadline(plan["start_ms"], DEFAULT_MS, debit) == plan["deadline_ms"]
            and debit == plan["setup_elapsed_ms"], "R5 source clock differs")
    groups = plan["phase_groups"]
    sizes = [len(x) for x in plan["phase_ids"]]
    ids = [question for phase in plan["phase_ids"] for question in phase]
    require(groups == [list(range(7)), list(range(7, 13))] and sizes == [8] * 12 + [4]
            and len(ids) == len(set(ids)) == 100, "original census changed")
    recorder = plan["recorder"]
    require((recorder["admission_limit"], recorder["record_limit"], recorder["byte_limit"]) == (1024, 8192, 64 << 20)
            and plan["capacity_basis"]["group_upper"] == [960, 920], "recorder capacity differs")
    sources = {}
    for row in plan["sources"]:
        path = Path(row["path"])
        require(path.resolve(strict=True) == path and path.is_relative_to(ROOT / "scripts"), "foreign historical source")
        data = path.read_bytes()
        require(len(data) == row["bytes"] and hashlib.sha256(data).hexdigest() == row["sha256"], "source changed: " + str(path))
        sources[str(path.relative_to(ROOT))] = row["sha256"]
    require(len(sources) == len(plan["sources"]) == 17, "expected all original source owners")
    deficit = timing["observed"]["missing_r5_work_to_final_admission_seconds"]
    require(timing["qualification"] is False and math.isfinite(deficit) and deficit > 0, "missing historical timing evidence")
    return {"attempt": "R6", "common_ms": selected, "original_setup_ms": debit,
            "net_common_ms": common_deadline(0, selected, debit), "original_lifetime_ticket_ms": DEFAULT_MS,
            "phase_ms": 1_800_000, "operation_ms": 120_000, "admission_and_delivery_ms": 125_000,
            "original_census": plan["census"], "phase_sizes": sizes, "phase_groups": groups, "sources": sources,
            "historical_deficit_seconds": deficit, "forecast_margin_seconds": (selected - DEFAULT_MS) / 1000 - deficit,
            "model_runs": 0, "physical_handoff_checked": False, "launch_authorized": False, "qualified": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--decision", type=Path)
    print(json.dumps(inspect(parser.parse_args().decision), sort_keys=True))
