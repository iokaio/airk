# SPDX-License-Identifier: Apache-2.0
"""Verify the native development card capture and dispatch custody. No model runs."""

from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import re
import struct
import sys

ROOT = Path(__file__).resolve().parents[4]
MIB = 1 << 20
TASKS = {
    "airk.context_split.v1": "split",
    "airk.context_plan.v1": "plan",
    "airk.context_coverage.v1": "coverage",
    "airk.context_follow_up.v1": "follow_up",
}
INPUTS = {
    "capture": ("development-card-calls.jsonl", 64 * MIB),
    "closure": ("development-card-calls.closed.json", MIB),
    "ready": ("ready.json", 8 * MIB),
    "closed": ("closed.json", MIB),
    "journal": ("composed-observer.jsonl", 64 * MIB),
    "journal-closure": ("composed-observer.closed.json", MIB),
}
BINDING = (
    "observer_id", "dispatch_identity", "profile_manifest", "task", "input_digest",
    "question_utf8_sha256", "original_deadline_unix_ms", "tokens", "charge_identity", "charge_bytes", "scope",
)
RELEASE_PROOF = "actual_final_reservation_drop_after_native_scorer_and_source_teardown"
SETTLE_PROOF = "actual_job_compute_delivery_and_per_job_resident_permit_dropped"
KINDS = {"dispatch_admitted", "score_returned", "job_settled", "profile_released", "charge_released"}
STATUSES = {"accepted", "abstained", "unsupported", "invalid_input", "unavailable"}
REFUSALS = {
    "The loop card call's native outcome is unknown": "outcome_unknown",
    "The native lane refused this card input": "input_refused",
    "The native lane refused to queue this card call": "queue_refused",
    "The resident Context session was unavailable before dispatch": "resident_unavailable",
    "Native card scoring returned a known failure": "score_failed",
    "Native card scoring completed, but its score shape or finiteness was invalid": "scores_invalid",
    "Source authority or the Context configuration changed after native work": "late_scope_change",
    "A one-card roster has no runner-up": "no_runner_up",
    "The top card was tied.": "tied_top",
    "No calibration rule admits the top card": "no_calibration_rule",
}


class EvidenceError(ValueError):
    """The provided capture does not establish native score custody."""


def need(value, message):
    if not value:
        raise EvidenceError(message)


def object_pairs(pairs):
    result = dict(pairs)
    need(len(result) == len(pairs), "duplicate key in private JSON")
    return result


def invalid_constant(_):
    raise EvidenceError("non-JSON number in private evidence")


def decode(raw):
    result = json.loads(raw.decode("utf-8"), object_pairs_hook=object_pairs,
                        parse_constant=invalid_constant)
    need(isinstance(result, dict), "private evidence must be a JSON object")
    return result


def jsonl(raw, limit, line_limit):
    need(raw and raw.endswith(b"\n"), "private JSONL is empty or lacks final newline")
    lines = raw[:-1].split(b"\n", limit)
    need(len(lines) <= limit, "private JSONL row limit exceeded")
    for line in lines:
        need(0 < len(line) <= line_limit, "private JSONL row byte limit exceeded")
        yield decode(line)


def exact_int(value, low, high):
    return type(value) is int and low <= value <= high


def label(value):
    return isinstance(value, str) and 0 < len(value) <= 128


def sha(value):
    need(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None,
         "invalid SHA-256 identity")
    return value


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def check_header(row, schema):
    need(row.get("schema") == schema and row.get("evaluation") is True
         and row.get("release_qualified") is False, "private proof schema or qualification differs")


def check_owner(ready, closed, owner):
    check_header(ready, "airk.private-arbiter-fixture/1")
    check_header(closed, "airk.private-arbiter-fixture-closed/1")
    need(closed.get("shutdown_completed") is True and closed.get("service_lease_reacquired") is True,
         "missing successful native shutdown or original lease reacquisition")
    health = ready["health"]
    need(isinstance(owner, str) and re.fullmatch(r"S-1-[0-9-]+", owner) is not None
         and health["user"] == owner, "native service owner differs from expected Windows SID")
    need(label(health["instance"]) and health["instance"] == closed["instance"]
         and isinstance(ready["control_root"], str) and bool(ready["control_root"])
         and ready["control_root"] == closed["control_root"]
         and exact_int(health["process_id"], 1, 2**32 - 1), "native Ready and close identities differ")
    sha(health["executable_sha256"])
    catalog = health["catalog_clock"]["namespace"]
    need(label(catalog), "native Ready has no catalog identity")
    profile_ids = [sha(row["manifest"]) for row in ready["profiles"]]
    need(profile_ids and len(set(profile_ids)) == len(profile_ids), "native Ready profile roster is invalid")
    return catalog, set(profile_ids)


