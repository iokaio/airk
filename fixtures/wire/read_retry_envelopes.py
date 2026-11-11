# SPDX-License-Identifier: Apache-2.0
"""Generate the top-level read-retry envelope cases (split contract C8, docs/split/contracts.md §10).

The rule under test is "may the host repeat this exact read, and after how many
milliseconds?" for a single (non-combined) MCP tool result. The consumers are
the hand-written TypeScript `readRetryDelay` (packages/vscode/src/readRetry.ts)
and the native test client `classify` / `classify_for_scope`
(crates/airk-cli/tests/common/read_retry.rs). Combined reads are covered by
read_retry_decisions.py and are not repeated here.

Every `expected` value is a reviewed statement of the rule, written
independently of those implementations; it is never computed by them.
`expected` is null (no retry) or the delay and whether the retry needs a fresh
brokerage. Stated rule:

  * The envelope has `isError: true` and exactly one text content block whose
    text parses as JSON with integer-only number literals. If
    `structuredContent` is present it equals that parsed text.
  * Storage busy (`status: "retry"`): `code` is `publication_busy` or
    `selection_busy`, `retryable` is true and `action` is
    `repeat_same_request`; no fresh brokerage.
  * Direct refusal (`error.code` `direct_query_admission` or
    `direct_query_setup`): `reason` is `busy`, `query_submitted` is false,
    `retryable` is true, and `fresh_brokerage_required` is true exactly for
    `direct_query_setup`.
  * Scoped setup refusal (`error.code` `scoped_status_prepare_timeout` or
    `scoped_connection_unavailable`) is retryable only for a caller that
    supplies its original frozen selection (`expected_scope`), with
    `structuredContent` present, only the fields `error` and
    `requested_scope`, closed error fields, `query_submitted` false,
    `retryable` true, `fresh_brokerage_required` true, `query_replay_safe`
    absent or true, `requested_scope.confirmed` false, and a selection equal to
    the original except that `expected_catalog_revision` may only move forward
    (project order is not significant). `scoped_connection_unavailable`
    requires `native_control_response` = `{status: "failure", code}` with one
    of the nine pre-query busy codes below; the timeout code carries none.
  * The delay is a JSON integer in 1..=1000.

Cases with an `emitter` name the production Rust function whose output must
equal `envelope`: the `text` byte for byte and the parsed envelope. Object keys
of a `serde_json::Value` serialize sorted; `ReadRetryHint` is a struct and
keeps its declaration order (status, retryable, code, retry_after_ms, action).

    python fixtures/wire/read_retry_envelopes.py          # rewrite the JSON
    python fixtures/wire/read_retry_envelopes.py --check  # exit 1 on drift
"""
import argparse
import copy
import json
from pathlib import Path
import sys

OUTPUT = Path(__file__).with_name("read_retry_envelopes.json")
SCHEMA = "airk.wire-fixture.read-retry-envelopes/1"

NATIVE_BUSY_CODES = [
    "broker_busy", "project_starting_or_unavailable", "project_unavailable_or_busy", "previous_workers_draining",
    "catalog_revision_changed", "catalog_clock_pending", "catalog_connection_busy", "project_capacity_busy",
    "project_restart_backoff",
]
PROJECT = "prj_" + "05" * 16
SECOND_PROJECT = "prj_" + "06" * 16
SELECTION = {"schema": 1, "context_key": "01" * 32, "namespace": "02" * 16, "expected_catalog_revision": 7,
             "grant_id": "grt_" + "03" * 16, "expected_grant_revision": 1, "index_id": "idx_" + "04" * 16,
             "project_ids": [PROJECT]}


def sorted_text(value):
    return json.dumps(value, separators=(",", ":"), sort_keys=True, ensure_ascii=False)


def ordered_text(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False)


def envelope(body, structured=False, text=None):
    value = {"isError": True, "content": [{"type": "text", "text": text if text is not None else sorted_text(body)}]}
    if structured:
        value["structuredContent"] = body
    return value


def accepted(delay, fresh):
    return {"retry_after_ms": delay, "fresh_brokerage_required": fresh}


