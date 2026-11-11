# SPDX-License-Identifier: Apache-2.0
"""Generate the I1 admin launch-code cases A1 (split C3b).

A1 issues a single-use admin launch code over the owner pipe only
(docs/split/contracts.md §2.7, §3.8, §4.9). Consumption is the service-internal
atomic compare-and-remove that the §4.5 session exchange calls; its HTTP
transport, headers and statuses are pinned separately by the C5 fixtures and
are not restated here. Expectations are reviewed statements of the contract,
written independently of any implementation; this file computes the stored
SHA-256 with hashlib.

A code may appear only in the A1 response on the pipe, in the stdout of the
one-shot `airkctl admin url|open` (as the URL fragment) and in the consumption
input. It never appears in a session frame, an HTTP request line or response,
a log record, a `Debug` rendering or an error, and neither does its SHA-256.

    python fixtures/split/i1/admin_launch_code.py          # rewrite the JSON
    python fixtures/split/i1/admin_launch_code.py --check  # exit 1 on drift
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import sys

OUTPUT = Path(__file__).with_name("admin_launch_code.json")
SCHEMA = "airk.split-fixture.i1-admin-launch-code/1"
INSTANCE = "e1" * 16
RESTARTED = "e2" * 16
PORT = 41234
TTL = 60_000
OUTSTANDING = 8
INVALID = "admin_launch_code_invalid"


def code(number):
    """Fixture stand-in for 32 random bytes, rendered base64url without padding (43 characters)."""
    return base64.urlsafe_b64encode(bytes([number]) * 32).decode("ascii").rstrip("=")


def stored(value):
    return hashlib.sha256(value.encode("ascii")).hexdigest()


def pipe(at, method, instance=INSTANCE):
    return {"at_ms": at, "channel": "pipe", "send": {"protocol": 1, "expected_instance": instance, "method": method}}


def issued(at, number):
    return [pipe(at, {"admin_launch_code": {}}),
            {"at_ms": at, "channel": "pipe", "reply": {"status": "admin_launch_code", "code": code(number),
                                                        "port": PORT, "expires_in_ms": TTL}},
            {"at_ms": at, "service_stores": {"sha256": stored(code(number)), "issued_at_ms": at}}]


def consume(at, value, result):
    outcome = {"result": "session_created"} if result else {"result": "refused", "code": INVALID}
    return {"at_ms": at, "consume": value, "expect": outcome}


def session(at, request, code_name):
    return [{"at_ms": at, "channel": "session", "send": {"id": "01" * 16, "request": request}},
            {"at_ms": at, "channel": "session", "reply": {"id": "01" * 16, "outcome": {
                "status": "failed", "code": code_name, "retryable": False}}}]


def case(name, rule, transcript, expected):
    return {"name": name, "rule": rule, "transcript": transcript, "expected": expected}


def cases():
    yield case("issue_over_owner_pipe", "A1 on the owner pipe; the service keeps only the SHA-256", [
        *issued(0, 1),
    ], {"outstanding": 1, "issued_codes": [1]})
    yield case("one_shot_url", "`airkctl admin url` prints the code only as the URL fragment", [
        *issued(0, 1),
        {"at_ms": 0, "channel": "stdout", "write": f"https://127.0.0.1:{PORT}/admin/launch#c={code(1)}\n"},
    ], {"outstanding": 1, "issued_codes": [1]})
    yield case("single_use", "Consumption is one atomic compare-and-remove; a replay is refused", [
        *issued(0, 1), consume(1_000, code(1), True), consume(1_001, code(1), False),
    ], {"outstanding": 0, "sessions_created": 1})
    yield case("expiry", "Valid while now < issued + 60000 ms (injected clock)", [
        *issued(0, 1), *issued(0, 2), consume(TTL - 1, code(1), True), consume(TTL, code(2), False),
    ], {"outstanding": 0, "sessions_created": 1})
    yield case("unknown_and_malformed", "Unknown, padded, short or non-canonical codes get the same refusal (no oracle)", [
        *issued(0, 1), consume(1, code(9), False), consume(1, code(1) + "=", False),
        consume(1, code(1)[:-1], False), consume(1, code(1)[:-1] + "F", False), consume(1, "", False),
    ], {"outstanding": 1, "sessions_created": 0})
    yield case("oldest_evicted", "At most 8 outstanding; the ninth issuance evicts the oldest", [
        *[step for number in range(1, OUTSTANDING + 2) for step in issued(number, number)],
        consume(20, code(1), False), consume(21, code(2), True), consume(22, code(OUTSTANDING + 1), True),
    ], {"outstanding": OUTSTANDING - 2, "sessions_created": 2})
    yield case("restart_invalidates", "Codes live in memory and die with the service instance", [
        *issued(0, 1), {"at_ms": 1, "harness": "service_restarts", "service_instance": RESTARTED},
        consume(2, code(1), False),
    ], {"outstanding": 0, "sessions_created": 0})
    yield case("no_request_fields", "A1 takes no fields; an unknown field is refused before dispatch", [
        pipe(0, {"admin_launch_code": {"ttl_ms": 600000}}),
        {"at_ms": 0, "channel": "pipe", "refused_before_dispatch": True},
    ], {"outstanding": 0, "issued_codes": []})
    yield case("refused_on_session_relay", "Never relayed by `airkctl session` (§3.8)", [
        *session(0, {"control": {"method": {"admin_launch_code": {}}}}, "session_command_refused"),
        *session(1, {"invoke": {"argv": ["admin", "url"], "deadline_ms": 10000}}, "session_command_refused"),
        *session(2, {"invoke": {"argv": ["admin", "open"], "deadline_ms": 10000}}, "session_command_refused"),
    ], {"outstanding": 0, "pipe_requests": []})
    yield case("refused_over_http", "No HTTP route reaches A1 (§4.9); nothing is issued", [
        {"at_ms": 0, "channel": "http", "send": {"method": "POST", "path": "/admin/api/v1/call/admin.launch_code"},
         "expect": {"issued": False, "route": "none"}},
        {"at_ms": 1, "channel": "http", "send": {"method": "POST", "path": "/api/v1/tools/admin_launch_code"},
         "expect": {"issued": False, "route": "none"}},
        {"at_ms": 2, "channel": "http", "send": {"method": "GET", "path": "/admin/launch"},
         "expect": {"issued": False, "route": "static entry page"}},
    ], {"outstanding": 0, "pipe_requests": []})
    yield case("never_logged", "Neither the code nor its SHA-256 reaches logs, Debug output or errors", [
        *issued(0, 1), consume(1, code(1), True), consume(2, code(1), False),
        {"at_ms": 3, "inspect": ["service_log", "debug_rendering", "error_messages"]},
    ], {"forbidden_substrings": [code(1), stored(code(1))]})


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
        "contract": "docs/split/contracts.md §2.7, §3.8, §4.5 step 3, §4.9",
        "generator": "fixtures/split/i1/admin_launch_code.py",
        "limits": {"code_random_bytes": 32, "code_chars": 43, "ttl_ms": TTL, "outstanding": OUTSTANDING},
        "code_format": "base64url without padding of 32 random bytes; stored as SHA-256 of the 43 ASCII bytes",
        "secrets": {"launch_code": [code(n) for n in range(1, OUTSTANDING + 2)]},
        "secret_channels": {"launch_code": ["pipe A1 reply", "stdout of the one-shot admin url|open", "consume"]},
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
