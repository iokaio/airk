# SPDX-License-Identifier: Apache-2.0
"""The split I1 fixtures (C3a/C3b) are current and keep their contract invariants.

These checks read the rendered JSON, not the generators' helpers, so a helper
mistake cannot hide an invariant violation.
"""
import base64
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "fixtures" / "split" / "i1"
GENERATORS = ["request_recovery", "window_plane", "admin_launch_code"]
CODE = re.compile(r"^[a-z][a-z0-9_]*$|^Key(Conflict|Expired)$")


def load(name):
    return json.loads((FIXTURES / f"{name}.json").read_text(encoding="utf-8"))


def text(value):
    return json.dumps(value, ensure_ascii=False)


def method_name(frame):
    method = frame["method"]
    return method if isinstance(method, str) else next(iter(method))


class Generators(unittest.TestCase):
    def test_check_mode_reports_no_drift(self):
        for name in GENERATORS:
            with self.subTest(name):
                result = subprocess.run([sys.executable, str(FIXTURES / f"{name}.py"), "--check"],
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_files_are_lf_and_generators_carry_spdx(self):
        for name in GENERATORS:
            for suffix in (".py", ".json"):
                data = (FIXTURES / f"{name}{suffix}").read_bytes()
                self.assertNotIn(b"\r", data, name + suffix)
            first = (FIXTURES / f"{name}.py").read_text(encoding="utf-8").splitlines()[0]
            self.assertEqual(first, "# SPDX-License-Identifier: Apache-2.0")

    def test_cases_are_named_uniquely_and_refusal_codes_are_spelled_canonically(self):
        for name in GENERATORS:
            cases = load(name)["cases"]
            self.assertGreater(len(cases), 0, name)
            self.assertEqual(len({case["name"] for case in cases}), len(cases), name)
            for case in cases:
                for step in case["transcript"]:
                    for frame in (step.get("reply"), (step.get("reply") or {}).get("outcome")):
                        if isinstance(frame, dict) and frame.get("status") in ("failure", "failed"):
                            self.assertRegex(frame["code"], CODE, case["name"])


class RequestRecovery(unittest.TestCase):
    def setUp(self):
        self.document = load("request_recovery")
        self.definitive = set(self.document["definitive_m5_refusals"])

    def test_rules_state_the_reconnect_guarantee(self):
        self.assertIn("Reconnect never mints a new key and never replays an uncertain mutation.",
                      self.document["rules"])

    def test_outcome_classes(self):
        outcomes = set(self.document["outcomes"])
        self.assertEqual(outcomes, {"submitted", "accepted", "completed", "refused", "unknown"})
        for case in self.document["cases"]:
            self.assertIn(case["expected"]["outcome"], outcomes, case["name"])
            for step in case["transcript"]:
                if step["step"] == "classify":
                    self.assertIn(step["outcome"], outcomes, case["name"])

    def test_new_keys_follow_only_a_definitive_refusal(self):
        for case in self.document["cases"]:
            previous = None
            minted = 0
            for step in case["transcript"]:
                if step["step"] == "client_mints_key":
                    minted += 1
                    if minted > 1:
                        self.assertIn(step.get("after"), self.definitive, case["name"])
                        self.assertEqual(previous, {"status": "failure", "code": step["after"]}, case["name"])
                # Reading a fresh epoch between the refusal and the new key is expected.
                if step["step"] in ("reply", "reply_lost") and step["frame"]["status"] != "request_epoch":
                    previous = step["frame"]
                elif step["step"] not in ("send", "reply", "client_mints_key"):
                    previous = step
            refusals = [step["frame"].get("code") for step in case["transcript"] if step["step"] == "reply"]
            if case["expected"]["new_key_allowed"]:
                self.assertTrue(self.definitive.intersection(refusals), case["name"])

    def test_unknown_is_never_replayed(self):
        for case in self.document["cases"]:
            expected = case["expected"]
            if expected["outcome"] == "unknown":
                self.assertFalse(expected["retry_may_reuse_key"], case["name"])
                self.assertFalse(expected["new_key_allowed"], case["name"])

    def test_no_key_succeeds_with_two_argument_sets(self):
        for case in self.document["cases"]:
            accepted = {}
            transcript = case["transcript"]
            for index, step in enumerate(transcript):
                if step["step"] != "send" or method_name(step["frame"]) not in ("client_request_prepare", "enqueue"):
                    continue
                method = step["frame"]["method"]
                body = method.get("enqueue") or method["client_request_prepare"]["request"]["enqueue"]
                identity = text([body["caller"], body["key"]])
                outcome = next(s for s in transcript[index + 1:] if s["step"] in ("reply", "reply_lost",
                                                                                     "connection_lost_before_durable"))
                if outcome["step"] == "connection_lost_before_durable" or outcome["frame"]["status"] == "failure":
                    continue
                arguments = text({k: v for k, v in body.items() if k not in ("caller", "key")})
                self.assertEqual(accepted.setdefault(identity, arguments), arguments, case["name"])

    def test_every_section_5_2_situation_is_covered(self):
        situations = {case["situation"].split(" (")[0].split(", then")[0] for case in self.document["cases"]}
        for required in ("disconnect before acceptance", "disconnect after durable acceptance",
                         "disconnect after completion", "client death", "service restart",
                         "duplicate key, same arguments", "same key, different arguments", "stale epoch",
                         "receipt lookup: accepted", "receipt lookup: completed", "receipt lookup: unknown"):
            self.assertIn(required, situations)


class WindowPlane(unittest.TestCase):
    def setUp(self):
        self.document = load("window_plane")
        self.leases = self.document["secrets"]["lease_by_window"]

    def test_leases_travel_only_on_the_pipe(self):
        self.assertEqual(self.document["secret_channels"], {"lease": ["pipe"]})
        for case in self.document["cases"]:
            for step in case["transcript"]:
                if step.get("channel") == "pipe":
                    continue
                for lease in self.leases.values():
                    self.assertNotIn(lease, text(step), case["name"])

    def test_a_foreign_lease_is_always_refused(self):
        foreign = 0
        for case in self.document["cases"]:
            transcript = case["transcript"]
            for index, step in enumerate(transcript):
                if step.get("channel") != "pipe" or "send" not in step:
                    continue
                body = next(iter(step["send"]["method"].values())) if isinstance(step["send"]["method"], dict) else {}
                pair = body.get("selection", {}).get("window", body) if "selection" in body else body
                if "lease" in pair and self.leases.get(pair["window_id"]) != pair["lease"]:
                    reply = next(s for s in transcript[index + 1:] if s.get("channel") == "pipe" and "reply" in s)
                    self.assertEqual(reply["reply"], {"status": "failure", "code": "window_lease_invalid"},
                                     case["name"])
                    foreign += 1
        self.assertGreaterEqual(foreign, 2)

    def test_restart_and_close_never_reuse_a_grant(self):
        cases = {case["name"]: case for case in self.document["cases"]}
        for name in ("grant_revoked_on_service_restart", "grant_revoked_on_session_close", "scope_release"):
            self.assertIsNone(cases[name]["expected"]["active_scope"], name)
        self.assertFalse(cases["grant_revoked_on_service_restart"]["expected"]["automatic_regrant"])
        granted = cases["scope_select_in_memory_grant"]["expected"]
        self.assertEqual((granted["grants_created"], granted["indexes_created"], granted["persisted"]), (0, 0, False))

    def test_queue_is_bounded_and_never_drops(self):
        limits = self.document["limits"]
        cases = {case["name"]: case for case in self.document["cases"]}
        count = cases["assistance_queue_count_bound"]
        self.assertEqual(count["expected"], {"queued": limits["drafts_per_window"], "dropped": 0})
        self.assertIn({"status": "failure", "code": "assistance_queue_full"},
                      [step.get("reply") for step in count["transcript"]])
        self.assertEqual(cases["session_buffer_overflow_resync"]["expected"]["lost_drafts"], 0)


class AdminLaunchCode(unittest.TestCase):
    def setUp(self):
        self.document = load("admin_launch_code")
        self.codes = self.document["secrets"]["launch_code"]

    def test_codes_are_256_bit_base64url_and_stored_as_their_sha256(self):
        for code in self.codes:
            self.assertRegex(code, r"^[A-Za-z0-9_-]{43}$")
            self.assertEqual(len(base64.urlsafe_b64decode(code + "=")), 32)
        for case in self.document["cases"]:
            replies = [s["reply"] for s in case["transcript"] if s.get("reply", {}).get("status") == "admin_launch_code"]
            stores = [s["service_stores"] for s in case["transcript"] if "service_stores" in s]
            self.assertEqual([hashlib.sha256(r["code"].encode("ascii")).hexdigest() for r in replies],
                             [s["sha256"] for s in stores], case["name"])

    def test_codes_appear_only_in_the_pipe_reply_stdout_and_consumption(self):
        hashes = {hashlib.sha256(code.encode("ascii")).hexdigest() for code in self.codes}
        for case in self.document["cases"]:
            for step in case["transcript"]:
                allowed = (step.get("channel") == "pipe" and step.get("reply", {}).get("status") == "admin_launch_code"
                           or step.get("channel") == "stdout" or "consume" in step)
                if not allowed:
                    for code in self.codes:
                        self.assertNotIn(code, text(step), case["name"])
                if "service_stores" not in step:
                    for digest in hashes:
                        self.assertNotIn(digest, text(step), case["name"])

    def test_single_use(self):
        for case in self.document["cases"]:
            used = [s["consume"] for s in case["transcript"]
                    if "consume" in s and s["expect"]["result"] == "session_created"]
            self.assertEqual(len(used), len(set(used)), case["name"])

    def test_session_and_http_never_reach_a1(self):
        cases = {case["name"]: case for case in self.document["cases"]}
        for name in ("refused_on_session_relay", "refused_over_http"):
            transcript = cases[name]["transcript"]
            self.assertFalse([s for s in transcript if s.get("channel") == "pipe"], name)
            self.assertEqual(cases[name]["expected"]["outstanding"], 0)
        for step in cases["refused_on_session_relay"]["transcript"]:
            if "reply" in step:
                self.assertEqual(step["reply"]["outcome"]["code"], "session_command_refused")


if __name__ == "__main__":
    unittest.main()
