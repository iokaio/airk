# SPDX-License-Identifier: Apache-2.0
"""Generate the I3 admin HTTP contract fixtures (split C5).

Writes, beside this file: admission_cases.json (Host/Origin/Fetch-Metadata,
peer, cookie, session and path rules), launch_exchange.json (code exchange and
the H1 peer cases), headers.json, envelopes.json and admin_operations.json.

Every expectation below is a reviewed statement of docs/split/contracts.md §4
as amended by §4.11 and docs/split/security-review-i3.md (H1, M1, M2, M3 CSP,
L1-L9, I3-I5), written literally per case and independently of the Rust code
in crates/airk-cli/src/admin_contract/. Nothing here evaluates admission
rules; expected outcomes are typed by hand.

    python fixtures/split/i3/admission_cases.py          # rewrite the JSON
    python fixtures/split/i3/admission_cases.py --check  # exit 1 on drift
"""
import argparse
import base64
import json
from pathlib import Path
import sys

HERE = Path(__file__).parent
GENERATOR = "fixtures/split/i3/admission_cases.py"
CONTRACT = "docs/split/contracts.md §4, §4.11; docs/split/security-review-i3.md"
PORT = 57421
HOST = "127.0.0.1:57421"
ORIGIN = "https://127.0.0.1:57421"
NOW = 50_000_000
COOKIE = base64.urlsafe_b64encode(bytes(range(32))).rstrip(b"=").decode()
CODE = base64.urlsafe_b64encode(bytes(range(32, 64))).rstrip(b"=").decode()
CSRF = base64.urlsafe_b64encode(bytes(range(64, 96))).rstrip(b"=").decode()
INSTANCE = "e1" * 16
JAR = "__Host-airk_admin=" + COOKIE


def admit(route, touches_idle=False):
    return {"outcome": "admit", "route": route, "touches_idle": touches_idle}


def refuse(status, code):
    return {"outcome": "refuse", "status": status, "code": code}


NOT_ADMIN = {"outcome": "not_admin"}
HOST_REFUSED = refuse(403, "admin_host_refused")
ORIGIN_REFUSED = refuse(403, "admin_origin_refused")
SITE_REFUSED = refuse(403, "admin_fetch_metadata_refused")
PEER_REFUSED = refuse(403, "admin_peer_refused")
PREFLIGHT = refuse(403, "admin_preflight_refused")
BEARER = refuse(401, "admin_bearer_refused")
SESSION_INVALID = refuse(401, "admin_session_invalid")
NOT_FOUND = refuse(404, "admin_not_found")
METHOD = refuse(405, "admin_method_not_allowed")
UPGRADE = refuse(400, "admin_upgrade_refused")
MALFORMED = refuse(400, "admin_malformed_request")
TOO_LARGE = refuse(413, "admin_request_too_large")
SHELL, LAUNCH, REDIRECT = {"kind": "shell"}, {"kind": "launch"}, {"kind": "redirect"}
EXCHANGE, LOGOUT, EVENTS = {"kind": "exchange"}, {"kind": "logout"}, {"kind": "events"}
STOP = {"kind": "call", "operation": "service.stop"}
LIVE = {"created_at_ms": NOW - 60_000, "last_used_at_ms": NOW - 1_000, "same_instance": True, "token_matches": True}


def page(target, **fields):
    """A top-level navigation launched by the OS opener."""
    request = {"listener_port": PORT, "now_ms": NOW, "method": "GET", "target": target, "host": [HOST],
               "sec_fetch_site": ["none"], "sec_fetch_mode": "navigate", "sec_fetch_dest": "document",
               "peer": "unavailable"}
    request.update(fields)
    return request


def api(target, **fields):
    """A same-origin fetch from the shell with a live session."""
    request = {"listener_port": PORT, "now_ms": NOW, "method": "POST", "target": target, "host": [HOST],
               "origin": [ORIGIN], "sec_fetch_site": ["same-origin"], "sec_fetch_mode": "cors",
               "sec_fetch_dest": "empty", "content_type": ["application/json"], "content_length": 2,
               "cookie": [JAR], "csrf_headers": 1, "peer": "same_user"}
    request.update(fields)
    return request


def exchange_request(**fields):
    request = api("/admin/api/v1/session", cookie=[], csrf_headers=0, content_length=60)
    request.update(fields)
    return request


def case(name, request, expect, session=None):
    return {"name": name, "request": request, "session": session, "expect": expect}


