# SPDX-License-Identifier: Apache-2.0
"""Generate the shared read-retry decision cases (simplification plan S1.3).

The rule under test is "may this combined read be repeated once, exactly?":
Rust `airk_core::combined_reads::read_retry::validated_retry` and the
hand-written TypeScript `singleRequiredMemberRetry` (packages/vscode/src/
readRetry.ts). Both consume the same materialized JSON inputs written here.

Every `expected` value below is a reviewed statement of the rule, written
independently of either implementation. It is never computed by the code
under test. `expected_ts` appears only where the TypeScript mirror is
deliberately or knowingly different. Each such case carries a `divergence`
note, which the pilot report lists.

    python fixtures/wire/read_retry_decisions.py          # rewrite the JSON
    python fixtures/wire/read_retry_decisions.py --check  # exit 1 on drift
"""
import argparse
import copy
import json
from pathlib import Path
import sys

OUTPUT = Path(__file__).with_name("read_retry_decisions.json")
SCHEMA = "airk.wire-fixture.read-retry-decisions/1"

INDEX = "idx_" + "01" * 16
PROJECT = "prj_" + "02" * 16
SECOND_PROJECT = "prj_" + "0a" * 16
GRANT = "grt_" + "03" * 16
IDENTITY = "04" * 32
AUTHORITY = "05" * 32
ABOVE_SAFE_INTEGER = 2**53 + 1


def storage_hint(**changes):
    hint = {"status": "retry", "code": "selection_busy", "retryable": True,
            "action": "repeat_same_request", "retry_after_ms": 25}
    hint.update(changes)
    return hint


def direct_hint(code="direct_query_admission", fresh=False, delay=40, **changes):
    error = {"code": code, "reason": "busy", "query_submitted": False, "retryable": True,
             "retry_after_ms": delay, "fresh_brokerage_required": fresh}
    error.update(changes)
    return {"error": error}


def text(value):
    return json.dumps(value, separators=(",", ":"))


def structured(hint):
    return {"isError": True, "content": [{"type": "text", "text": text(hint)}], "structuredContent": hint}


def retry(code="selection_busy", delay=25, fresh=False, pointer="/members/0/result/structuredContent"):
    return {"schema": 1, "scope": "same_single_required_member", "index_id": INDEX,
            "selection_identity": IDENTITY, "action": "repeat_same_request", "query_replay_safe": True,
            "result_pointer": pointer, "code": code, "retry_after_ms": delay,
            "fresh_brokerage_required": fresh}


def combined(result, declared):
    value = {
        "schema": "airk.combined-read/1",
        "selection": {"project_ids": [PROJECT], "index_ids": [INDEX], "grant_id": GRANT,
                      "grant_revision": 1, "member_availability": "require", "identity": IDENTITY},
        "publication_vector": {"authority": AUTHORITY, "observed_from_unix_ms": 1, "observed_to_unix_ms": 2,
                               "members": [{"index_id": INDEX, "publication": None}]},
        "members": [{"index_id": INDEX, "is_error": True, "outcome": "failed",
                     "allocation": {"index_id": INDEX, "limit": 1, "max_bytes": 4096, "deadline_ms": 2000,
                                    "verification_max_sources": 1, "verification_max_bytes": 4096,
                                    "verification_deadline_ms": 2000},
                     "result": result}],
        "ranking": [], "partial": True, "truncated": False,
        "error": {"code": "member_unavailable", "retryable": False, "failed_members": [INDEX]},
    }
    if declared is not None:
        value["read_retry"] = declared
    return value


def case(name, rule, result, expected, declared="expected", mutate=None, expected_ts="same", divergence=None):
    """`declared` defaults to the expected record, or the canonical record for refusals,
    so a refusal is attributable to the named mutation rather than a missing declaration."""
    if declared == "expected":
        declared = expected if expected is not None else retry()
    value = combined(result, declared)
    if mutate:
        mutate(value)
    entry = {"name": name, "rule": rule, "input": value, "expected": expected}
    if expected_ts != "same":
        entry["expected_ts"] = expected_ts
        entry["divergence"] = divergence
    return entry


def set_path(*path_and_value):
    *path, value = path_and_value

    def apply(target):
        for key in path[:-1]:
            target = target[key]
        target[path[-1]] = value
    return apply