def scope_key(scope, catalog):
    need(isinstance(scope, dict) and scope.get("catalog") == catalog
         and exact_int(scope.get("configuration_revision"), 1, 2**64 - 1)
         and isinstance(scope.get("publication"), dict) and scope["publication"]
         and label(scope.get("index_id")), "native evidence scope is outside Ready or malformed")
    sha(scope["installation"])
    sha(scope["authorization"])
    return json.dumps(scope, sort_keys=True, separators=(",", ":"), allow_nan=False)


def score_bits(scores, bits, *, allow_nonfinite=False):
    need(isinstance(scores, list) and isinstance(bits, list) and len(scores) == len(bits),
         "scores and score-bit counts differ")
    for score, bit in zip(scores, bits, strict=True):
        need(exact_int(bit, 0, 2**32 - 1), "score bit pattern is not a u32")
        number = struct.unpack(">f", bit.to_bytes(4, "big"))[0]
        if math.isfinite(number):
            need(type(score) in (int, float) and math.isfinite(score)
                 and struct.pack(">f", score) == bit.to_bytes(4, "big"),
                 "score decimal differs from the exact native bits")
        else:
            need(allow_nonfinite and score is None, "nonfinite bits in retained native advice")
    return tuple(bits)


def timings(work):
    values = tuple(work[field] for field in ("queue_us", "compile_us", "inference_us"))
    need(all(exact_int(value, 0, 2**64 - 1) for value in values), "missing or invalid native scored work timing")
    return values


def score_key(profile, task, scope, input_digest, tokens, bits, work):
    return profile, task, scope, input_digest, tokens, bits, timings(work)


def journal_witness(journal, closure, catalog, profiles):
    check_header(closure, "airk.private.native-dispatch-recorder/1")
    need(closure.get("complete") is True and exact_int(closure.get("dropped"), 0, 0)
         and exact_int(closure.get("writer_failed"), 0, 0) and closure.get("error", False) is None
         and exact_int(closure.get("bytes"), len(journal), len(journal)),
         "native dispatch journal closure is incomplete or has loss")
    dispatches, observer_id, count = {}, None, 0
    for count, event in enumerate(jsonl(journal, 8192, 65536), 1):
        kind = event.get("kind")
        need(event.get("schema") == "airk.private.native-dispatch/1"
             and exact_int(event.get("sequence"), count, count) and kind in KINDS
             and exact_int(event.get("unix_ms"), 1, 2**64 - 1), "native dispatch journal event or sequence differs")
        binding = tuple(event[field] for field in BINDING)
        observer = event["observer_id"]
        observer_id = observer if observer_id is None else observer_id
        identity, profile, tokens = event["dispatch_identity"], sha(event["profile_manifest"]), event["tokens"]
        scope_key(event["scope"], catalog)
        sha(event["input_digest"])
        sha(event["question_utf8_sha256"])
        need(label(observer) and observer == observer_id and profile in profiles
             and exact_int(identity, 1, 2**64 - 1) and exact_int(tokens, 1, 8192)
             and exact_int(event["charge_bytes"], 1, 2**64 - 1)
             and exact_int(event["original_deadline_unix_ms"], 1, 2**64 - 1)
             and label(event["charge_identity"]), "native dispatch binding differs from Ready or its bounds")
        if identity not in dispatches:
            need(kind == "dispatch_admitted", "native dispatch has no earlier guarded admission")
            dispatches[identity] = binding, {}
        original, events = dispatches[identity]
        need(binding == original and kind not in events, "native dispatch binding drift or duplicate event")
        events[kind] = event
    need(exact_int(closure.get("records"), count, count)
         and exact_int(closure.get("last_sequence"), count, count), "native journal count differs from closure")

    successes, states = Counter(), Counter()
    for _, events in dispatches.values():
        need({"dispatch_admitted", "job_settled", "profile_released", "charge_released"} <= events.keys(),
             "native dispatch lacks actual job or charge settlement")
        scored = events.get("score_returned")
        settled = events["job_settled"]
        need(settled["proof"] == SETTLE_PROOF and settled["return_observed"] is (scored is not None)
             and all(events[kind]["proof"] == RELEASE_PROOF for kind in ("profile_released", "charge_released")),
             "native dispatch settlement or release proof differs")
        if scored is None:
            states["no_return"] += 1
            continue
        work = scored["work"]
        input_digest, tokens = scored["input_digest"], scored["tokens"]
        need(scored["return_observed"] is True and scored["sequence"] < settled["sequence"]
             and work["input_digest"] == input_digest and exact_int(work["input_tokens"], tokens, tokens),
             "returned score differs from guarded compiled dispatch")
        if scored["outcome"] == "error":
            need(scored["raw_score_bits"] is None and scored["raw_scores"] is None
                 and isinstance(scored["error"], str) and bool(scored["error"]),
                 "failed native return contains scores or lacks an error")
            states["error"] += 1
            continue
        need(scored["outcome"] == "success", "unknown native return outcome")
        bits = score_bits(scored["raw_scores"], scored["raw_score_bits"], allow_nonfinite=True)
        finite = all(value is not None for value in scored["raw_scores"])
        need(0 < len(bits) <= 64 and scored["finite_raw_scores"] is finite, "native dispatch score shape differs")
        if scored["task"] in TASKS:
            states["card_success" if finite else "card_nonfinite"] += 1
            if finite:
                key = score_key(scored["profile_manifest"], scored["task"], scope_key(scored["scope"], catalog),
                                input_digest, tokens, bits, work)
                successes[key] += 1
        else:
            states["noncard_success"] += 1
    return successes, states