def admission_cases():
    call = "/admin/api/v1/call/service.stop"
    # Static pages: no data, no session, no peer proof.
    yield case("shell_top_level_navigation", page("/admin/"), admit(SHELL))
    yield case("shell_same_origin_reload", page("/admin/", sec_fetch_site=["same-origin"]), admit(SHELL))
    yield case("shell_without_fetch_metadata", page("/admin/", sec_fetch_site=[]), admit(SHELL))
    yield case("shell_same_site_refused", page("/admin/", sec_fetch_site=["same-site"]), SITE_REFUSED)
    yield case("shell_cross_site_refused", page("/admin/", sec_fetch_site=["cross-site"]), SITE_REFUSED)
    yield case("shell_unknown_fetch_site_refused", page("/admin/", sec_fetch_site=["Same-Origin"]), SITE_REFUSED)
    yield case("shell_two_fetch_site_headers_refused", page("/admin/", sec_fetch_site=["none", "none"]), SITE_REFUSED)
    yield case("launch_from_redirect_file_cross_site", page("/admin/launch", sec_fetch_site=["cross-site"]),
               admit(LAUNCH))
    yield case("launch_top_level_navigation", page("/admin/launch"), admit(LAUNCH))
    yield case("launch_same_site_refused", page("/admin/launch", sec_fetch_site=["same-site"]), SITE_REFUSED)
    yield case("asset_from_shell", page("/admin/assets/app.3f2a9c.js", sec_fetch_site=["same-origin"],
                                        sec_fetch_mode="no-cors", sec_fetch_dest="script"),
               admit({"kind": "asset", "name": "app.3f2a9c.js"}))
    yield case("asset_cross_site_refused", page("/admin/assets/app.3f2a9c.js", sec_fetch_site=["cross-site"]),
               SITE_REFUSED)
    yield case("redirect_without_trailing_slash", page("/admin"), admit(REDIRECT))
    yield case("static_exact_origin_allowed", page("/admin/", origin=[ORIGIN], sec_fetch_site=["same-origin"]),
               admit(SHELL))
    yield case("static_other_port_origin_refused", page("/admin/", origin=["https://127.0.0.1:3000"]),
               ORIGIN_REFUSED)
    yield case("static_null_origin_refused", page("/admin/", origin=["null"]), ORIGIN_REFUSED)
    yield case("static_bearer_refused", page("/admin/", authorization=1), BEARER)
    yield case("static_needs_no_peer_proof", page("/admin/", peer="other_user"), admit(SHELL))
    yield case("static_ignores_cookies", page("/admin/", cookie=[JAR, JAR]), admit(SHELL))
    yield case("static_post_not_allowed", page("/admin/", method="POST"), METHOD)
    yield case("static_head_not_allowed", page("/admin/", method="HEAD"), METHOD)
    yield case("static_lowercase_method_not_allowed", page("/admin/", method="get"), METHOD)
    yield case("static_preflight_refused", page("/admin/", method="OPTIONS"), PREFLIGHT)
    yield case("shell_websocket_upgrade_refused", page("/admin/", upgrade=True), UPGRADE)
    # Host: exact single 127.0.0.1:<port> (DNS rebinding).
    yield case("host_other_port_refused", page("/admin/", host=["127.0.0.1:57422"]), HOST_REFUSED)
    yield case("host_localhost_name_refused", page("/admin/", host=["localhost:57421"]), HOST_REFUSED)
    yield case("host_twice_refused", page("/admin/", host=[HOST, HOST]), HOST_REFUSED)
    yield case("host_missing_refused", page("/admin/", host=[]), HOST_REFUSED)
    yield case("rebound_name_on_api_refused", api(call, host=["attacker.example:57421"]), HOST_REFUSED,
               session=LIVE)
    yield case("host_checked_before_unknown_path", page("/admin/nothing", host=["attacker.example"]), HOST_REFUSED)
    # Session exchange: the code is checked separately (launch_exchange.json).
    yield case("exchange_same_origin", exchange_request(), admit(EXCHANGE))
    yield case("exchange_cross_origin_refused", exchange_request(origin=["https://evil.example"],
                                                                 sec_fetch_site=["cross-site"]), ORIGIN_REFUSED)
    yield case("exchange_missing_origin_refused", exchange_request(origin=[]), ORIGIN_REFUSED)
    yield case("exchange_same_site_refused", exchange_request(sec_fetch_site=["same-site"]), SITE_REFUSED)
    yield case("exchange_missing_fetch_site_refused", exchange_request(sec_fetch_site=[]), SITE_REFUSED)
    yield case("exchange_navigation_site_none_refused", exchange_request(sec_fetch_site=["none"]), SITE_REFUSED)
    yield case("exchange_bearer_refused", exchange_request(authorization=1), BEARER)
    yield case("exchange_text_plain_refused", exchange_request(content_type=["text/plain"]), MALFORMED)
    yield case("exchange_json_with_charset", exchange_request(content_type=["application/json; charset=utf-8"]),
               admit(EXCHANGE))
    yield case("exchange_two_content_types_refused",
               exchange_request(content_type=["application/json", "application/json"]), MALFORMED)
    yield case("exchange_body_exactly_1_kib", exchange_request(content_length=1024), admit(EXCHANGE))
    yield case("exchange_body_over_1_kib_refused", exchange_request(content_length=1025), TOO_LARGE)
    yield case("exchange_streamed_body_admitted_reader_bounds_it", exchange_request(content_length=None),
               admit(EXCHANGE))
    yield case("exchange_ignores_presented_cookies", exchange_request(cookie=[JAR, JAR]), admit(EXCHANGE))
    yield case("exchange_get_not_allowed", exchange_request(method="GET"), METHOD)
    yield case("exchange_preflight_refused", exchange_request(method="OPTIONS"), PREFLIGHT)
    # Authenticated API calls.
    yield case("call_with_live_session", api(call), admit(STOP, True), session=LIVE)
    yield case("call_cross_origin_refused", api(call, origin=["https://evil.example"], sec_fetch_site=["cross-site"]),
               ORIGIN_REFUSED, session=LIVE)
    yield case("call_other_127_port_page_refused", api(call, origin=["https://127.0.0.1:3000"],
                                                       sec_fetch_site=["same-site"]), ORIGIN_REFUSED, session=LIVE)
    yield case("call_cross_site_form_post_refused",
               api(call, origin=["https://evil.example"], sec_fetch_site=["cross-site"], sec_fetch_mode="navigate",
                   content_type=["application/x-www-form-urlencoded"], csrf_headers=0), ORIGIN_REFUSED, session=LIVE)
    yield case("call_missing_origin_refused", api(call, origin=[]), ORIGIN_REFUSED, session=LIVE)
    yield case("call_missing_fetch_metadata_refused", api(call, sec_fetch_site=[]), SITE_REFUSED, session=LIVE)
    yield case("call_same_site_with_exact_origin_refused", api(call, sec_fetch_site=["same-site"]), SITE_REFUSED,
               session=LIVE)
    yield case("call_peer_unavailable_refused", api(call, peer="unavailable"), PEER_REFUSED, session=LIVE)
    yield case("call_bearer_refused", api(call, authorization=1), BEARER, session=LIVE)
    yield case("call_bearer_checked_after_peer", api(call, authorization=1, peer="other_user"), PEER_REFUSED,
               session=LIVE)
    # One uniform 401 (M2 rule 3).
    yield case("call_without_cookie", api(call, cookie=[]), SESSION_INVALID)
    yield case("call_two_cookies_one_header", api(call, cookie=[JAR + "; " + JAR]), SESSION_INVALID, session=LIVE)
    yield case("call_two_cookies_two_headers", api(call, cookie=[JAR, JAR]), SESSION_INVALID, session=LIVE)
    yield case("call_unknown_cookie", api(call), SESSION_INVALID)
    yield case("call_wrong_token", api(call), SESSION_INVALID, session={**LIVE, "token_matches": False})
    yield case("call_missing_token_header", api(call, csrf_headers=0), SESSION_INVALID, session=LIVE)
    yield case("call_two_token_headers", api(call, csrf_headers=2), SESSION_INVALID, session=LIVE)
    yield case("call_idle_expired_at_30_min", api(call), SESSION_INVALID,
               session={**LIVE, "last_used_at_ms": NOW - 1_800_000})
    yield case("call_idle_just_inside_30_min", api(call), admit(STOP, True),
               session={**LIVE, "last_used_at_ms": NOW - 1_799_999})
    yield case("call_absolute_expired_at_12_h", api(call), SESSION_INVALID,
               session={**LIVE, "created_at_ms": NOW - 43_200_000})
    yield case("call_absolute_just_inside_12_h", api(call), admit(STOP, True),
               session={**LIVE, "created_at_ms": NOW - 43_199_999})
    yield case("call_session_of_previous_instance", api(call), SESSION_INVALID,
               session={**LIVE, "same_instance": False})
    yield case("call_cookie_name_case_differs", api(call, cookie=["__host-airk_admin=" + COOKIE]), SESSION_INVALID,
               session=LIVE)
    yield case("call_other_cookies_ignored", api(call, cookie=["theme=dark; " + JAR + "; lang=en"]),
               admit(STOP, True), session=LIVE)
    yield case("call_session_checked_before_content_type", api(call, cookie=[], content_type=["text/plain"]),
               SESSION_INVALID)
    yield case("call_text_plain_refused", api(call, content_type=["text/plain"]), MALFORMED, session=LIVE)
    yield case("call_body_exactly_64_kib", api(call, content_length=65_536), admit(STOP, True), session=LIVE)
    yield case("call_body_over_64_kib_refused", api(call, content_length=65_537), TOO_LARGE, session=LIVE)
    yield case("call_get_not_allowed", api(call, method="GET"), METHOD, session=LIVE)
    yield case("call_preflight_refused", api(call, method="OPTIONS"), PREFLIGHT, session=LIVE)
    yield case("call_uppercase_operation_not_found", api("/admin/api/v1/call/Service.Stop"), NOT_FOUND, session=LIVE)
    yield case("call_trailing_slash_not_found", api(call + "/"), NOT_FOUND, session=LIVE)
    yield case("events_do_not_renew_idle", api("/admin/api/v1/events"), admit(EVENTS), session=LIVE)
    yield case("events_websocket_upgrade_refused", api("/admin/api/v1/events", method="GET", upgrade=True), UPGRADE,
               session=LIVE)
    yield case("events_query_not_found", api("/admin/api/v1/events?cursor=1"), NOT_FOUND, session=LIVE)
    yield case("logout_with_live_session", api("/admin/api/v1/session/logout"), admit(LOGOUT), session=LIVE)
    yield case("logout_without_session", api("/admin/api/v1/session/logout", cookie=[]), SESSION_INVALID)
    # The bearer plane and unrelated paths stay with the existing pipeline.
    yield case("mcp_is_not_admin", api("/mcp", cookie=[JAR]), NOT_ADMIN, session=LIVE)
    yield case("api_v1_is_not_admin", api("/api/v1/tools/search"), NOT_ADMIN)
    yield case("absolute_form_is_not_admin", page("https://127.0.0.1:57421/admin/"), NOT_ADMIN)
    yield case("uppercase_admin_is_not_admin", page("/ADMIN/"), NOT_ADMIN)
    yield case("admin_prefixed_word_not_found", page("/adminx"), NOT_FOUND)
    yield case("traversal_to_mcp_not_found", api("/admin/../mcp"), NOT_FOUND, session=LIVE)
    yield case("encoded_dot_not_found", page("/admin/%2e%2e/mcp"), NOT_FOUND)