def case(name, rule, value, expected, expected_scope=None, emitter=None):
    entry = {"name": name, "rule": rule, "envelope": value, "expected_scope": expected_scope, "expected": expected}
    if emitter is not None:
        entry["emitter"] = emitter
    return entry


# Storage busy hints. The MCP emitter writes the ReadRetryHint struct in field order.
def storage(**changes):
    hint = {"status": "retry", "retryable": True, "code": "selection_busy", "retry_after_ms": 25,
            "action": "repeat_same_request"}
    hint.update(changes)
    return hint


def storage_cases():
    for code, error in [("selection_busy", "selection_contention"), ("publication_busy", "current_read_contention")]:
        hint = storage(code=code)
        yield case(f"storage_{code}", f"{code} is a storage busy code; MCP writes the hint struct in field order",
                   envelope(hint, text=ordered_text(hint)), accepted(25, False),
                   emitter={"function": "airk_mcp::response::typed_error_result", "error": error})
    yield case("storage_structured_equal", "equal structured content is accepted",
               envelope(storage(), structured=True), accepted(25, False))
    disagreeing = envelope(storage(), structured=True)
    disagreeing["structuredContent"] = storage(retry_after_ms=30)
    yield case("storage_structured_disagrees", "structured content must equal the text", disagreeing, None)
    yield case("storage_wrong_action", "action must be repeat_same_request",
               envelope(storage(action="reconnect")), None)
    yield case("storage_not_retryable", "retryable must be true", envelope(storage(retryable=False)), None)
    yield case("storage_other_code", "only publication_busy and selection_busy are storage busy codes",
               envelope(storage(code="index_busy")), None)
    for delay, expected in [(0, None), (1, accepted(1, False)), (1000, accepted(1000, False)), (1001, None)]:
        yield case(f"storage_delay_{delay}", "delay must be 1..=1000 ms", envelope(storage(retry_after_ms=delay)), expected)
    yield case("storage_delay_missing", "a delay is required", envelope({k: v for k, v in storage().items() if k != "retry_after_ms"}), None)
    yield case("storage_delay_string", "the delay is a JSON number", envelope(storage(retry_after_ms="25")), None)
    yield case("storage_integral_float_delay", "the delay is a JSON integer literal; 25.0 is refused",
               envelope(storage(retry_after_ms=25.0)), None)
    not_error = envelope(storage())
    not_error["isError"] = False
    yield case("storage_not_error", "only an error result can ask for a retry", not_error, None)
    doubled = envelope(storage())
    doubled["content"].append({"type": "text", "text": "{}"})
    yield case("storage_two_content_blocks", "exactly one text block", doubled, None)
    yield case("text_not_json", "the text must parse as JSON", envelope(None, text="selection_busy"), None)


def direct(code="direct_query_admission", fresh=None, delay=25, **changes):
    error = {"code": code, "reason": "busy", "query_submitted": False, "retryable": True, "retry_after_ms": delay,
             "fresh_brokerage_required": (code == "direct_query_setup") if fresh is None else fresh}
    error.update(changes)
    return {"error": error}


def direct_cases():
    yield case("direct_admission_busy", "unsubmitted busy admission keeps the brokerage",
               envelope(direct()), accepted(25, False))
    yield case("direct_setup_busy", "busy setup requires fresh brokerage",
               envelope(direct("direct_query_setup", delay=100)), accepted(100, True))
    yield case("direct_setup_without_fresh_brokerage", "setup must declare fresh brokerage",
               envelope(direct("direct_query_setup", fresh=False, delay=100)), None)
    yield case("direct_admission_with_fresh_brokerage", "admission must not declare fresh brokerage",
               envelope(direct(fresh=True)), None)
    yield case("direct_admission_fresh_missing", "fresh_brokerage_required is required",
               envelope({"error": {k: v for k, v in direct()["error"].items() if k != "fresh_brokerage_required"}}), None)
    yield case("direct_admission_draining", "a draining refusal is not retried",
               envelope(direct(reason="draining", retryable=False, retry_after_ms=None, fresh=True)), None)
    yield case("direct_admission_other_reason", "reason must be busy even when retryable",
               envelope(direct(reason="canceled")), None)
    yield case("direct_submitted", "a submitted query is never replayed", envelope(direct(query_submitted=True)), None)
    yield case("direct_submitted_unknown", "an unknown submission is never replayed",
               envelope(direct(query_submitted=None)), None)
    yield case("direct_with_message", "the advisory message of the MCP request queue does not change the decision",
               envelope(direct(message="AIRK request queue is full or timed out; retry after outstanding requests complete")),
               accepted(25, False))
    yield case("direct_other_code", "only the two direct codes are retryable", envelope(direct("request_draining")), None)
    for delay, expected in [(0, None), (1, accepted(1, False)), (1000, accepted(1000, False)), (1001, None)]:
        yield case(f"direct_delay_{delay}", "delay must be 1..=1000 ms", envelope(direct(delay=delay)), expected)
    yield case("direct_integral_float_delay", "the delay is a JSON integer literal; 25.0 is refused",
               envelope(direct(delay=25.0)), None)