def refusal(outcome):
    status = outcome["status"]
    need(status in STATUSES, "unknown native card outcome status")
    if status == "accepted":
        need(isinstance(outcome["decision"], dict), "accepted native advice lacks a decision")
        return "accepted"
    reason = outcome["reason"]
    need(isinstance(reason, str) and 0 < len(reason.encode("utf-8")) <= 1024, "native refusal reason is invalid")
    return next((kind for prefix, kind in REFUSALS.items() if reason.startswith(prefix)), "other_" + status)


def capture_counts(capture, closure, catalog, profiles, witnesses):
    check_header(closure, "airk.private.development-card-capture-closure/1")
    need(closure.get("complete") is True and closure.get("service_completed") is True
         and exact_int(closure.get("dropped"), 0, 0) and closure.get("writer_failed") is False
         and exact_int(closure.get("bytes"), len(capture), len(capture)) and closure.get("sha256") == digest(capture),
         "development card closure is incomplete, lossy or does not bind the capture")
    seen, all_tasks, all_profiles, count = set(), {}, {}, 0
    for count, row in enumerate(jsonl(capture, 512, 512 << 10), 1):
        mode = row["decision_mode"]
        need(row.get("schema") == "airk.private.development-card-call/1"
             and mode in ("calibrated", "top_score") and label(row["request_id"])
             and exact_int(row["max_sub_needs"], 1, 255), "development card row contract differs")
        advice, segments = row["advice"], row["segments"]
        task, slot = advice["task"], advice["card_slot"]
        profile = sha(advice["profile"])
        need(advice.get("schema") == "airk.arbiter.advice/3" and task in TASKS and profile in profiles
             and exact_int(slot, 1, 255), "card advice owner, schema, task or slot differs")
        identity = row["request_id"], slot
        need(identity not in seen, "duplicate native read attempt and card slot")
        seen.add(identity)
        scope = scope_key(advice["scope"], catalog)
        sha(advice["input_digest"])
        roster, bits, outcomes = row["roster_digests"], row["raw_score_bits"], advice["segments"]
        need(all(isinstance(value, list) for value in (segments, roster, bits, outcomes))
             and len(segments) == len(roster) == len(bits) == len(outcomes) > 0,
             "private roster, card advice and score segment counts differ")
        ordinals, flat_bits, status_counts, refusals = set(), [], Counter(), Counter()
        for segment, roster_hash, segment_bits, outcome in zip(segments, roster, bits, outcomes, strict=True):
            sha(roster_hash)
            ordinal = segment["ordinal"]
            need(exact_int(ordinal, 1, 255) and ordinal not in ordinals
                 and exact_int(outcome["ordinal"], ordinal, ordinal) and exact_int(segment["card_slot"], slot, slot)
                 and segment["stage"] == TASKS[task], "ordered roster and advice segment identity differ")
            ordinals.add(ordinal)
            cards = segment["cards"]
            need(isinstance(cards, list) and 0 < len(cards) <= 8, "invalid native roster shape")
            symbols = [card["symbol"] for card in cards]
            need(all(label(symbol) for symbol in symbols) and len(set(symbols)) == len(symbols),
                 "invalid native roster symbols")
            exact_bits = score_bits(outcome["raw_scores"], segment_bits)
            need(not exact_bits or len(cards) == len(exact_bits), "card count differs from retained native score count")
            flat_bits.extend(exact_bits)
            status = outcome["outcome"]["status"]
            reason = refusal(outcome["outcome"])
            status_counts[status] += 1
            if status != "accepted":
                refusals[reason] += 1
        work = advice["work"]
        evidence = row["score_evidence"]
        compiled, scored = evidence["compiled_input_digest"], evidence["scored_input_digest"]
        if compiled is not None:
            sha(compiled)
        if scored is None:
            need(not flat_bits and not status_counts["accepted"], "claimed advice score lacks a native scored digest")
        else:
            sha(scored)
            need(compiled is None or compiled == scored, "captured compiler and score digests differ")
            tokens = advice["observed_input_tokens"]
            need(exact_int(tokens, 1, 8192) and exact_int(work["input_tokens"], tokens, tokens)
                 and len(flat_bits) == sum(len(segment["cards"]) for segment in segments)
                 and flat_bits, "scored capture lacks exact native score bits or token work")
            key = score_key(profile, task, scope, scored, tokens, tuple(flat_bits), work)
            need(witnesses[key] > 0, "scored capture has no unused exact native execution match")
            witnesses[key] -= 1
        for bucket in (all_tasks.setdefault(task, Counter()), all_profiles.setdefault(profile, Counter())):
            bucket["calls"] += 1
            bucket["segments"] += len(segments)
            bucket["mode_" + mode] += 1
            bucket["scored_witnessed" if scored is not None else "without_score"] += 1
            if scored is not None and compiled is None:
                bucket["compiled_callback_unobserved"] += 1
            bucket.update({"outcome_" + key: n for key, n in status_counts.items()})
            bucket.update({"refusal_" + key: n for key, n in refusals.items()})
    need(exact_int(closure.get("records"), count, count), "development card row count differs from closure")
    return count, all_tasks, all_profiles