def path(target, expect):
    return {"target": target, "expect": expect}


def route(value):
    return {"match": "route", "route": value}


def path_cases():
    gone = {"match": "not_found"}
    other = {"match": "not_admin"}
    yield path("/admin", route(REDIRECT))
    yield path("/admin/", route(SHELL))
    yield path("/admin/launch", route(LAUNCH))
    yield path("/admin/assets/app.3f2a9c.js", route({"kind": "asset", "name": "app.3f2a9c.js"}))
    yield path("/admin/assets/inter-latin_400.woff2", route({"kind": "asset", "name": "inter-latin_400.woff2"}))
    yield path("/admin/assets/" + "a" * 128, route({"kind": "asset", "name": "a" * 128}))
    yield path("/admin/api/v1/session", route(EXCHANGE))
    yield path("/admin/api/v1/session/logout", route(LOGOUT))
    yield path("/admin/api/v1/call/storage.purge", route({"kind": "call", "operation": "storage.purge"}))
    yield path("/admin/api/v1/call/catalog.index_setup_draft",
               route({"kind": "call", "operation": "catalog.index_setup_draft"}))
    yield path("/admin/api/v1/call/" + "a" * 30 + "." + "b" * 33, route({"kind": "call",
                                                                       "operation": "a" * 30 + "." + "b" * 33}))
    yield path("/admin/api/v1/events", route(EVENTS))
    for target in ("/admin//", "/admin/launch/", "/admin/launch?c=x", "/admin/launch#c=x", "/admin/assets/",
                   "/admin/assets/../secret", "/admin/assets/.hidden", "/admin/assets/app.js.", "/admin/assets/a..b.js",
                   "/admin/assets/App.js", "/admin/assets/sub/app.js", "/admin/assets/%2e%2e/x", "/admin/%2e%2e/mcp",
                   "/admin/%2E%2E/mcp", "/admin/%252e/x", "/admin\\launch", "/admin/assets/app.js;v=1",
                   "/admin/launch\u0000", "/admin/lаunch", "/admin/./launch", "/admin/api/v1/call/",
                   "/admin/api/v1/call/stop", "/admin/api/v1/call/service..stop", "/admin/api/v1/call/service.stop.",
                   "/admin/api/v1/call/1service.stop", "/admin/api/v1/call/service.Stop",
                   "/admin/api/v1/call/" + "a" * 32 + "." + "b" * 32, "/admin/api/v2/session",
                   "/admin/API/v1/session", "/adminx", "/admin-x", "/admin/assets/" + "a" * 129,
                   "/admin/api/v1/session/", "/admin/api/v1/call/service.stop/extra", "/admin/~/launch"):
        yield path(target, gone)
    for target in ("/mcp", "/api/v1/tools", "/", "*", "https://127.0.0.1:57421/admin/", "/ADMIN", "", "/Admin/"):
        yield path(target, other)


def cookie_cases():
    yield {"headers": [], "admin_cookies": 0}
    yield {"headers": [JAR], "admin_cookies": 1}
    yield {"headers": ["a=1; " + JAR + "; b=2"], "admin_cookies": 1}
    yield {"headers": [JAR + "; __Host-airk_admin=other"], "admin_cookies": 2}
    yield {"headers": [JAR, "__Host-airk_admin=other"], "admin_cookies": 2}
    yield {"headers": ["__host-airk_admin=" + COOKIE], "admin_cookies": 0}
    yield {"headers": ["airk_admin=" + COOKIE], "admin_cookies": 0}
    yield {"headers": ["__Host-airk_admin"], "admin_cookies": 0}
    yield {"headers": ["__Host-airk_admin="], "admin_cookies": 1}
    yield {"headers": ["  __Host-airk_admin =" + COOKIE + ";"], "admin_cookies": 1}
    yield {"headers": ["x=__Host-airk_admin=" + COOKIE], "admin_cookies": 0}


def bearer_plane_cases():
    yield {"headers": [JAR], "expect": {"status": 403, "code": "cookie_on_bearer_plane"}}
    yield {"headers": ["session=abc; theme=dark"], "expect": None}
    yield {"headers": ["__host-airk_admin=" + COOKIE], "expect": None}
    yield {"headers": [], "expect": None}


