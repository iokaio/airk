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

The separate `binding_cases` retain the caller's original native selector,
public selection source where applicable and original request arguments.
They test the Rust caller-binding helpers and their TypeScript consumer;
coherent outer/inner metadata alone never establishes active authority.

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
    # Formerly divergence W3: the TypeScript mirror now refuses non-integer
    # number literals in the native text (JSON.parse source-text access).
    yield case("integral_float_delay", "retry_after_ms must be a JSON integer",
               structured(storage_hint(retry_after_ms=25.0)), None)
    yield from selection_cases()


def selection_cases():
    """Independent selection-source rules already supported by native Core."""
    def selected(name, rule, expected, owner="window", observed=True, mutate=None):
        source = {"owner": owner, "scope_identity": "0c" * 32, "catalog_revision": 5}
        hint = storage_hint()
        if observed:
            hint["scope_observation"] = {
                "schema": 1, "qualifier": "selected_subset", "fingerprint": "07" * 32,
                "selection": copy.deepcopy(source),
                "members": [{"index_id": INDEX,
                             "memberships": [{"project_id": PROJECT, "membership_revision": 1}],
                             "source_root_ids": ["src_" + "06" * 16], "source_revision": 1,
                             "policy_epoch": 0, "policy_digest": None}],
            }
        value = combined(structured(hint), expected if expected is not None else retry())
        del value["selection"]["grant_id"]
        del value["selection"]["grant_revision"]
        value["selection"]["selection"] = source
        if mutate:
            mutate(value)
            result = value["members"][0]["result"]
            result["content"][0]["text"] = text(result["structuredContent"])
        return {"name": name, "rule": rule, "input": value, "expected": expected}

    yield selected("selection_window_without_observation", "one selection source needs no fabricated member observation", retry(), observed=False)
    yield selected("selection_window_matching_observation", "window observation must agree with its exact source", retry())
    yield selected("selection_admin_matching_observation", "admin observation uses the same exact-source rule", retry(), owner="admin")
    for field, replacement in [("owner", "admin"), ("scope_identity", "0d" * 32), ("catalog_revision", 6)]:
        yield selected(f"selection_member_changed_{field}", "member selection owner, identity and revision must all match", None,
                       mutate=set_path("members", 0, "result", "structuredContent", "scope_observation", "selection", field, replacement))
    yield selected("selection_with_catalog_grant", "catalog and selection sources are mutually exclusive", None,
                   mutate=lambda value: value["selection"].update(grant_id=GRANT, grant_revision=1))
    yield selected("selection_without_source", "neither authority source is invalid", None,
                   mutate=lambda value: value["selection"].pop("selection"))
    for field in ["grant_nonce", "service_instance"]:
        yield selected(f"selection_member_leaks_{field}", "public selection observations never carry native secrets", None,
                       mutate=set_path("members", 0, "result", "structuredContent", "scope_observation", "selection", field, "0a" * 16))
    yield selected("selection_member_other_index", "the member observation must name the sole selected index", None,
                   mutate=set_path("members", 0, "result", "structuredContent", "scope_observation", "members", 0, "index_id", "idx_" + "08" * 16))
    yield selected("selection_member_other_project", "membership observations stay inside selected projects", None,
                   mutate=set_path("members", 0, "result", "structuredContent", "scope_observation", "members", 0, "memberships", 0, "project_id", SECOND_PROJECT))


