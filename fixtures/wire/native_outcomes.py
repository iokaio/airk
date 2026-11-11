# SPDX-License-Identifier: Apache-2.0
"""Generate the native reply outcome classes (split contract C8, docs/split/contracts.md §10).

Each case is one native control exchange and the reviewed outcome class the
host must give it. The consumer under test is the hand-written TypeScript
(`ServiceClient.rpc` in packages/vscode/src/serviceClient.ts and the
nativeOutcome.ts helpers `validateNativeReply`, `retainUnvalidatedNativeReply`
and `retainNativeOutcome`). Every `class` and `code` below is a reviewed
statement of the rule, written independently of that code:

  * accepted: the reply carries the method's acceptance status (`job`,
    `catalog_applied`). The complete reply is returned unchanged.
  * refused: a `failure` reply whose code proves no effect, or a session
    failure before the request was sent. `outcomeUnknown` is false.
  * unknown: a `failure` code ending in `_outcome_unknown`,
    `registry_response_unknown_retry_with_key`, `invalid_registry_request`
    after a catalog mutation (older handlers collapse a post-prepare failure
    into it), any non-failure status other than the expected acceptance
    (for example `catalog_outcome_unknown`), or transport loss after send.
    `outcomeUnknown` is true.
  * unvalidated: a well-formed accepted reply that the consumer validator
    rejects. `outcomeUnknown` is true and the reply is marked unvalidated. The
    error code is the validator's machine code: an `Error.code` matching
    `[A-Za-z0-9_]{1,128}`, else an `Error.message` matching `[a-z][a-z0-9_]{0,127}`,
    else `native_response_validation_failed`.

For every class: the complete native reply (when one arrived) is retained on the
error unchanged, the exchange is sent exactly once, and no class authorizes
another write. A local tracking failure after an accepted reply keeps that
reply and marks only local tracking unconfirmed.

Replies with `control_response: true` are complete `airk_core::service::ControlResponse`
values; the Rust test parses and re-serializes each one to the same JSON value.
The accepted catalog receipt is the reviewed `small_revision` receipt of
catalog_receipt.json. The validator itself is a stated failure (`validator`),
not a real consumer validator: real validators need complete intents and are
covered by their own tests. No pure Rust classifier exists yet; the I2 exit
mapper (split step C4a) consumes this fixture when it lands.

    python fixtures/wire/native_outcomes.py          # rewrite the JSON
    python fixtures/wire/native_outcomes.py --check  # exit 1 on drift
"""
import argparse
import copy
import json
from pathlib import Path
import sys

OUTPUT = Path(__file__).with_name("native_outcomes.json")
RECEIPTS = Path(__file__).with_name("catalog_receipt.json")
SCHEMA = "airk.wire-fixture.native-outcomes/1"
CLASSES = ("accepted", "refused", "unknown", "unvalidated")


def reviewed_receipt():
    receipts = json.loads(RECEIPTS.read_text(encoding="utf-8"))["receipts"]
    return json.loads(next(item for item in receipts if item["name"] == "small_revision")["persisted"])


RECEIPT = reviewed_receipt()
JOB = {"status": "job", "job": {"id": "0a" * 32, "project": "0b" * 32, "kind": "index", "state": "pending",
                                "fencing_generation": 1, "attempts": 0, "outcome": None}}
APPLIED = {"status": "catalog_applied", "receipt": RECEIPT}


def failure(code):
    return {"status": "failure", "code": code}


def case(name, rule, method, expected_status, outcome, code, reply=None, transport="reply", validator=None,
         control_response=True):
    if outcome not in CLASSES:
        raise SystemExit(f"{name}: unknown class {outcome}")
    entry = {"name": name, "rule": rule, "method": method, "expected_status": expected_status,
             "transport": transport, "reply": reply, "control_response": control_response and reply is not None,
             "class": outcome, "code": code}
    if validator is not None:
        entry["validator"] = validator
    return entry