def peer_cases():
    sid, other_sid = "S-1-5-21-1000-2000-3000-1001", "S-1-5-21-1000-2000-3000-1002"
    yield {"observation": {"platform": "windows", "service_sid": sid, "peer_sid": sid}, "expect": "same_user"}
    yield {"observation": {"platform": "windows", "service_sid": sid, "peer_sid": other_sid}, "expect": "other_user"}
    yield {"observation": {"platform": "windows", "service_sid": sid, "peer_sid": None}, "expect": "unavailable"}
    yield {"observation": {"platform": "linux", "service_uid": 1000, "peer_uid": 1000}, "expect": "same_user"}
    yield {"observation": {"platform": "linux", "service_uid": 1000, "peer_uid": 0}, "expect": "other_user"}
    yield {"observation": {"platform": "linux", "service_uid": 1000, "peer_uid": None}, "expect": "unavailable"}
    yield {"observation": {"platform": "macos", "found_among_service_user_sockets": True}, "expect": "same_user"}
    yield {"observation": {"platform": "macos", "found_among_service_user_sockets": False}, "expect": "other_user"}
    yield {"observation": {"platform": "macos", "found_among_service_user_sockets": None}, "expect": "unavailable"}


def admission_document():
    return {
        "schema": "airk.split-fixture.i3-admission/1",
        "contract": CONTRACT,
        "generator": GENERATOR,
        "port": PORT,
        "limits": {"launch_code_ttl_ms": 60_000, "session_idle_ms": 1_800_000, "session_absolute_ms": 43_200_000,
                   "max_sessions": 16, "max_exchange_body_bytes": 1024, "max_request_body_bytes": 65_536,
                   "max_response_bytes": 33_554_432, "max_streams_per_session": 2, "max_streams": 16,
                   "stream_buffer_events": 256, "stream_buffer_bytes": 1_048_576, "stream_seconds": 120,
                   "max_asset_name_bytes": 128, "max_operation_bytes": 64, "secret_text_chars": 43,
                   "max_confirm_text_bytes": 256, "max_reviews_per_session": 32},
        "conventions": ["`session` is the lookup result for the single admin cookie, or null when unknown.",
                        "Cookie values are fixture placeholders, never real secrets."],
        "cases": list(admission_cases()),
        "paths": list(path_cases()),
        "cookies": list(cookie_cases()),
        "bearer_plane": list(bearer_plane_cases()),
        "peers": list(peer_cases()),
    }


def exchange_case(name, body, stored, expect, now=NOW):
    return {"name": name, "body": body, "stored": stored, "now_ms": now, "expect": expect}


def launch_document():
    accepted = {"outcome": "accepted"}
    invalid = {"outcome": "refused", "status": 401, "code": "admin_launch_code_invalid"}
    malformed = {"outcome": "refused", "status": 400, "code": "admin_malformed_request"}
    fresh = {"issued_at_ms": NOW - 1_000, "same_instance": True}
    body = json.dumps({"code": CODE})
    exchanges = [
        exchange_case("fresh_code_accepted", body, fresh, accepted),
        exchange_case("accepted_just_inside_60_s", body, {"issued_at_ms": NOW - 59_999, "same_instance": True},
                      accepted),
        exchange_case("expired_at_60_s", body, {"issued_at_ms": NOW - 60_000, "same_instance": True}, invalid),
        exchange_case("unknown_code", body, None, invalid),
        exchange_case("replay_after_consumption", body, None, invalid),
        exchange_case("code_of_previous_instance", body, {**fresh, "same_instance": False}, invalid),
        exchange_case("short_code", json.dumps({"code": CODE[:42]}), fresh, invalid),
        exchange_case("non_canonical_final_character", json.dumps({"code": "A" * 42 + "B"}), fresh, invalid),
        exchange_case("padded_code", json.dumps({"code": CODE + "="}), fresh, invalid),
        exchange_case("standard_base64_alphabet", json.dumps({"code": "+" + CODE[1:]}), fresh, invalid),
        exchange_case("empty_code", json.dumps({"code": ""}), None, invalid),
        exchange_case("unknown_field", json.dumps({"code": CODE, "cookie": COOKIE}), fresh, malformed),
        exchange_case("missing_code", "{}", fresh, malformed),
        exchange_case("code_not_a_string", json.dumps({"code": 5}), fresh, malformed),
        exchange_case("array_body", "[]", fresh, malformed),
        exchange_case("not_json", "c=" + CODE, fresh, malformed),
        exchange_case("body_over_1_kib", json.dumps({"code": CODE, "pad": "x" * 1024}), fresh,
                      {"outcome": "refused", "status": 413, "code": "admin_request_too_large"}),
    ]
    # H1 fixtures required by the review: forged browser headers from another
    # local user's process, the same user, an unavailable observer, and a
    # foreign peer holding a valid cookie and token.
    forged = exchange_request(peer="other_user")
    admission = [
        case("forged_headers_from_foreign_peer", forged, PEER_REFUSED),
        case("same_user_peer_accepted", exchange_request(), admit(EXCHANGE)),
        case("peer_observer_unavailable", exchange_request(peer="unavailable"), PEER_REFUSED),
        case("foreign_peer_with_valid_cookie_and_token", api("/admin/api/v1/call/service.stop", peer="other_user"),
             PEER_REFUSED, session=LIVE),
    ]
    return {
        "schema": "airk.split-fixture.i3-launch-exchange/1",
        "contract": CONTRACT,
        "generator": GENERATOR,
        "launch_url": {"port": PORT, "code": CODE, "url": "https://127.0.0.1:57421/admin/launch#c=" + CODE},
        "exchanges": exchanges,
        "admission": admission,
        "responses": [{"name": "exchange_result", "json": {"csrf": CSRF, "idle_expires_in_ms": 1_800_000,
                                                           "absolute_expires_in_ms": 43_200_000,
                                                           "service_instance": INSTANCE}}],
        "invalid_responses": [
            {"name": "unknown_field", "json": {"csrf": CSRF, "idle_expires_in_ms": 1, "absolute_expires_in_ms": 1,
                                               "service_instance": INSTANCE, "cookie": COOKIE}},
            {"name": "missing_csrf", "json": {"idle_expires_in_ms": 1, "absolute_expires_in_ms": 1,
                                              "service_instance": INSTANCE}},
        ],
        "rejected_responses": [
            {"name": "short_csrf", "reason": "exchange_csrf",
             "json": {"csrf": CSRF[:40], "idle_expires_in_ms": 1, "absolute_expires_in_ms": 1,
                      "service_instance": INSTANCE}},
            {"name": "uppercase_instance", "reason": "exchange_service_instance",
             "json": {"csrf": CSRF, "idle_expires_in_ms": 1, "absolute_expires_in_ms": 1,
                      "service_instance": INSTANCE.upper()}},
        ],
    }


CSP = ("default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; font-src 'self'; "
       "connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; "
       "require-trusted-types-for 'script'; trusted-types 'none'")
PERMISSIONS = ("accelerometer=(), autoplay=(), camera=(), display-capture=(), geolocation=(), gyroscope=(), "
               "magnetometer=(), microphone=(), midi=(), payment=(), publickey-credentials-get=(), "
               "screen-wake-lock=(), serial=(), usb=(), xr-spatial-tracking=()")


