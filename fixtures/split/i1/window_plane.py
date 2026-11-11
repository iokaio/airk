# SPDX-License-Identifier: Apache-2.0
"""Generate the I1 window-plane sequences W1-W8 (split C3b).

Each case is a timed transcript over three channels: `session` (editor window
to its `airkctl session`, I2 frames), `pipe` (that session, `airkctl` or the
admin handler to the service, I1 ControlRequest/ControlResponse) and `mcp`
(an agent's Explorer tool call on `/mcp`; the bearer is not shown). `at_ms` is
the injected manual clock. Expectations are reviewed statements of
docs/split/contracts.md §2.5 (U10, U77; ledger P0.6: window grants are
in-memory and session-bound), written independently of any implementation.

The lease is a secret: it may appear only on `pipe` frames between the owning
session and the service, never on `session` or `mcp` frames, stdout or logs.

    python fixtures/split/i1/window_plane.py          # rewrite the JSON
    python fixtures/split/i1/window_plane.py --check  # exit 1 on drift
"""
import argparse
import json
from pathlib import Path
import sys

OUTPUT = Path(__file__).with_name("window_plane.json")
SCHEMA = "airk.split-fixture.i1-window-plane/1"
INSTANCE = "e1" * 16
RESTARTED = "e2" * 16
WINDOW_A, WINDOW_B, WINDOW_A2 = "a0" * 16, "b0" * 16, "a2" * 16
LEASE_A, LEASE_B, LEASE_A2 = "1a" * 32, "1b" * 32, "2a" * 32
NAMESPACE = "0a" * 16
INDEX = "idx_" + "01" * 16
MISSING_INDEX = "idx_" + "0f" * 16
PROJECT = "prj_" + "02" * 16
SCOPE_1, SCOPE_2 = "5c" * 32, "5d" * 32
HANDOFF = "4e" * 16
TTL = 30_000
DRAFT_MAX, QUEUE_MAX_BYTES, QUEUE_MAX = 262_144, 1_048_576, 16
WINDOWS = {WINDOW_A: LEASE_A, WINDOW_B: LEASE_B, WINDOW_A2: LEASE_A2}


def pipe(at, method, instance=INSTANCE):
    return {"at_ms": at, "channel": "pipe", "send": {"protocol": 1, "expected_instance": instance, "method": method}}


def answer(at, response, channel="pipe"):
    return {"at_ms": at, "channel": channel, "reply": response}


def mcp(at, method):
    """An agent's Explorer tool call; `method` is the I1 method its handler calls."""
    return {"at_ms": at, "channel": "mcp", "send": {"method": method}}


def session(at, number, request):
    return {"at_ms": at, "channel": "session", "send": {"id": f"{number:02x}" * 16, "request": request}}


def session_reply(at, number, outcome):
    return {"at_ms": at, "channel": "session", "reply": {"id": f"{number:02x}" * 16, "outcome": outcome}}


def event(at, value):
    return {"at_ms": at, "channel": "session", "event": value}


def harness(at, what, **fields):
    return {"at_ms": at, "harness": what, **fields}


def failure(code):
    return {"status": "failure", "code": code}


def refused(code, retryable=False):
    return {"status": "failed", "code": code, "retryable": retryable}


def lease_of(window):
    return {"window_id": window, "lease": WINDOWS[window]}


def opened(window=WINDOW_A, instance=INSTANCE):
    return {"status": "window_opened", "window_id": window, "lease": WINDOWS[window], "ttl_ms": TTL,
            "service_instance": instance}


def open_window(at=0, window=WINDOW_A, label="workspace A"):
    return [pipe(at, {"window_open": {"label": label, "owner_pid": 4242}}), answer(at, opened(window))]


def selection(index=INDEX, revision=12):
    return {"namespace": NAMESPACE, "expected_catalog_revision": revision, "index_ids": [index],
            "project_ids": [PROJECT]}


def grant(at, window=WINDOW_A, scope=SCOPE_1, index=INDEX):
    return [pipe(at, {"window_scope_grant": {**lease_of(window), "selection": selection(index)}}),
            answer(at, {"status": "window_scope_granted", "scope_identity": scope, "catalog_revision": 12,
                        "expires_at_ms": at + TTL})]


def read(at, window=WINDOW_A, instance=INSTANCE):
    return pipe(at, {"tools_call": {"tool": "search", "arguments": {"query": "fixture"},
                                    "selection": {"window": lease_of(window)}, "read_budget": "elided",
                                    "usage_client": "elided"}}, instance)