def scoped(code="scoped_status_prepare_timeout", native=None, delay=100, selection=None, **changes):
    error = {"code": code, "retryable": True, "query_submitted": False, "retry_after_ms": delay,
             "fresh_brokerage_required": True}
    if native is not None:
        error["native_control_response"] = native
    error.update(changes)
    return {"error": error, "requested_scope": {"confirmed": False, "selection": selection or SELECTION}}


def selection(**changes):
    value = copy.deepcopy(SELECTION)
    value.update(changes)
    return value


def native(code):
    return {"status": "failure", "code": code}


def scoped_cases():
    scope = SELECTION

    def emitter(code, retryable=True, native_response=None):
        return {"function": "airk_mcp::remote::scoped_setup_failure", "code": code, "retryable": retryable,
                "native_control_response": native_response}

    yield case("scoped_prepare_timeout", "a prepare timeout before submission is retried with fresh brokerage",
               envelope(scoped(), structured=True), accepted(100, True), scope, emitter("scoped_status_prepare_timeout"))
    for code in NATIVE_BUSY_CODES:
        yield case(f"scoped_unavailable_{code}", f"{code} is a pre-query native busy refusal",
                   envelope(scoped("scoped_connection_unavailable", native(code)), structured=True), accepted(100, True),
                   scope, emitter("scoped_connection_unavailable", native_response=native(code)))
    yield case("scoped_unavailable_grant_revoked", "a tenth native code is not a busy refusal",
               envelope(scoped("scoped_connection_unavailable", native("grant_revoked")), structured=True), None, scope,
               emitter("scoped_connection_unavailable", native_response=native("grant_revoked")))
    yield case("scoped_unavailable_without_native_response", "the unavailable code needs its native response",
               envelope(scoped("scoped_connection_unavailable"), structured=True), None, scope,
               emitter("scoped_connection_unavailable"))
    yield case("scoped_unavailable_native_not_failure", "the native response is a failure",
               envelope(scoped("scoped_connection_unavailable", {"status": "ok", "code": "broker_busy"}), structured=True),
               None, scope)
    yield case("scoped_unavailable_native_extra_field", "the native response has only status and code",
               envelope(scoped("scoped_connection_unavailable", dict(native("broker_busy"), detail="x")), structured=True),
               None, scope)
    yield case("scoped_timeout_with_native_response", "the timeout code carries no native response",
               envelope(scoped(native=native("broker_busy")), structured=True), None, scope,
               emitter("scoped_status_prepare_timeout", native_response=native("broker_busy")))
    yield case("scoped_not_retryable", "a non-retryable setup failure is final",
               envelope({"error": {"code": "scoped_status_prepare_unavailable", "retryable": False, "query_submitted": False},
                         "requested_scope": {"confirmed": False, "selection": SELECTION}}, structured=True), None, scope,
               emitter("scoped_status_prepare_unavailable", retryable=False))
    yield case("scoped_without_expected_scope", "a caller without its original selection cannot retry",
               envelope(scoped(), structured=True), None, None)
    yield case("scoped_without_structured_content", "the scoped envelope carries equal structured content",
               envelope(scoped()), None, scope)
    disagreeing = envelope(scoped(), structured=True)
    disagreeing["structuredContent"] = scoped(delay=200)
    yield case("scoped_structured_disagrees", "structured content must equal the text", disagreeing, None, scope)
    yield case("scoped_clock_backwards", "the catalog clock never moves backwards",
               envelope(scoped(selection=selection(expected_catalog_revision=6)), structured=True), None, scope)
    yield case("scoped_clock_forward", "native admission may refresh the catalog clock forward",
               envelope(scoped(selection=selection(expected_catalog_revision=12)), structured=True), accepted(100, True), scope)
    yield case("scoped_changed_grant", "a different grant is a different authority",
               envelope(scoped(selection=selection(grant_id="grt_" + "09" * 16)), structured=True), None, scope)
    yield case("scoped_changed_grant_revision", "a changed grant revision is a different authority",
               envelope(scoped(selection=selection(expected_grant_revision=2)), structured=True), None, scope)
    yield case("scoped_changed_index", "a different index is a different read",
               envelope(scoped(selection=selection(index_id="idx_" + "09" * 16)), structured=True), None, scope)
    two = selection(project_ids=[PROJECT, SECOND_PROJECT])
    yield case("scoped_project_order", "project order is not significant",
               envelope(scoped(selection=selection(project_ids=[SECOND_PROJECT, PROJECT])), structured=True),
               accepted(100, True), two)
    yield case("scoped_selection_unknown_field", "the selection has a closed field set",
               envelope(scoped(selection=selection(note="x")), structured=True), None, scope)
    confirmed = scoped()
    confirmed["requested_scope"]["confirmed"] = True
    yield case("scoped_confirmed", "a confirmed scope is not a pre-query refusal", envelope(confirmed, structured=True), None, scope)
    yield case("scoped_submitted", "a submitted query is never replayed",
               envelope(scoped(query_submitted=True), structured=True), None, scope)
    yield case("scoped_replay_safe_true", "query_replay_safe may be stated as true",
               envelope(scoped(query_replay_safe=True), structured=True), accepted(100, True), scope)
    yield case("scoped_replay_unsafe", "query_replay_safe false is never replayed",
               envelope(scoped(query_replay_safe=False), structured=True), None, scope)
    yield case("scoped_unknown_error_field", "the error has a closed field set",
               envelope(scoped(unexpected=True), structured=True), None, scope)
    extra = scoped()
    extra["hits"] = []
    yield case("scoped_extra_body_field", "the body has only error and requested_scope", envelope(extra, structured=True), None, scope)
    yield case("scoped_without_fresh_brokerage", "a scoped retry always needs fresh brokerage",
               envelope(scoped(fresh_brokerage_required=False), structured=True), None, scope)
    for delay, expected in [(0, None), (1, accepted(1, True)), (1000, accepted(1000, True)), (1001, None)]:
        yield case(f"scoped_delay_{delay}", "delay must be 1..=1000 ms",
                   envelope(scoped(delay=delay), structured=True), expected, scope)
    yield case("scoped_integral_float_delay", "the delay is a JSON integer literal; 25.0 is refused",
               envelope(scoped(delay=25.0), structured=True), None, scope)


def render():
    document = {
        "schema": SCHEMA,
        "rust": "crates/airk-cli/tests/common/read_retry.rs classify, classify_for_scope",
        "typescript": "packages/vscode/src/readRetry.ts readRetryDelay",
        "generator": "fixtures/wire/read_retry_envelopes.py",
        "native_busy_codes": NATIVE_BUSY_CODES,
        "cases": [*storage_cases(), *direct_cases(), *scoped_cases()],
    }
    return json.dumps(document, indent=1, ensure_ascii=True, sort_keys=True) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    rendered = render()
    names = [entry["name"] for entry in json.loads(rendered)["cases"]]
    if len(names) != len(set(names)):
        raise SystemExit("duplicate case names")
    if args.check:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if current != rendered:
            print(f"{OUTPUT} is stale; rerun without --check and review the diff", file=sys.stderr)
            return 1
        return 0
    OUTPUT.write_bytes(rendered.encode("utf-8"))
    print(f"{len(names)} cases -> {OUTPUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