def headers_document():
    security = [["content-security-policy", CSP], ["x-content-type-options", "nosniff"],
                ["referrer-policy", "no-referrer"], ["x-frame-options", "DENY"],
                ["cross-origin-opener-policy", "same-origin"], ["cross-origin-resource-policy", "same-origin"],
                ["permissions-policy", PERMISSIONS]]
    no_store = [["cache-control", "no-store"]]
    return {
        "schema": "airk.split-fixture.i3-headers/1",
        "contract": CONTRACT,
        "generator": GENERATOR,
        "comparison": "Header sets are compared as sorted [name, value] pairs.",
        "kinds": {
            "api": security + no_store + [["content-type", "application/json"]],
            "events": security + no_store + [["content-type", "application/x-ndjson"]],
            "shell": security + no_store + [["content-type", "text/html; charset=utf-8"]],
            "asset": security + [["cache-control", "private, max-age=31536000, immutable"]],
            "redirect": security + no_store + [["location", "/admin/"]],
        },
        "never_sent_prefixes": ["access-control-"],
        "cookie_name": "__Host-airk_admin",
        "csrf_header": "x-airk-csrf",
        "set_cookie": {"value": COOKIE,
                       "header": "__Host-airk_admin=" + COOKIE + "; Secure; HttpOnly; SameSite=Strict; Path=/"},
        "clear_cookie": "__Host-airk_admin=; Secure; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        "redacted_request_headers": ["authorization", "cookie", "x-airk-csrf"],
        "exchange_secret_fields": ["code", "csrf"],
        "bare_status_exceptions": [400, 431],
    }


CODES = [
    ("admin_host_refused", "forbidden", False), ("admin_origin_refused", "forbidden", False),
    ("admin_fetch_metadata_refused", "forbidden", False), ("admin_peer_refused", "forbidden", False),
    ("admin_preflight_refused", "forbidden", False), ("admin_bearer_refused", "unauthenticated", False),
    ("admin_session_invalid", "unauthenticated", False), ("admin_launch_code_invalid", "unauthenticated", False),
    ("admin_not_found", "not_found", False), ("admin_operation_unknown", "not_found", False),
    ("admin_method_not_allowed", "method_not_allowed", False), ("admin_upgrade_refused", "malformed", False),
    ("admin_malformed_request", "malformed", False), ("admin_request_too_large", "too_large", False),
    ("admin_stream_limit", "unavailable", True), ("admin_exchange_rate_limited", "unavailable", True),
    ("admin_internal", "internal", False), ("not_yet_served", "unavailable", False),
    ("service_draining", "unavailable", True), ("upgrade_in_progress", "unavailable", True),
    ("cookie_on_bearer_plane", "forbidden", False),
    ("admin_review_unknown", "malformed", False), ("admin_confirmation_mismatch", "malformed", False),
]
STATUS = {"malformed": 400, "unauthenticated": 401, "forbidden": 403, "not_found": 404, "method_not_allowed": 405,
          "refused": 409, "too_large": 413, "internal": 500, "unavailable": 503}


def success(operation, result, request=None, review=None):
    value = {"schema": "airk-admin/1", "ok": True, "operation": operation, "result": result}
    if request is not None:
        value["request"] = request
    if review is not None:
        value["review"] = review
    return value


def failure(code, klass, retryable, **extra):
    return {"schema": "airk-admin/1", "ok": False,
            "error": {"code": code, "class": klass, "retryable": retryable, **extra}}


def envelopes_document():
    nonce = "4e" * 16
    return {
        "schema": "airk.split-fixture.i3-envelopes/1",
        "contract": CONTRACT,
        "generator": GENERATOR,
        "envelope_schema": "airk-admin/1",
        "status_by_class": STATUS,
        "codes": [{"code": code, "class": klass, "status": STATUS[klass], "retryable": retryable}
                  for code, klass, retryable in CODES],
        "valid": [
            {"name": "exchange", "json": success("session.exchange", {
                "csrf": CSRF, "idle_expires_in_ms": 1_800_000, "absolute_expires_in_ms": 43_200_000,
                "service_instance": INSTANCE})},
            {"name": "logout", "json": success("session.logout", {})},
            {"name": "read", "json": success("service.health", {"status": "health", "version": "elided"})},
            {"name": "accepted_mutation", "json": success("jobs.enqueue", {"job": None},
                                                          {"nonce": nonce, "state": "accepted"})},
            {"name": "unknown_outcome", "json": success("service.stop", None, {"nonce": nonce, "state": "unknown"})},
            {"name": "session_invalid", "json": failure("admin_session_invalid", "unauthenticated", False)},
            {"name": "draining", "json": failure("service_draining", "unavailable", True, retry_after_ms=500)},
            {"name": "upgrade_checkpoint", "json": failure("upgrade_in_progress", "unavailable", True)},
            {"name": "not_yet_served", "json": failure("not_yet_served", "unavailable", False)},
            {"name": "native_refusal", "json": failure("key_conflict", "refused", False,
                                                       native={"status": "failure", "code": "key_conflict"})},
            {"name": "native_busy_refusal", "json": failure("registry_busy", "refused", True, retry_after_ms=100)},
            # U98: a generate-mode draft names its review token; the request key stays in the session.
            {"name": "review_token", "json": success("catalog.index_setup_draft", {"draft": "elided"},
                                                     review="5f" * 16)},
        ],
        "invalid": [
            {"name": "success_unknown_field", "json": {**success("service.health", {}), "token": "x"}},
            {"name": "failure_unknown_error_field", "json": failure("admin_internal", "internal", False,
                                                                    message="x")},
            {"name": "unknown_class", "json": failure("admin_internal", "teapot", False)},
            {"name": "request_unknown_state", "json": success("jobs.enqueue", {}, {"nonce": nonce,
                                                                                   "state": "queued"})},
            {"name": "neither_result_nor_error", "json": {"schema": "airk-admin/1", "ok": True,
                                                          "operation": "service.health"}},
        ],
        "rejected_after_decoding": [
            {"name": "wrong_schema", "reason": "envelope_schema",
             "json": {**success("service.health", {}), "schema": "airk-admin/2"}},
            {"name": "success_not_ok", "reason": "envelope_ok", "json": {**success("service.health", {}), "ok": False}},
            {"name": "failure_ok", "reason": "envelope_ok",
             "json": {**failure("admin_internal", "internal", False), "ok": True}},
            {"name": "bad_operation", "reason": "envelope_operation", "json": success("Service.Health", {})},
            {"name": "uppercase_nonce", "reason": "envelope_nonce",
             "json": success("jobs.enqueue", {}, {"nonce": nonce.upper(), "state": "accepted"})},
            {"name": "code_not_snake", "reason": "error_code", "json": failure("Admin-Internal", "internal", False)},
            {"name": "known_code_wrong_class", "reason": "error_class",
             "json": failure("admin_session_invalid", "forbidden", False)},
            {"name": "known_code_wrong_retryable", "reason": "error_retryable",
             "json": failure("admin_peer_refused", "forbidden", True)},
            {"name": "native_code_not_refused", "reason": "error_class",
             "json": failure("key_conflict", "internal", False)},
            {"name": "retry_after_not_retryable", "reason": "error_retry_after",
             "json": failure("key_conflict", "refused", False, retry_after_ms=10)},
            {"name": "uppercase_review", "reason": "envelope_review",
             "json": success("catalog.preview", {}, review=("5f" * 16).upper())},
        ],
        "stream_lines": [
            {"name": "event", "json": {"kind": "event", "cursor": "opaque-1", "event": {"kind": "job"}}},
            {"name": "resync", "json": {"kind": "resync", "reason": "overflow"}},
            {"name": "resync_restart", "json": {"kind": "resync", "reason": "service_restarted"}},
            {"name": "closing", "json": {"kind": "closing"}},
        ],
        "invalid_stream_lines": [
            {"name": "unknown_kind", "json": {"kind": "gap"}},
            {"name": "closing_with_field", "json": {"kind": "closing", "cursor": "x"}},
            {"name": "unknown_reason", "json": {"kind": "resync", "reason": "later"}},
        ],
    }


