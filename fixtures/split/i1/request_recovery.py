# SPDX-License-Identifier: Apache-2.0
"""Generate the I1 request-journal recovery transcripts (split C3a).

Each case scripts one recovery situation of split plan §5.2 and
docs/split/contracts.md §2.4: the client's pipe frames, the service's replies,
the injected disconnect, death, restart or epoch rotation, and the expected
client-visible outcome. Expectations are reviewed statements of the contract,
written independently of any implementation; nothing here is computed by the
code under test.

Grounding: `crates/airk-cli/src/client_requests.rs` (journal), registry key rules
`crates/airk-store/src/service/registry/jobs.rs:636-729` (existing key first,
then epoch), `request_receipt` (jobs.rs:243-258), wire spelling of registry
refusals (`service_management.rs:500`, Debug format), the instance fence
(`service_runtime.rs:143-196`) and the management timeout
(`service_management.rs:1629`). M5-M8 shapes are the [P] variants of §2.2.

    python fixtures/split/i1/request_recovery.py          # rewrite the JSON
    python fixtures/split/i1/request_recovery.py --check  # exit 1 on drift
"""
import argparse
import json
from pathlib import Path
import sys

OUTPUT = Path(__file__).with_name("request_recovery.json")
SCHEMA = "airk.split-fixture.i1-request-recovery/1"
NAMESPACE = "0a" * 16
CALLER = "0c" * 16
NONCE = "0b" * 16
NEW_NONCE = "0d" * 16
PROJECT = "a1" * 32
JOB = "b2" * 32
INSTANCE = "e1" * 16
RESTARTED = "e2" * 16
EPOCH = 7
OUTCOMES = ["submitted", "accepted", "completed", "refused", "unknown"]
# Definitive M5 refusals: the service proves nothing was journaled under the key.
DEFINITIVE_M5_REFUSALS = ["client_request_key_expired", "client_request_journal_full"]


def key(nonce=NONCE, epoch=EPOCH):
    return {"namespace": NAMESPACE, "epoch": epoch, "nonce": nonce}


def enqueue(kind="index", nonce=NONCE, epoch=EPOCH):
    return {"enqueue": {"project": PROJECT, "caller": CALLER, "key": key(nonce, epoch), "kind": kind}}


def prepare(kind="index", nonce=NONCE, epoch=EPOCH, outer_nonce=None):
    return {"client_request_prepare": {"caller": CALLER, "key": key(outer_nonce or nonce, epoch),
                                       "request": enqueue(kind, nonce, epoch)}}


def submit(nonce=NONCE):
    return {"client_request_submit": {"nonce": nonce}}


def receipt_request(nonce=NONCE, epoch=EPOCH):
    return {"request_receipt": {"caller": CALLER, "key": key(nonce, epoch)}}


def send(method, instance=INSTANCE):
    return {"step": "send", "frame": {"protocol": 1, "expected_instance": instance, "method": method}}


def reply(response):
    return {"step": "reply", "frame": response}


def reply_lost(response):
    """The service committed `response`; the connection dropped before the client read it."""
    return {"step": "reply_lost", "frame": response}


def harness(event, **fields):
    return {"step": event, **fields}


def classify(outcome):
    return {"step": "classify", "outcome": outcome}


def epoch(value=EPOCH):
    return {"status": "request_epoch", "namespace": NAMESPACE, "epoch": value}


def prepared(duplicate=False, nonce=NONCE, kind="index"):
    return {"status": "client_request_prepared", "nonce": nonce, "project": PROJECT, "kind": kind,
            "duplicate": duplicate}


def job(state="pending", outcome=None):
    return {"status": "job", "job": job_record(state, outcome)}


def job_record(state="pending", outcome=None):
    return {"id": JOB, "project": PROJECT, "kind": "index", "state": state, "fencing_generation": 1,
            "attempts": 0 if state == "pending" else 1, "outcome": outcome}


def receipt(state=None, expired=False, outcome=None):
    return {"status": "request_receipt", "job": job_record(state, outcome) if state else None, "expired": expired}


def listed(*nonces, kind="index"):
    return {"status": "client_requests", "pending": [{"nonce": n, "project": PROJECT, "kind": kind} for n in nonces],
            "capacity": 64}