def cases():
    yield case("job_accepted", "an enqueue acceptance returns the complete job reply", "enqueue", "job",
               "accepted", None, JOB)
    yield case("catalog_applied", "a catalog apply acceptance returns the complete receipt reply", "catalog_apply",
               "catalog_applied", "accepted", None, APPLIED)
    yield case("failure_refused", "an ordinary failure code is a refusal", "catalog_apply", "catalog_applied",
               "refused", "grant_revoked", failure("grant_revoked"))
    yield case("failure_invalid_registry_request_read", "invalid_registry_request outside a catalog mutation is a refusal",
               "catalog_preview", "catalog_preview", "refused", "invalid_registry_request", failure("invalid_registry_request"))
    yield case("session_refused_before_send", "a session failure before send cannot have started the request",
               "enqueue", "job", "refused", "native_session_queue_full", transport="not_sent")
    yield case("failure_outcome_unknown", "a failure code ending in _outcome_unknown is unknown", "catalog_apply",
               "catalog_applied", "unknown", "catalog_apply_outcome_unknown", failure("catalog_apply_outcome_unknown"))
    yield case("failure_registry_response_unknown", "registry_response_unknown_retry_with_key is unknown", "enqueue",
               "job", "unknown", "registry_response_unknown_retry_with_key",
               failure("registry_response_unknown_retry_with_key"))
    for method, status in [("catalog_apply", "catalog_applied"), ("catalog_source_change_resolve", "catalog_source_change_resolved")]:
        yield case(f"failure_invalid_registry_request_{method}",
                   "invalid_registry_request after a catalog mutation may hide an applied change", method, status,
                   "unknown", "invalid_registry_request", failure("invalid_registry_request"))
    yield case("catalog_outcome_unknown", "a native unknown-outcome reply is not the expected acceptance",
               "catalog_apply", "catalog_applied", "unknown", "unexpected_service_response",
               {"status": "catalog_outcome_unknown", "code": "catalog_apply_outcome_unknown", "caller": RECEIPT["caller"],
                "key": RECEIPT["key"], "request_digest": RECEIPT["request_digest"]})
    yield case("source_change_resolution_outcome_unknown", "a native unknown resolution is not the expected acceptance",
               "catalog_source_change_resolve", "catalog_source_change_resolved", "unknown", "unexpected_service_response",
               {"status": "source_change_resolution_outcome_unknown", "code": "source_change_resolution_outcome_unknown",
                "begin_operation": "15" * 16})
    yield case("transport_lost_after_send", "transport loss after send leaves the outcome unknown", "catalog_apply",
               "catalog_applied", "unknown", "native_session_disposed", transport="lost_after_send")
    crossed = copy.deepcopy(APPLIED)
    crossed["receipt"]["caller"] = "16" * 16
    yield case("crossed_receipt_unvalidated", "a well-formed receipt for another caller is retained unvalidated",
               "catalog_apply", "catalog_applied", "unvalidated", "crossed_catalog_receipt", crossed,
               validator={"message": "crossed_catalog_receipt"})
    yield case("validator_code_unvalidated", "a validator error code is kept", "enqueue", "job", "unvalidated",
               "Job_Identity_Mismatch", JOB, validator={"code": "Job_Identity_Mismatch", "message": "job identity mismatch"})
    yield case("validator_prose_unvalidated", "validator prose is not displayed as a code", "enqueue", "job",
               "unvalidated", "native_response_validation_failed", JOB, validator={"message": "Job identity does not match"})


def render():
    document = {
        "schema": SCHEMA,
        "typescript": "packages/vscode/src/serviceClient.ts ServiceClient.rpc; packages/vscode/src/nativeOutcome.ts",
        "rust": "airk_core::service::ControlResponse (reply shapes only; no classifier yet)",
        "generator": "fixtures/wire/native_outcomes.py",
        "classes": list(CLASSES),
        "cases": list(cases()),
    }
    return json.dumps(document, indent=1, ensure_ascii=True, sort_keys=True) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    rendered = render()
    parsed = json.loads(rendered)
    names = [entry["name"] for entry in parsed["cases"]]
    if len(names) != len(set(names)):
        raise SystemExit("duplicate case names")
    if {entry["class"] for entry in parsed["cases"]} != set(CLASSES):
        raise SystemExit("every class needs a reviewed case")
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