ALL4 = ["pipe", "airkctl", "session", "admin"]
MGMT = ["pipe", "airkctl", "admin"]
LOCAL = ["pipe", "airkctl"]


def row(operation, method, plane, surfaces, effect, idempotency, conformance):
    authorization = []
    if set(surfaces) & {"pipe", "airkctl", "session"}:
        authorization.append("owner_pipe")
    if "admin" in surfaces:
        authorization.append("admin_session_csrf")
    value = {"operation": operation, "method": method, "plane": plane, "surfaces": surfaces,
             "authorization": authorization, "effect": effect, "idempotency": idempotency,
             "conformance": conformance}
    if operation in KEYS:
        mode, caller, key = KEYS[operation]
        value["request_key"] = {"mode": mode, **({"caller": caller} if caller else {}), "key": key}
    if operation in CONFIRM:
        value["confirmation"] = CONFIRM[operation]
    return value


# Request-identity placement (contracts §2.4, U98), read from the I1 method
# definitions in crates/airk-core/src/service.rs and the catalog request types.
# "generate": the service fills a fresh identity and returns a review token.
# "review": the browser names that token; the service fills the identity back.
KEYS = {
    "indexes.configure": ("generate", "caller", "key"),
    "jobs.enqueue": ("generate", "caller", "key"),
    "jobs.retry": ("generate", "caller", "key"),
    "requests.prepare": ("generate", "caller", "key"),
    "recovery.plan_create": ("generate", "caller", "key"),
    "catalog.policy_resume": ("generate", "caller", "key"),
    "service.stop": ("generate", "caller", "key"),
    "service.restart": ("generate", "caller", "key"),
    "catalog.preview": ("generate", "request.caller", "request.key"),
    "catalog.index_setup_draft": ("generate", None, "selection.key"),
    "catalog.split_draft": ("generate", None, "selection.key"),
    "catalog.grant_draft": ("generate", None, "selection.key"),
    "catalog.source_draft": ("generate", None, "selection.key"),
    "catalog.split_recover": ("generate", "request.caller", "request.key"),
    "catalog.apply": ("review", "request.caller", "request.key"),
    "catalog.index_setup_apply": ("review", "draft.request.caller", "draft.request.key"),
    "catalog.split_apply": ("review", "draft.request.caller", "draft.request.key"),
    "catalog.receipt": ("review", "caller", "key"),
    "catalog.index_setup_receipt": ("review", "caller", "key"),
    "catalog.split_receipt": ("review", "caller", "key"),
    "catalog.split_recovery_receipt": ("review", "caller", "key"),
    "catalog.policy_resume_receipt": ("review", "caller", "key"),
}
CONFIRM = {
    "catalog.apply": "catalog_target",
    "catalog.split_apply": "split_original_index",
    "recovery.plan_execute": "plan_id",
    "storage.purge": "storage_target",
}