def failure(code):
    return {"status": "failure", "code": code}


def mints(nonce, value=EPOCH, after=None):
    step = {"step": "client_mints_key", "caller": CALLER, "key": key(nonce, value)}
    if after:
        step["after"] = after
    return step


def journaled():
    """Epoch, key and durable acceptance with a delivered reply."""
    return [send("request_epoch"), reply(epoch()), mints(NONCE), send(prepare()), reply(prepared())]


def case(name, situation, transcript, outcome, reuse, new_key, journal, jobs, note, grounding):
    return {"name": name, "situation": situation, "transcript": transcript,
            "expected": {"outcome": outcome, "retry_may_reuse_key": reuse, "new_key_allowed": new_key,
                         "journal_entry": journal, "accepted_jobs_for_key": jobs},
            "note": note, "grounding": grounding}


def cases():
    yield case("disconnect_before_acceptance", "disconnect before acceptance", [
        send("request_epoch"), reply(epoch()), mints(NONCE), send(prepare()),
        harness("connection_lost_before_durable"), classify("submitted"),
        send({"client_request_list": {}}), reply(listed()),
        send(prepare()), reply(prepared()), send(submit()), reply(job()),
    ], "accepted", True, False, True, 1,
        "The client resends the identical M5; it never mints a key because the first reply is missing.",
        ["client_requests.rs:213-257"])
    yield case("disconnect_before_acceptance_epoch_rotated", "disconnect before acceptance, then stale epoch", [
        send("request_epoch"), reply(epoch()), mints(NONCE), send(prepare()),
        harness("connection_lost_before_durable"), harness("epoch_rotates", epoch=EPOCH + 1),
        send(prepare()), reply(failure("client_request_key_expired")),
        send("request_epoch"), reply(epoch(EPOCH + 1)),
        mints(NEW_NONCE, EPOCH + 1, after="client_request_key_expired"),
        send(prepare(nonce=NEW_NONCE, epoch=EPOCH + 1)), reply(prepared(nonce=NEW_NONCE)),
    ], "accepted", False, True, True, 0,
        "M5 checks the nonce before the epoch; a stale key with no journal entry is a definitive refusal, "
        "so only then may the client mint a new key.", ["jobs.rs:636-729"])
    yield case("disconnect_after_durable_acceptance", "disconnect after durable acceptance", [
        send("request_epoch"), reply(epoch()), mints(NONCE), send(prepare()), reply_lost(prepared()),
        classify("submitted"), send(prepare()), reply(prepared(duplicate=True)), classify("accepted"),
        send(submit()), reply(job()),
    ], "accepted", True, False, True, 1, "The resent identical M5 answers duplicate:true.",
        ["client_requests.rs:213-257"])
    yield case("disconnect_after_submit_reply_lost", "disconnect after durable acceptance (submit reply lost)", [
        *journaled(), send(submit()), reply_lost(job()), classify("unknown"),
        send(receipt_request()), reply(receipt("pending")),
    ], "accepted", True, False, True, 1, "Receipt lookup recovers the accepted job; nothing is resent.",
        ["jobs.rs:243-258"])
    yield case("disconnect_after_submit_not_yet_accepted", "disconnect after durable acceptance (submit lost)", [
        *journaled(), send(submit()), harness("connection_lost_before_effect"), classify("unknown"),
        send(receipt_request()), reply(receipt(None, expired=False)), classify("accepted"),
        send(submit()), reply(job()),
    ], "accepted", True, False, True, 1,
        "job:null, expired:false: resend M7 with the same nonce; the saved key makes it idempotent.",
        ["jobs.rs:243-258", "jobs.rs:636-660"])
    yield case("disconnect_after_completion", "disconnect after completion", [
        *journaled(), send(submit()), reply_lost(job()), harness("job_finishes", state="succeeded"),
        send(receipt_request()), reply(receipt("succeeded", outcome={"state": "succeeded", "revision": None})),
        send({"client_request_forget": {"nonce": NONCE}}),
        reply({"status": "client_request_forgotten", "nonce": NONCE, "existed": True}),
    ], "completed", False, False, False, 1, "A terminal receipt is displayed; forgetting never cancels work.",
        ["jobs.rs:243-258", "client_requests.rs:354-367"])
    yield case("client_death_before_submit", "client death", [
        *journaled(), harness("client_dies"), harness("client_starts", surface="session"),
        send({"client_request_list": {}}), reply(listed(NONCE)), send(submit()), reply(job()),
    ], "accepted", True, False, True, 1,
        "Another surface finds the entry by M6 and submits the saved request by nonce.",
        ["client_requests.rs:301-352"])
    yield case("client_death_after_submit", "client death", [
        *journaled(), send(submit()), reply_lost(job()), harness("client_dies"),
        harness("client_starts", surface="admin"), send({"client_request_list": {}}), reply(listed(NONCE)),
        send(submit()), reply(job()),
    ], "accepted", True, False, True, 1,
        "The new surface lacks caller and key, so it resubmits by nonce: the saved key returns the same job.",
        ["jobs.rs:636-660"])
    yield case("service_restart_after_submit", "service restart", [
        *journaled(), send(submit()), harness("service_restarts", service_instance=RESTARTED),
        send(receipt_request()), reply(failure("service_instance_changed")),
        harness("observe_health", service_instance=RESTARTED),
        send(receipt_request(), RESTARTED), reply(receipt("pending")),
    ], "accepted", True, False, True, 1,
        "The stale instance fence refuses before dispatch; the journal and receipts are durable.",
        ["service_runtime.rs:143-196", "jobs.rs:243-258"])
    yield case("service_restart_before_submit", "service restart", [
        *journaled(), harness("service_restarts", service_instance=RESTARTED),
        harness("observe_health", service_instance=RESTARTED),
        send({"client_request_list": {}}, RESTARTED), reply(listed(NONCE)),
        send(submit(), RESTARTED), reply(job()),
    ], "accepted", True, False, True, 1, "A reconnect never replays M5 with a new key.",
        ["client_requests.rs:213-257"])
    yield case("duplicate_key_same_arguments", "duplicate key, same arguments", [
        *journaled(), send(prepare()), reply(prepared(duplicate=True)), send(submit()), reply(job()),
        send(submit()), reply(job()), send(enqueue()), reply(job()),
    ], "accepted", True, False, True, 1, "Every repeat returns the one accepted job.", ["jobs.rs:638-660"])
    yield case("same_key_different_arguments_journal", "same key, different arguments", [
        *journaled(), send(prepare(kind="verify")), reply(failure("client_request_key_conflict")),
        send({"client_request_list": {}}), reply(listed(NONCE)),
    ], "refused", False, False, True, 0, "Fail closed; the original journal entry is unchanged.",
        ["client_requests.rs:254 (persist_noclobber)"])
    yield case("same_key_different_arguments_registry", "same key, different arguments", [
        *journaled(), send(submit()), reply(job()), send(enqueue(kind="verify")), reply(failure("KeyConflict")),
        send(receipt_request()), reply(receipt("pending")),
    ], "refused", False, False, True, 1,
        "Existing registry spelling (Debug format) is kept; the original is shown by receipt.",
        ["jobs.rs:653-657", "service_management.rs:500"])
    yield case("prepare_key_mismatch", "same key, different arguments", [
        send("request_epoch"), reply(epoch()), mints(NONCE), send(prepare(outer_nonce=NEW_NONCE)),
        reply(failure("client_request_key_mismatch")),
    ], "refused", False, False, False, 0, "M5's caller and key must equal those inside its request.", [])
    yield case("stale_epoch_journaled_not_submitted", "stale epoch", [
        *journaled(), harness("epoch_rotates", epoch=EPOCH + 1), send(submit()), reply(failure("KeyExpired")),
        send(receipt_request()), reply(receipt(None, expired=True)), classify("unknown"),
    ], "unknown", False, False, True, 0,
        "KeyExpired cannot distinguish never accepted from accepted and pruned (the journal records no "
        "submission), so the outcome is unknown and nothing is replayed.", ["jobs.rs:390-421", "jobs.rs:723-729"])
    yield case("stale_epoch_already_accepted", "stale epoch", [
        *journaled(), send(submit()), reply_lost(job()), harness("epoch_rotates", epoch=EPOCH + 1),
        send(receipt_request()), reply(receipt("pending")), send(submit()), reply(job()),
    ], "accepted", True, False, True, 1, "The accepted key is looked up before the epoch check.",
        ["jobs.rs:638-660", "jobs.rs:243-258"])
    yield case("receipt_lookup_accepted", "receipt lookup: accepted", [
        *journaled(), send(submit()), reply(job()), harness("job_starts"),
        send(receipt_request()), reply(receipt("running")),
    ], "accepted", True, False, True, 1, "Pending or running is accepted.", ["jobs.rs:243-258"])
    yield case("receipt_lookup_completed", "receipt lookup: completed", [
        *journaled(), send(submit()), reply(job()), harness("job_finishes", state="failed"),
        send(receipt_request()), reply(receipt("failed", outcome={"state": "failed", "code": "fixture_failure"})),
    ], "completed", False, False, True, 1, "A failed terminal outcome is completed, not unknown.",
        ["jobs.rs:243-258"])
    yield case("receipt_lookup_unknown", "receipt lookup: unknown", [
        *journaled(), send(submit()), reply_lost(job()), harness("job_finishes", state="succeeded"),
        harness("receipt_pruned"), harness("epoch_rotates", epoch=EPOCH + 1),
        send(receipt_request()), reply(receipt(None, expired=True)),
    ], "unknown", False, False, True, 1, "A pruned receipt is unknown, never success.", ["jobs.rs:243-258"])
    yield case("management_timeout_on_submit", "disconnect after durable acceptance (actor timeout)", [
        *journaled(), send(submit()), reply(failure("registry_response_unknown_retry_with_key")),
        classify("unknown"), send(submit()), reply(job()),
    ], "accepted", True, False, True, 1, "The code names its own recovery: retry with the same key.",
        ["service_management.rs:1610-1632"])
    yield case("journal_full", "journal capacity", [
        send("request_epoch"), reply(epoch()), mints(NONCE), send(prepare()),
        reply(failure("client_request_journal_full")),
    ], "refused", True, True, False, 0,
        "Definitive refusal before journaling; inspect and forget entries, then resend.",
        ["client_requests.rs:238-242"])