def cases():
    canonical = structured(storage_hint())
    yield case("structured_selection_busy", "canonical storage busy hint inside MCP structured content", canonical, retry())
    yield case("structured_publication_busy", "publication_busy is the other storage busy code",
               structured(storage_hint(code="publication_busy")), retry(code="publication_busy"))
    yield case("bare_storage_hint", "a bare native envelope points at the result itself",
               storage_hint(), retry(pointer="/members/0/result"))
    yield case("undeclared", "an inner busy hint alone is not an outer replay instruction", canonical, None, declared=None)
    yield case("declared_differs", "declared metadata must equal the proposal exactly", canonical, None,
               declared=retry(delay=26))
    yield case("direct_admission_busy", "unsubmitted busy admission keeps the brokerage",
               structured(direct_hint()), retry(code="direct_query_admission", delay=40))
    yield case("direct_setup_busy", "busy setup requires fresh brokerage",
               structured(direct_hint(code="direct_query_setup", fresh=True)),
               retry(code="direct_query_setup", delay=40, fresh=True))
    yield case("direct_setup_without_fresh_brokerage", "setup must declare fresh brokerage",
               structured(direct_hint(code="direct_query_setup", fresh=False)), None)
    yield case("direct_submitted", "a submitted query is never replayed",
               structured(direct_hint(query_submitted=True)), None)
    yield case("delay_zero", "delay must be 1..=1000 ms", structured(storage_hint(retry_after_ms=0)), None)
    yield case("delay_maximum", "1000 ms is the inclusive bound",
               structured(storage_hint(retry_after_ms=1000)), retry(delay=1000))
    yield case("delay_above_maximum", "delay must be 1..=1000 ms", structured(storage_hint(retry_after_ms=1001)), None)
    yield case("truncated", "a truncated composition is never replayed", canonical, None,
               mutate=set_path("truncated", True))
    yield case("not_partial", "only a partial composition can carry a member failure", canonical, None,
               mutate=set_path("partial", False))
    yield case("ranked", "a non-empty ranking means results were delivered", canonical, None,
               mutate=set_path("ranking", ["x"]))
    yield case("shared_values_present", "shared values mean results were delivered", canonical, None,
               mutate=set_path("shared_values", {}))
    yield case("member_succeeded", "only a failed member can be retried", canonical, None,
               mutate=set_path("members", 0, "outcome", "succeeded"))
    yield case("member_cutoff", "a cut-off member is not a busy refusal", canonical, None,
               mutate=set_path("members", 0, "cutoff", "deadline"))
    yield case("outer_error_retryable", "the outer composition error itself must not be retryable", canonical, None,
               mutate=set_path("error", "retryable", True))
    yield case("publication_observed", "a member with an observed publication is not unavailable", canonical, None,
               mutate=set_path("publication_vector", "members", 0, "publication", "airk-r1:fixture:1:0"))
    disagreeing = structured(storage_hint())
    disagreeing["content"][0]["text"] = text(storage_hint(retry_after_ms=30))
    yield case("text_disagrees_with_structured_content", "text and structured content must agree", disagreeing, None)
    yield case("storage_hint_submitted", "query_submitted must be false when present",
               structured(storage_hint(query_submitted=True)), None)
    yield case("storage_hint_unknown_field", "hint fields are a closed set",
               structured(storage_hint(note="fixture")), None)
    yield case("storage_hint_other_code", "only publication_busy and selection_busy are storage busy codes",
               structured(storage_hint(code="index_busy")), None)
    yield case("request_id_128_bytes", "request_id may be up to 128 bytes",
               structured(storage_hint(request_id="r" * 128)), retry())
    yield case("request_id_129_bytes", "request_id may be up to 128 bytes",
               structured(storage_hint(request_id="r" * 129)), None)
    yield case("two_projects", "several distinct projects may share one index selection", canonical, retry(),
               mutate=set_path("selection", "project_ids", [PROJECT, SECOND_PROJECT]))
    yield case("duplicate_projects", "project ids must be unique", canonical, None,
               mutate=set_path("selection", "project_ids", [PROJECT, PROJECT]))
    yield case("two_indexes", "only a single required member can be retried", canonical, None,
               mutate=set_path("selection", "index_ids", [INDEX, "idx_" + "0b" * 16]))
    yield case("optional_members", "member_availability must be require", canonical, None,
               mutate=set_path("selection", "member_availability", "best_effort"))
    yield case("grant_revision_above_safe_integer",
               "catalog revisions are 1..=i64::MAX; the value does not enter the retry record",
               canonical, retry(), mutate=set_path("selection", "grant_revision", ABOVE_SAFE_INTEGER),
               expected_ts=None,
               divergence="TypeScript parses JSON numbers as doubles and refuses non-safe integers; "
                          "native accepts revisions up to i64::MAX. Conservative refusal (no replay).")
    yield case("integral_float_delay", "retry_after_ms must be a JSON integer",
               structured(storage_hint(retry_after_ms=25.0)), None,
               expected_ts=retry(),
               divergence="JSON.parse maps 25.0 to the integer 25, so the TypeScript mirror accepts a "
                          "value native refuses. Permissive: it could authorize a replay native would not "
                          "declare (native never emits 25.0 itself).")


def render():
    document = {
        "schema": SCHEMA,
        "rust": "airk_core::combined_reads::read_retry::validated_retry",
        "typescript": "packages/vscode/src/readRetry.ts singleRequiredMemberRetry",
        "generator": "fixtures/wire/read_retry_decisions.py",
        "cases": list(cases()),
    }
    return json.dumps(document, indent=1, ensure_ascii=True) + "\n"


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
