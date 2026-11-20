# SPDX-License-Identifier: Apache-2.0
"""Check the settled native dispatch and charge journal for the CAL gathering stage."""

import hashlib
import json

KINDS = ("dispatch_admitted", "score_returned", "job_settled", "profile_released", "charge_released")
IDENTITY = ("observer_id", "profile_manifest", "scope", "task", "input_digest", "tokens",
            "question_utf8_sha256", "original_deadline_unix_ms", "charge_identity", "charge_bytes")
RELEASE = "actual_final_reservation_drop_after_native_scorer_and_source_teardown"


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def check_journal(raw, summary, focus_manifest, *, setup_only):
    require(summary["schema"] == "airk.private.native-dispatch-recorder/1" and summary["evaluation"] is True
            and summary["release_qualified"] is False and summary["complete"] is True and not summary["error"]
            and summary["dropped"] == summary["writer_failed"] == 0 and summary["bytes"] == len(raw) <= 64 << 20,
            "incomplete, dropped, failed or oversized dispatch observation")
    groups = {}
    last = count = 0
    for line in raw.splitlines():
        row = json.loads(line)
        require(row["schema"] == "airk.private.native-dispatch/1" and count < 8192 and
                type(row["sequence"]) is int and row["sequence"] == last + 1,
                "dispatch sequence or record bound differs")
        last = row["sequence"]
        count += 1
        require(row["task"] == "airk.focus_rank.v1" and row["profile_manifest"] == focus_manifest,
                "unexpected Context task or other scoring identity during CAL gathering")
        key = row["dispatch_identity"]
        group = groups.setdefault(key, [])
        require(len(groups) <= 1024 and len(group) < len(KINDS) and row["kind"] == KINDS[len(group)],
                "dispatch limit or ordered native lifecycle differs")
        if group:
            require(all(row[field] == group[0][field] for field in IDENTITY),
                    "native dispatch identity changed")
        group.append(row)
    require(count == summary["records"] and last == summary["last_sequence"], "native recorder totals differ")
    if setup_only:
        require(not groups, "setup-only fixture issued scoring work")
    else:
        require(groups, "CAL gathering observed no actual Focus dispatch")
    successes = failures = 0
    for group in groups.values():
        require(len(group) == len(KINDS), "unsettled native dispatch or unreleased charge")
        _, returned, settled, profile, charge = group
        require(profile["proof"] == charge["proof"] == RELEASE and settled["return_observed"] is True,
                "native scorer or final reservation drop was not observed")
        if returned["outcome"] == "success":
            require(returned["finite_raw_scores"] is True and returned["error"] is None,
                    "successful Focus return lacks finite scores")
            successes += 1
        else:
            require(returned["outcome"] == "error" and returned["error"], "unknown Focus return disposition")
            failures += 1
    return {"complete": True, "records": count, "focus_dispatches": len(groups),
            "successful_focus_returns": successes, "failed_focus_returns": failures,
            "context_dispatches": 0, "journal_sha256": hashlib.sha256(raw).hexdigest()}


def read_journal(work, focus_manifest, *, setup_only):
    summary_raw = (work / "composed-observer.closed.json").read_bytes()
    summary = json.loads(summary_raw)
    journal = work / "composed-observer.jsonl"
    require(journal.stat().st_size == summary["bytes"] <= 64 << 20, "dispatch journal size differs")
    result = check_journal(journal.read_bytes(), summary, focus_manifest, setup_only=setup_only)
    result["summary_sha256"] = hashlib.sha256(summary_raw).hexdigest()
    return result