def operations():
    read_m, read_e = ["P3", "P5", "P8"], ["P3", "P5", "P6", "P7", "P8"]
    natural, keyed, digest = ["P2", "P3", "P5"], ["P1", "P2", "P3", "P5", "P9"], ["P2", "P3", "P5", "P9"]
    job = ["P1", "P2", "P3", "P4", "P5", "P9"]
    reads = [
        ("service.health", "Health", ALL4), ("service.resources", "Resources", MGMT),
        ("service.workers", "Workers", MGMT), ("service.login_startup_status", "LoginStartup.Status", MGMT),
        ("indexes.list", "Projects", ALL4), ("indexes.inspect", "Inspection", ALL4),
        ("indexes.progress", "Progress", ALL4), ("indexes.plan_registration", "PlanRegistration", MGMT),
        # U103: the Explorer shows watch state; the admin pages read it through Progress.
        ("indexes.watch_state", "Watch", ["pipe", "airkctl", "session"]),
        ("indexes.prepare_configuration", "PrepareConfiguration", MGMT), ("catalog.page", "CatalogPage", ALL4),
        ("catalog.detail", "CatalogDetail", ALL4), ("catalog.preview", "CatalogPreview", MGMT),
        ("catalog.receipt", "CatalogReceipt", MGMT), ("catalog.index_setup_draft", "CatalogIndexSetupDraft", MGMT),
        ("catalog.index_setup_receipt", "CatalogIndexSetupReceipt", MGMT),
        ("catalog.split_draft", "CatalogSplitDraft", MGMT), ("catalog.split_status", "CatalogSplitStatus", ALL4),
        ("catalog.retirement_evidence", "CatalogIndexRetirementEvidence", MGMT),
        ("catalog.policy_status", "CatalogPolicyStatus", ALL4), ("events.page", "Events", ALL4),
        # Split completion (U97, U98): every catalog read the Index library uses.
        ("catalog.source_draft", "CatalogSourceMutationDraft", MGMT),
        ("catalog.source_observation", "CatalogSourceChangeObservation", MGMT),
        ("catalog.pending_source", "CatalogPendingSourceChange", MGMT),
        ("catalog.grant_draft", "CatalogGrantDraft", MGMT),
        ("catalog.split_source_state", "CatalogSplitSourceState", MGMT),
        ("catalog.split_receipt", "CatalogSplitReceipt", MGMT),
        ("catalog.split_recovery_receipt", "CatalogSplitRecoveryReceipt", MGMT),
        ("catalog.policy_resume_receipt", "CatalogPolicyResumeReceipt", MGMT),
        # U103: usage and recent logs are basic status in the Explorer.
        ("usage.document", "UsageDocument", ALL4),
        ("jobs.get", "Job", ALL4), ("requests.list", "ClientRequestList", MGMT),
        ("requests.receipt", "RequestReceipt", MGMT), ("recovery.plan", "Plan", MGMT),
        ("recovery.plan_items", "PlanItems", MGMT), ("storage.inventory", "StorageInventory", MGMT),
        ("storage.purge_plan", "StoragePurgePlan", MGMT),
        # M24: the Browse dialog of the path fields (folders only, never files).
        ("files.browse", "FolderBrowse", MGMT),
        ("logs.recent", "Logs", ALL4), ("logs.export", "LogsExport", MGMT),
        # The local HTTPS MCP endpoint an agent is pointed at (no credential).
        ("agents.endpoint", "AgentEndpoint", MGMT),
        ("agents.connection", "HttpConnectionInfo", MGMT), ("agents.ownership", "HttpConnectionOwnership", MGMT),
        ("agents.verify", "AgentConnectionVerify", MGMT), ("https.trust_status", "HttpTrust.Inspect", MGMT),
        ("https.rotation_status", "HttpIssuerRotation.Status", MGMT), ("models.catalog", "ModelCatalog", MGMT),
        ("indexes.resolve", "ResolveProject", ["pipe", "airkctl", "session"]),
    ]
    for operation, method, surfaces in reads:
        yield row(operation, method, "management", surfaces, "read", "read", read_m)
    for operation, method, surfaces in (("tools.catalog", "ToolsCatalog", ALL4), ("search.call", "ToolsCall", ALL4),
                                        ("explorer.offline", "ExplorerOffline", ALL4),
                                        ("explorer.handoff", "ExplorerHandoff", ["pipe", "airkctl", "session"]),
                                        ("evidence.native_read_scoped", "NativeReadScoped",
                                         ["pipe", "airkctl", "session"])):
        yield row(operation, method, "evidence", surfaces, "read", "read", read_e)
    yield row("search.select_scope", "AdminScopeGrant", "evidence", ["admin"], "mutation", "natural", natural)
    mutations = [
        ("indexes.prepare_storage", "PrepareStorage", MGMT, "mutation", "natural", natural),
        ("indexes.register", "Register", MGMT, "mutation", "natural", natural),
        ("indexes.configure", "ConfigureProject", MGMT, "mutation", "request_key", keyed),
        # U103: the Explorer's everyday index and job actions.
        ("indexes.watch", "ConfigureWatch", ALL4, "mutation", "natural", natural),
        ("indexes.pause", "Pause", ALL4, "mutation", "natural", natural),
        ("jobs.enqueue", "Enqueue", ALL4, "mutation", "request_key", job),
        ("jobs.retry", "Retry", ALL4, "mutation", "request_key", job),
        ("jobs.cancel", "Cancel", ALL4, "mutation", "natural", ["P2", "P3", "P4", "P5"]),
        ("requests.prepare", "ClientRequestPrepare", LOCAL, "mutation", "request_key", keyed),
        ("requests.submit", "ClientRequestSubmit", MGMT, "mutation", "request_key", keyed),
        ("requests.forget", "ClientRequestForget", MGMT, "mutation", "natural", natural),
        ("catalog.apply", "CatalogApply", MGMT, "destructive", "operation_digest", digest),
        ("catalog.index_setup_apply", "CatalogIndexSetupApply", MGMT, "mutation", "operation_digest", digest),
        ("catalog.split_apply", "CatalogSplitApply", MGMT, "destructive", "operation_digest", digest),
        ("catalog.policy_resume", "CatalogPolicyResume", MGMT, "mutation", "request_key", keyed),
        ("catalog.split_recover", "CatalogSplitRecover", MGMT, "mutation", "request_key", keyed),
        ("catalog.source_resolve", "CatalogSourceChangeResolve", MGMT, "mutation", "natural", natural),
        ("recovery.plan_create", "PlanCreate", MGMT, "mutation", "request_key", keyed),
        ("recovery.plan_execute", "PlanExecute", MGMT, "destructive", "operation_digest", digest),
        ("recovery.plan_pause", "PlanPause", MGMT, "mutation", "natural", natural),
        ("recovery.plan_cancel", "PlanCancel", MGMT, "mutation", "natural", natural),
        ("storage.purge", "StoragePurge", MGMT, "destructive", "operation_digest", digest),
        ("service.configure_resources", "ConfigureResources", MGMT, "mutation", "natural", natural),
        ("service.stop", "Lifecycle.Stop", MGMT, "mutation", "request_key", keyed),
        # Admin Restart (plan §7.3, P0 decision): the handler restarts through the OS login
        # manager when login startup is registered; otherwise it refuses with guidance.
        ("service.restart", "Lifecycle.Restart", MGMT, "mutation", "request_key", keyed),
        ("service.abandon_lifecycle", "AbandonLifecycle", MGMT, "mutation", "natural", natural),
        ("service.login_startup_install", "LoginStartup.Install", MGMT, "mutation", "natural", natural),
        ("service.login_startup_uninstall", "LoginStartup.Uninstall", MGMT, "mutation", "natural", natural),
        ("service.record_client_error", "RecordClientError", ALL4, "mutation", "natural", natural),
        ("agents.revoke", "HttpRevoke", MGMT, "mutation", "natural", natural),
        # U105: an open connection's default scope and agent choice; the Explorer's
        # applied search scope relays it (session). Fenced by its defaults revision.
        ("agents.connection_defaults", "AgentConnectionDefaults", LOCAL + ["session"], "mutation", "natural",
         natural),
    ]
    for operation, method, surfaces, effect, idempotency, conformance in mutations:
        yield row(operation, method, "management", surfaces, effect, idempotency, conformance)


NEVER = [
    {"item": "bearer_credentials", "methods": [],
     "reason": "Bearer tokens and credential files never leave the owner plane."},
    {"item": "private_keys", "methods": [], "reason": "TLS leaf and issuer private keys."},
    {"item": "connection_grants", "methods": ["Connect", "ConnectScoped"], "reason": "Raw ConnectionGrant."},
    {"item": "upgrade_checkpoints", "methods": ["PrepareUpgradeCheckpoint", "ObserveUpgradeCheckpoint",
                                                "UpgradeRecoveryProbe", "ReleaseUpgradeAdmission"],
     "reason": "Upgrade checkpoints are installer-class owner-pipe operations."},
    {"item": "credential_enrollment", "methods": ["HttpEnroll"],
     "reason": "Credential-creating; `airkctl agents connect` only. The Agents page shows the command."},
    {"item": "admin_launch_code", "methods": ["AdminLaunchCode"], "reason": "A1 is owner pipe only."},
    {"item": "admin_session_revocation", "methods": ["AdminEndSessions"],
     "reason": "Ending every admin session after a suspected leak is an owner-pipe method (L4)."},
    {"item": "window_leases", "methods": ["WindowOpen", "WindowRenew", "WindowClose", "AssistanceDrain",
                                          "HandoffPublish"], "reason": "W1-W3, W7-W8 carry a window lease."},
    {"item": "certificate_trust_changes", "methods": ["HttpIssuerRotation.Stage", "HttpIssuerRotation.Commit",
                                                      "HttpIssuerRotation.Finalize"],
     "reason": "Trust install/remove are offline airkctl operations; rotation stages trust (L8). Status only."},
    {"item": "uninstall_and_setup_role", "methods": [], "reason": "Installer and setup role only."},
    {"item": "raw_relay", "methods": [], "reason": "`airkctl service rpc` is an owner-pipe raw relay."},
]