SEARCH = {"invoke": {"argv": ["search", "--query", "fixture"], "deadline_ms": 60000}}
SUBSCRIBED = {"status": "subscribed", "cursor": "elided"}


def invoked_refusal(code):
    return {"status": "invoked", "exit_code": 1, "class": "refused", "stdout": "", "stderr": "elided",
            "error": {"airkctl_error": 1, "class": "refused", "code": code, "retryable": False, "message": "elided"}}


def submit(at, window=WINDOW_A, draft_bytes=None):
    arguments = {"window_id": window, "kind": "search", "draft": {"query": "fixture"}, "submitter": "codex"}
    if draft_bytes is not None:
        arguments["draft"] = {"$fill_bytes": draft_bytes}
    return pipe(at, {"assistance_submit": arguments})


def queued(number, count):
    return {"status": "assistance_queued", "draft_id": f"{0xd0 + number:02x}" * 16, "queued": count}


def draft(sequence, fill=None):
    return {"sequence": sequence, "draft_id": f"{0xd0 + sequence:02x}" * 16, "kind": "search",
            "draft": {"$fill_bytes": fill} if fill else {"query": "fixture"}}


def drain(at, ack=None, limit=8, window=WINDOW_A, owner=None):
    lease = WINDOWS[owner or window]
    return pipe(at, {"assistance_drain": {"window_id": window, "lease": lease, "ack_through": ack, "limit": limit}})


def drained(*sequences, more=None, fill=None):
    return {"status": "assistance_drained", "drafts": [draft(s, fill) for s in sequences], "next": more}


def windows(*rows):
    return {"status": "assistance_windows", "windows": [
        {"window_id": w, "label": label, "accepting": accepting, "queued": count} for w, label, accepting, count in rows]}


def case(name, rule, transcript, expected):
    return {"name": name, "rule": rule, "transcript": transcript, "expected": expected}