def verify(data, owner):
    ready, closed = decode(data["ready"]), decode(data["closed"])
    catalog, profiles = check_owner(ready, closed, owner)
    witnesses, journal_counts = journal_witness(data["journal"], decode(data["journal-closure"]), catalog, profiles)
    records, tasks, by_profile = capture_counts(
        data["capture"], decode(data["closure"]), catalog, profiles, witnesses
    )
    return {
        "schema": "airk.private.development-card-capture-verification/1",
        "records": records,
        "native_score_journal_counts": journal_counts,
        "unmatched_native_card_successes": sum(witnesses.values()),
        "tasks": tasks,
        "profiles": by_profile,
        "input_sha256": {name: digest(raw) for name, raw in data.items()},
    }


def read_pinned(work, name, size, expected):
    path = work / name
    need(path.resolve(strict=True) == path and path.is_file(), "private evidence must be a direct work-directory file")
    with path.open("rb") as stream:
        raw = stream.read(size + 1)
    need(len(raw) <= size and digest(raw) == sha(expected), "private evidence size or SHA-256 differs")
    return raw


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", required=True, type=Path)
    parser.add_argument("--owner-sid", required=True)
    for key in INPUTS:
        parser.add_argument(f"--{key}-sha256", required=True)
    args = parser.parse_args(argv)
    try:
        work = args.work.resolve(strict=True)
        need(work.is_dir() and work.is_relative_to((ROOT / "artifacts").resolve()),
             "private work directory must be under the checkout artifacts directory")
        data = {
            key: read_pinned(work, name, size, getattr(args, key.replace("-", "_") + "_sha256"))
            for key, (name, size) in INPUTS.items()
        }
        print(json.dumps(verify(data, args.owner_sid), sort_keys=True, indent=2))
    except EvidenceError as error:
        print(f"verify_card_capture: {error}", file=sys.stderr)
        return 1
    except (OSError, ValueError, TypeError, KeyError, AttributeError, OverflowError, RecursionError, struct.error):
        print("verify_card_capture: malformed or unreadable private evidence", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