def binding_cases():
    """Caller authority is independent of a coherent outer/inner reply."""
    source = {"owner": "window", "scope_identity": "0c" * 32, "catalog_revision": 5}
    window = {"namespace": "09" * 16, "expected_catalog_revision": 5,
              "index_ids": [INDEX], "project_ids": [PROJECT]}
    catalog = {"schema": 1, "context_key": "0a" * 32, "namespace": "09" * 16,
               "expected_catalog_revision": 5, "grant_id": GRANT, "expected_grant_revision": 1,
               "index_id": INDEX, "project_ids": [PROJECT]}
    arguments = {"project_ids": [PROJECT], "index_ids": [INDEX], "member_availability": "require"}
    observation = {"schema": 1, "qualifier": "selected_subset", "fingerprint": "07" * 32,
                   "members": [{"index_id": INDEX,
                                "memberships": [{"project_id": PROJECT, "membership_revision": 1}],
                                "source_root_ids": ["src_" + "06" * 16], "source_revision": 1,
                                "policy_epoch": 0, "policy_digest": None}]}

    def bound(name, rule, expected, kind="selection", mutate=None):
        member = copy.deepcopy(observation)
        selected = combined(structured(storage_hint()), retry())
        if kind == "selection":
            member["selection"] = copy.deepcopy(source)
            del selected["selection"]["grant_id"]
            del selected["selection"]["grant_revision"]
            selected["selection"]["selection"] = copy.deepcopy(source)
            expected_scope = copy.deepcopy(window)
        else:
            member.update(grant={"id": GRANT, "revision": 1}, provenance="user_reviewed_project_grant")
            expected_scope = copy.deepcopy(catalog)
            if kind == "catalog_combined":
                del expected_scope["index_id"]
                expected_scope["index_ids"] = [INDEX]
        selected["members"][0]["result"]["structuredContent"]["scope_observation"] = member
        entry = {"name": name, "rule": rule, "kind": kind, "input": selected,
                 "expected_scope": expected_scope, "arguments": copy.deepcopy(arguments), "expected": expected}
        if kind == "selection":
            entry["expected_selection"] = copy.deepcopy(source)
        if mutate:
            mutate(entry)
        result = entry["input"]["members"][0]["result"]
        result["content"][0]["text"] = text(result["structuredContent"])
        return entry

    def coherent_source(field, value):
        def change(entry):
            entry["input"]["selection"]["selection"][field] = value
            entry["input"]["members"][0]["result"]["structuredContent"]["scope_observation"]["selection"][field] = value
        return change

    def coherent_index(entry):
        other = "idx_" + "08" * 16
        value = entry["input"]
        value["selection"]["index_ids"] = [other]
        value["members"][0]["index_id"] = other
        value["members"][0]["allocation"]["index_id"] = other
        value["publication_vector"]["members"][0]["index_id"] = other
        value["error"]["failed_members"] = [other]
        value["read_retry"]["index_id"] = other
        value["members"][0]["result"]["structuredContent"]["scope_observation"]["members"][0]["index_id"] = other

    yield bound("active_window", "the exact captured window source and original request permit one replay", retry())
    def admin(entry):
        coherent_source("owner", "admin")(entry)
        entry["expected_selection"]["owner"] = "admin"
    yield bound("active_admin", "the exact captured admin source follows the same rule", retry(), mutate=admin)
    for field, value in [("scope_identity", "0d" * 32), ("catalog_revision", 6), ("owner", "admin")]:
        yield bound(f"coherent_stale_{field}", "coherent outer and inner metadata cannot replace original caller authority", None,
                    mutate=coherent_source(field, value))
    yield bound("caller_epoch_disagrees", "the captured source revision agrees with the captured window revision", None,
                mutate=set_path("expected_selection", "catalog_revision", 6))
    yield bound("coherent_other_index", "the original captured index set bounds even a coherent result", None, mutate=coherent_index)
    def other_project(entry):
        entry["input"]["selection"]["project_ids"] = [SECOND_PROJECT]
        entry["input"]["members"][0]["result"]["structuredContent"]["scope_observation"]["members"][0]["memberships"][0]["project_id"] = SECOND_PROJECT
    yield bound("coherent_other_project", "the original requested project set bounds the observed membership", None, mutate=other_project)
    def pruned(entry):
        entry["expected_scope"]["project_ids"] = [PROJECT, SECOND_PROJECT]
        entry["arguments"]["project_ids"] = [SECOND_PROJECT, PROJECT]
    yield bound("pruned_projects", "native membership pruning may retain a nonempty subset of requested projects", retry(), mutate=pruned)
    def narrowed(entry):
        entry["expected_scope"]["index_ids"].append("idx_" + "08" * 16)
    yield bound("explicit_narrowed_index", "an explicit original index subset remains exact", retry(), mutate=narrowed)
    def resolved(entry):
        narrowed(entry)
        del entry["arguments"]["index_ids"]
    yield bound("project_resolved_index_subset", "explicit projects may resolve only a subset of known permitted indexes", retry(), mutate=resolved)
    def wrong_narrow(entry):
        narrowed(entry)
        coherent_index(entry)
    yield bound("coherent_other_permitted_index", "a sibling permitted index is not the explicitly requested index", None, mutate=wrong_narrow)
    for name, change in [
        ("original_report_mode", set_path("arguments", "member_availability", "report")),
        ("original_default_mode", lambda entry: entry["arguments"].pop("member_availability")),
        ("original_indexes_without_projects", lambda entry: entry["arguments"].pop("project_ids")),
        ("original_duplicate_projects", set_path("arguments", "project_ids", [PROJECT, PROJECT])),
        ("original_unknown_index", set_path("arguments", "index_ids", ["idx_" + "0f" * 16])),
        ("original_unknown_project", set_path("arguments", "project_ids", [SECOND_PROJECT])),
    ]:
        yield bound(name, "invalid, widened or non-required original selectors never authorize replay", None, mutate=change)
    for kind in ["catalog_single", "catalog_combined"]:
        yield bound(f"{kind}_active", "the exact original grant/revision and known index selection permit replay", retry(), kind=kind)
        yield bound(f"{kind}_grant_changed", "an older or foreign grant cannot replace the captured grant", None, kind=kind,
                    mutate=set_path("expected_scope", "grant_id", "grt_" + "0e" * 16))
        yield bound(f"{kind}_grant_revision_changed", "grant revision remains exact", None, kind=kind,
                    mutate=set_path("expected_scope", "expected_grant_revision", 2))
        yield bound(f"{kind}_metadata_clock_forward", "catalog metadata clock changes do not invent a different grant revision", retry(), kind=kind,
                    mutate=set_path("expected_scope", "expected_catalog_revision", 12))
        yield bound(f"{kind}_other_index", "a coherent other index cannot replace the captured selection", None, kind=kind, mutate=coherent_index)
        yield bound(f"{kind}_pruned_projects", "catalog callers retain safe membership pruning", retry(), kind=kind, mutate=pruned)
    yield bound("catalog_combined_unknown_expansion", "absent captured index membership is not fabricated from a receipt", None, kind="catalog_combined",
                mutate=lambda entry: entry["expected_scope"].pop("index_ids"))
    yield bound("window_reply_to_catalog_caller", "authority source kinds never substitute for one another", None,
                mutate=lambda entry: entry.update(kind="catalog_single", expected_scope=copy.deepcopy(catalog)))
    def catalog_to_window(entry):
        entry.update(kind="selection", expected_scope=copy.deepcopy(window), expected_selection=copy.deepcopy(source))
    yield bound("catalog_reply_to_window_caller", "a catalog grant cannot substitute for an active window source", None,
                kind="catalog_single", mutate=catalog_to_window)


def render():
    document = {
        "schema": SCHEMA,
        "rust": "airk_core::combined_reads::read_retry::validated_retry",
        "typescript": "packages/vscode/src/readRetry.ts singleRequiredMemberRetry",
        "generator": "fixtures/wire/read_retry_decisions.py",
        "cases": list(cases()),
        "binding_cases": list(binding_cases()),
        "rust_binding": "airk_core::combined_reads::read_retry validated_retry_for_selection, validated_retry_for_catalog, validated_retry_for_combined_catalog",
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