def invalid_tables():
    good = row("service.stop", "Lifecycle.Stop", "management", MGMT, "mutation", "request_key",
               ["P1", "P2", "P3", "P5", "P9"])
    read = row("service.health", "Health", "management", ALL4, "read", "read", ["P3", "P5", "P8"])

    def bad(name, reason, *rows):
        return {"name": name, "reason": reason, "rows": list(rows)}

    yield bad("duplicate_operation", "operation_duplicate", good, good)
    yield bad("operation_not_dotted", "operation_id", {**good, "operation": "stop"})
    yield bad("operation_uppercase", "operation_id", {**good, "operation": "Service.Stop"})
    yield bad("method_empty", "method_empty", {**good, "method": ""})
    yield bad("no_surface", "surfaces", {**good, "surfaces": []})
    yield bad("surface_twice", "surfaces", {**good, "surfaces": ["pipe", "pipe", "airkctl", "admin"]})
    yield bad("authorization_twice", "duplicate_entry",
              {**good, "authorization": ["owner_pipe", "owner_pipe", "admin_session_csrf"]})
    yield bad("admin_without_csrf_rule", "authorization_admin", {**good, "authorization": ["owner_pipe"]})
    yield bad("csrf_rule_without_admin", "authorization_admin", {**good, "surfaces": ["pipe", "airkctl"]})
    yield bad("pipe_without_owner_rule", "authorization_owner_pipe", {**good, "authorization": ["admin_session_csrf"]})
    yield bad("read_with_request_key", "effect_idempotency", {**read, "idempotency": "request_key"})
    yield bad("mutation_with_read_idempotency", "effect_idempotency", {**good, "idempotency": "read"})
    yield bad("destructive_without_digest", "effect_idempotency", {**good, "effect": "destructive"})
    yield bad("missing_p5", "conformance_missing", {**read, "conformance": ["P3", "P8"]})
    yield bad("mutation_without_p2", "conformance_missing", {**good, "conformance": ["P1", "P3", "P5", "P9"]})
    yield bad("request_key_without_p1", "conformance_missing", {**good, "conformance": ["P2", "P3", "P5", "P9"]})
    yield bad("admin_read_without_p8", "conformance_missing", {**read, "conformance": ["P3", "P5"]})
    yield bad("evidence_read_without_p7", "conformance_missing",
              {**read, "plane": "evidence", "conformance": ["P3", "P5", "P6", "P8"]})
    yield bad("admin_enroll", "never_over_http", {**good, "operation": "agents.enroll", "method": "HttpEnroll"})
    yield bad("admin_rotation_commit", "never_over_http",
              {**good, "operation": "https.rotation_commit", "method": "HttpIssuerRotation.Commit"})
    yield bad("admin_action_of_never_method", "never_over_http",
              {**good, "operation": "agents.connect", "method": "Connect.Project"})
    keyed_rule = {"mode": "generate", "caller": "caller", "key": "key"}
    yield bad("key_path_uppercase", "request_key_path", {**good, "request_key": {**keyed_rule, "key": "Key"}})
    yield bad("key_path_empty_segment", "request_key_path",
              {**good, "request_key": {**keyed_rule, "caller": "request..caller"}})
    yield bad("key_path_too_deep", "request_key_path", {**good, "request_key": {**keyed_rule, "key": "a.b.c.d.e"}})
    digest_row = row("catalog.index_setup_apply", "CatalogIndexSetupApply", "management", MGMT, "mutation",
                     "operation_digest", ["P2", "P3", "P5", "P9"])
    yield bad("generate_on_digest_row", "request_key_mode",
              {**digest_row, "request_key": {"mode": "generate", "key": "draft.request.key"}})
    yield bad("review_on_keyed_mutation", "request_key_mode", {**good, "request_key": {**keyed_rule, "mode": "review"}})
    destructive = row("storage.purge", "StoragePurge", "management", MGMT, "destructive", "operation_digest",
                      ["P2", "P3", "P5", "P9"])
    yield bad("destructive_without_confirmation", "confirmation",
              {key: value for key, value in destructive.items() if key != "confirmation"})
    yield bad("confirmation_on_mutation", "confirmation", {**good, "confirmation": "plan_id"})


def operations_document():
    return {
        "schema": "airk.split-fixture.i3-operations/1",
        "contract": CONTRACT + "; split plan §7.3; contracts.md §2.2",
        "generator": GENERATOR,
        "status": "Reviewed table (C5, amended by U98 on 12 November 2026). admin_plane/CONTRACT.md lists the rows the service serves and the rows it refuses.",
        "enumerations": {"plane": ["management", "evidence"], "surface": ["pipe", "airkctl", "session", "admin"],
                         "authorization": ["owner_pipe", "window_lease", "admin_session_csrf"],
                         "effect": ["read", "mutation", "destructive"],
                         "idempotency": ["read", "request_key", "operation_digest", "natural"],
                         "conformance": ["P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8", "P9"],
                         "key_mode": ["generate", "review"],
                         "confirmation": ["catalog_target", "split_original_index", "plan_id", "storage_target"]},
        "operations": list(operations()),
        "never_over_http": NEVER,
        "invalid_tables": list(invalid_tables()),
    }


DOCUMENTS = {
    "admission_cases.json": admission_document,
    "launch_exchange.json": launch_document,
    "headers.json": headers_document,
    "envelopes.json": envelopes_document,
    "admin_operations.json": operations_document,
}


def render(builder):
    return json.dumps(builder(), indent=1, ensure_ascii=True) + "\n"


def names(document):
    found = []
    for key in ("cases", "exchanges", "admission", "valid", "invalid", "rejected_after_decoding", "invalid_tables"):
        found.extend(entry["name"] for entry in document.get(key, []))
    found.extend(entry["operation"] for entry in document.get("operations", []))
    return found


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    stale = []
    for file_name, builder in DOCUMENTS.items():
        rendered = render(builder)
        listed = names(json.loads(rendered))
        if len(listed) != len(set(listed)):
            raise SystemExit(f"{file_name}: duplicate case names")
        output = HERE / file_name
        if args.check:
            current = output.read_text(encoding="utf-8") if output.exists() else ""
            if current != rendered:
                stale.append(file_name)
            continue
        output.write_bytes(rendered.encode("utf-8"))
        print(f"{len(listed)} named entries -> {output.name}")
    if stale:
        print(f"stale: {', '.join(stale)}; rerun without --check and review the diff", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