def cases():
    yield case("open_lease_held_by_session", "W1: the service mints the window; only the session holds the lease", [
        session(0, 1, {"hello": {"protocol": 2, "client": {"name": "airk-vscode", "version": "1.9.0"},
                                 "i2_contracts": [1]}}),
        *open_window(),
        session_reply(0, 1, {"status": "hello", "protocol": 2, "version": "1.9.0", "i2_contract": 1,
                             "capabilities": ["window-scope/1", "assistance/1"], "executable_sha256": "33" * 32,
                             "service_instance": INSTANCE, "window_id": WINDOW_A}),
    ], {"windows_open": 1, "lease_on_channels": ["pipe"]})
    yield case("renew_and_expiry", "W2: valid while now < last renewal + ttl_ms; expiry acts as close", [
        *open_window(), pipe(10_000, {"window_renew": lease_of(WINDOW_A)}),
        answer(10_000, {"status": "window_renewed", "ttl_ms": TTL}),
        pipe(39_999, {"assistance_windows": {}}), answer(39_999, windows((WINDOW_A, "workspace A", True, 0))),
        pipe(40_000, {"window_renew": lease_of(WINDOW_A)}), answer(40_000, failure("window_unknown")),
        pipe(40_000, {"assistance_windows": {}}), answer(40_000, windows()),
    ], {"windows_open": 0})
    yield case("foreign_lease", "W2: a known window with another lease is refused and left unchanged", [
        *open_window(), *open_window(1, WINDOW_B, "workspace B"),
        pipe(2, {"window_renew": {"window_id": WINDOW_A, "lease": LEASE_B}}), answer(2, failure("window_lease_invalid")),
        pipe(3, {"window_renew": lease_of(WINDOW_A)}), answer(3, {"status": "window_renewed", "ttl_ms": TTL}),
    ], {"windows_open": 2})
    yield case("open_limits", "W1: at most 64 windows; label at most 128 bytes", [
        pipe(0, {"window_open": {"label": "x" * 129, "owner_pid": 4242}}), answer(0, failure("window_label_invalid")),
        harness(1, "windows_opened", count=64),
        pipe(2, {"window_open": {"label": "x" * 128, "owner_pid": 4242}}), answer(2, failure("window_capacity")),
    ], {"windows_open": 64})
    yield case("scope_select_in_memory_grant", "W4 (U77): selects existing indexes only; nothing persisted", [
        *open_window(), session(1, 2, {"scope_select": {"selection": selection()}}), *grant(1),
        session_reply(1, 2, {"status": "scope", "scope_identity": SCOPE_1, "catalog_revision": 12}),
        session(2, 3, SEARCH), read(2), answer(2, {"status": "tool_result", "result": "elided"}),
        session_reply(2, 3, {"status": "invoked", "exit_code": 0, "class": "success", "stdout": "elided",
                             "stderr": ""}),
        pipe(3, {"window_scope_grant": {**lease_of(WINDOW_A), "selection": selection(MISSING_INDEX)}}),
        answer(3, failure("scoped_read_unavailable")),
    ], {"catalog_revision_after": 12, "grants_created": 0, "indexes_created": 0, "persisted": False,
        "active_scope": SCOPE_1})
    yield case("scope_reselect_replaces", "W4: a new grant atomically replaces the old one", [
        *open_window(), *grant(1), *grant(2, scope=SCOPE_2),
    ], {"active_scope": SCOPE_2, "revoked_scopes": [SCOPE_1]})
    yield case("scope_release", "W4b: release drops the grant; release is idempotent", [
        *open_window(), *grant(1), session(2, 2, {"scope_release": {}}),
        pipe(2, {"window_scope_release": lease_of(WINDOW_A)}), answer(2, {"status": "window_scope_released"}),
        session_reply(2, 2, {"status": "scope_released"}),
        pipe(3, {"window_scope_release": lease_of(WINDOW_A)}), answer(3, {"status": "window_scope_released"}),
        read(4), answer(4, failure("window_scope_not_granted")),
    ], {"active_scope": None})
    yield case("grant_revoked_on_service_restart", "All W state is per instance; no stale grant is reused", [
        *open_window(), *grant(1), session(2, 2, SEARCH),
        harness(2, "service_restarts", service_instance=RESTARTED), read(2),
        answer(2, failure("service_instance_changed")),
        session_reply(2, 2, refused("service_restarted", retryable=True)),
        event(2, {"kind": "resync", "reason": "service_restarted"}),
        pipe(3, {"window_renew": lease_of(WINDOW_A)}, RESTARTED), answer(3, failure("window_unknown")),
        pipe(3, {"window_open": {"label": "workspace A", "owner_pid": 4242}}, RESTARTED),
        answer(3, opened(WINDOW_A2, RESTARTED)), session(4, 3, SEARCH), read(4, WINDOW_A2, RESTARTED),
        answer(4, failure("window_scope_not_granted")), session_reply(4, 3, invoked_refusal("window_scope_not_granted")),
    ], {"active_scope": None, "automatic_regrant": False, "window_id_after": WINDOW_A2})
    yield case("grant_revoked_on_session_close", "Owner loss closes the window; process death expires it", [
        *open_window(), *grant(1), harness(2, "session_stdin_eof"), pipe(2, {"window_close": lease_of(WINDOW_A)}),
        answer(2, {"status": "window_closed"}), *open_window(3, WINDOW_B, "workspace B"), *grant(3, WINDOW_B, SCOPE_2),
        harness(4, "session_process_dies", window_id=WINDOW_B),
        mcp(30_003, {"assistance_windows": {}}), answer(30_003, windows(), "mcp"),
        pipe(30_003, {"window_scope_grant": {**lease_of(WINDOW_B), "selection": selection()}}),
        answer(30_003, failure("window_unknown")),
    ], {"windows_open": 0, "active_scope": None})
    yield case("close_drops_state", "W3: close drops the queue, handoffs and scope grant", [
        *open_window(), *grant(1), submit(1), answer(1, queued(1, 1)),
        pipe(1, {"handoff_publish": {**lease_of(WINDOW_A), "handoff": {"kind": "studio"}, "ttl_ms": 600000}}),
        answer(1, {"status": "handoff_published", "handoff_id": HANDOFF, "expires_at_ms": 600001}),
        pipe(2, {"window_close": lease_of(WINDOW_A)}), answer(2, {"status": "window_closed"}),
        pipe(3, {"explorer_handoff": {"handoff_id": HANDOFF}}), answer(3, failure("handoff_unknown")),
        submit(3), answer(3, failure("window_unknown")), read(3), answer(3, failure("window_unknown")),
    ], {"windows_open": 0, "queued": 0, "handoffs": 0})
    full = [submit(1 + n) for n in range(QUEUE_MAX)]
    yield case("assistance_queue_count_bound", "W6: at most 16 drafts; full is refused, never dropped", [
        *open_window(), *[step for n, s in enumerate(full, 1) for step in (s, answer(n, queued(n, n)))],
        submit(20), answer(20, failure("assistance_queue_full")),
        pipe(21, {"assistance_windows": {}}), answer(21, windows((WINDOW_A, "workspace A", False, QUEUE_MAX))),
    ], {"queued": QUEUE_MAX, "dropped": 0})
    yield case("assistance_queue_byte_bound", "W6: draft <= 256 KiB, window queue <= 1 MiB (compact JSON bytes)", [
        *open_window(), submit(1, draft_bytes=DRAFT_MAX + 1), answer(1, failure("assistance_draft_oversize")),
        *[step for n in range(1, 5) for step in (submit(1 + n, draft_bytes=DRAFT_MAX), answer(1 + n, queued(n, n)))],
        submit(6, draft_bytes=2), answer(6, failure("assistance_queue_full")),
    ], {"queued": 4, "queued_bytes": QUEUE_MAX_BYTES})
    yield case("drain_and_ack", "W7: drain is non-destructive; ack removes; at most `limit` unacknowledged", [
        *open_window(), *[step for n in range(1, 11) for step in (submit(n), answer(n, queued(n, n)))],
        session(11, 2, {"subscribe": {"topics": ["assistance"]}}), session_reply(11, 2, SUBSCRIBED),
        drain(11), answer(11, drained(*range(1, 9), more=9)),
        *[event(11, {"kind": "assistance_draft", "cursor": "elided", "draft": draft(s)}) for s in range(1, 9)],
        session(12, 3, {"assistance_ack": {"through": 8}}), drain(12, ack=8), answer(12, drained(9, 10)),
        session_reply(12, 3, {"status": "acknowledged"}),
        *[event(12, {"kind": "assistance_draft", "cursor": "elided", "draft": draft(s)}) for s in (9, 10)],
        drain(13, ack=8), answer(13, drained(9, 10)),
        drain(14, ack=12), answer(14, failure("assistance_ack_ahead")),
        drain(15, ack=10), answer(15, drained()),
        drain(16, limit=9), answer(16, {"status": "failure", "code": "assistance_drain_limit_invalid"}),
    ], {"queued": 0, "redelivered_events": 0, "next_sequence": 11})
    yield case("session_buffer_overflow_resync", "Overflow drops the session buffer, emits one resync, re-drains", [
        *open_window(),
        *[step for n in range(1, 5) for step in (submit(n, draft_bytes=DRAFT_MAX), answer(n, queued(n, n)))],
        session(5, 2, {"subscribe": {"topics": ["assistance"]}}), session_reply(5, 2, SUBSCRIBED),
        drain(5), answer(5, drained(1, 2, 3, 4, fill=DRAFT_MAX)),
        harness(5, "editor_stops_reading"), harness(6, "session_event_buffer_exceeds", bytes_limit=1_048_576),
        event(7, {"kind": "resync", "reason": "overflow"}), harness(7, "editor_resumes_reading"),
        drain(7), answer(7, drained(1, 2, 3, 4, fill=DRAFT_MAX)),
        *[event(7, {"kind": "assistance_draft", "cursor": "elided", "draft": draft(s, DRAFT_MAX)}) for s in range(1, 5)],
    ], {"resync_events": 1, "lost_drafts": 0, "duplicate_identity": "sequence and draft_id"})
    yield case("cross_window_refused", "Another window's lease or cursor never delivers its drafts", [
        *open_window(), *open_window(1, WINDOW_B, "workspace B"),
        mcp(2, {"assistance_submit": {"window_id": WINDOW_A, "kind": "search", "draft": {"query": "fixture"},
                                      "submitter": "codex"}}),
        answer(2, queued(1, 1), "mcp"),
        drain(3, window=WINDOW_A, owner=WINDOW_B), answer(3, failure("window_lease_invalid")),
        drain(3, window=WINDOW_B), answer(3, drained()),
        session(4, 2, {"subscribe": {"topics": ["assistance"], "cursor": {
            "service_instance": INSTANCE, "window_id": WINDOW_A,
            "native": {"namespace": NAMESPACE, "project": None, "after": 0, "through": None}}}}),
        session_reply(4, 2, refused("event_cursor_scope_mismatch")),
    ], {"queued_for": {WINDOW_A: 1, WINDOW_B: 0}})
    publish = {"handoff_publish": {"handoff": {"kind": "studio", "draft_revision": 1}, "ttl_ms": 60000}}
    yield case("handoff_publish_and_read", "W8 + M12: published through the window session; reads do not consume", [
        *open_window(), harness(0, "session_renews", every_ms=10_000), session(1, 2, publish),
        pipe(1, {"handoff_publish": {**lease_of(WINDOW_A), "handoff": {"kind": "studio", "draft_revision": 1},
                                     "ttl_ms": 60000}}),
        answer(1, {"status": "handoff_published", "handoff_id": HANDOFF, "expires_at_ms": 60001}),
        session_reply(1, 2, {"status": "handoff", "handoff_id": HANDOFF, "expires_at_ms": 60001}),
        pipe(2, {"explorer_handoff": {"handoff_id": HANDOFF}}),
        answer(2, {"status": "explorer_handoff", "handoff": {"kind": "studio", "draft_revision": 1}}),
        pipe(60_000, {"explorer_handoff": {"handoff_id": HANDOFF}}),
        answer(60_000, {"status": "explorer_handoff", "handoff": {"kind": "studio", "draft_revision": 1}}),
        pipe(60_001, {"explorer_handoff": {"handoff_id": HANDOFF}}), answer(60_001, failure("handoff_unknown")),
    ], {"handoffs": 0})
    yield case("handoff_limits", "W8: ttl 1..=600000 ms, <= 64 KiB, <= 32 per window", [
        *open_window(),
        pipe(1, {"handoff_publish": {**lease_of(WINDOW_A), "handoff": {}, "ttl_ms": 600001}}),
        answer(1, failure("handoff_ttl_invalid")),
        pipe(1, {"handoff_publish": {**lease_of(WINDOW_A), "handoff": {"$fill_bytes": 65537}, "ttl_ms": 1000}}),
        answer(1, failure("handoff_oversize")), harness(2, "handoffs_published", window_id=WINDOW_A, count=32),
        pipe(3, {"handoff_publish": {**lease_of(WINDOW_A), "handoff": {}, "ttl_ms": 1000}}),
        answer(3, failure("handoff_capacity")),
    ], {"handoffs": 32})
    # The editor never holds a lease, so its attempt carries a guess; refusal is by method, before the pipe.
    guess = {"window_id": WINDOW_A, "lease": "ff" * 32}
    relayed = [{"window_open": {"label": "x", "owner_pid": 1}}, {"window_renew": guess}, {"window_close": guess},
               {"window_scope_grant": {**guess, "selection": selection()}}, {"window_scope_release": guess},
               {"assistance_drain": {**guess, "ack_through": None, "limit": 8}},
               {"handoff_publish": {**guess, "handoff": {}, "ttl_ms": 1000}}]
    yield case("relay_refuses_lease_methods", "Lease-bearing methods never cross the session control relay", [
        *open_window(),
        *[step for n, method in enumerate(relayed, 2) for step in (
            session(1, n, {"control": {"method": method}}), session_reply(1, n, refused("session_command_refused")))],
        session(2, 9, {"control": {"method": {"assistance_windows": {}}}}), pipe(2, {"assistance_windows": {}}),
        answer(2, windows((WINDOW_A, "workspace A", True, 0))),
        session_reply(2, 9, {"status": "control", "response": windows((WINDOW_A, "workspace A", True, 0))}),
    ], {"pipe_requests_from_relay": ["assistance_windows"]})


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
        "contract": "docs/split/contracts.md §2.5; split plan §5.2, §6.5; U10, U77",
        "generator": "fixtures/split/i1/window_plane.py",
        "limits": {"windows": 64, "label_bytes": 128, "ttl_ms": TTL, "renew_every_ms": 10_000,
                   "drafts_per_window": QUEUE_MAX, "queue_bytes": QUEUE_MAX_BYTES, "draft_bytes": DRAFT_MAX,
                   "drain_limit": 8, "handoffs_per_window": 32, "handoff_bytes": 65_536, "handoff_ttl_ms": 600_000},
        "conventions": [
            "`$fill_bytes: N` stands for a JSON value whose compact serialization is exactly N bytes.",
            "`elided` stands for a value pinned elsewhere (read budgets, tool results, event cursors).",
            "Session frames scope_select, scope_release, assistance_ack and handoff_publish follow §3.7 [P].",
        ],
        "secrets": {"lease_by_window": WINDOWS},
        "secret_channels": {"lease": ["pipe"]},
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