def dump(value, depth=0, inline_at=4):
    if depth >= inline_at or not isinstance(value, (dict, list)) or not value:
        return json.dumps(value, separators=(", ", ": "), ensure_ascii=True)
    pad = " " * (depth + 1)
    if isinstance(value, dict):
        items = [f"{pad}{json.dumps(k)}: {dump(v, depth + 1, inline_at)}" for k, v in value.items()]
        return "{\n" + ",\n".join(items) + "\n" + " " * depth + "}"
    return "[\n" + ",\n".join(pad + dump(v, depth + 1, inline_at) for v in value) + "\n" + " " * depth + "]"


def render():
    document = {
        "schema": SCHEMA,
        "contract": "docs/split/contracts.md §2.4; split plan §5.2",
        "generator": "fixtures/split/i1/request_recovery.py",
        "rules": [
            "Reconnect never mints a new key and never replays an uncertain mutation.",
            "A new key is minted only right after a definitive M5 refusal proving nothing was journaled: "
            + ", ".join(DEFINITIVE_M5_REFUSALS) + ".",
            "M7 resubmits the exact saved key by nonce; the registry returns the accepted job or refuses.",
            "M5 order: identical saved request -> duplicate:true; same nonce, other bytes -> "
            "client_request_key_conflict; stale namespace or epoch -> client_request_key_expired; "
            "full -> client_request_journal_full.",
            "Registry refusals keep their existing wire spelling KeyConflict and KeyExpired.",
        ],
        "outcomes": OUTCOMES,
        "definitive_m5_refusals": DEFINITIVE_M5_REFUSALS,
        "steps": {
            "send": "client writes one ControlRequest on a new owner-pipe connection",
            "reply": "client reads the service's ControlResponse",
            "reply_lost": "service committed this response; the connection dropped before the client read it",
            "connection_lost_before_durable": "connection dropped before M5 was durable; nothing journaled",
            "connection_lost_before_effect": "connection dropped before the registry saw the submission",
            "client_mints_key": "client generates caller and key; `after` names the refusal that allows it",
            "classify": "client-visible state at this point",
        },
        "cases": list(cases()),
    }
    return dump(document) + "\n"


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
