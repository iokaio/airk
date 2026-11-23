# SPDX-License-Identifier: Apache-2.0
"""Run the device workflow against a small stand-in service that follows, or breaks, the opt-in contract."""
import json
from pathlib import Path
import tempfile
import types
import unittest

from gpu_opt_in_workflow import exercise

INDEX = "idx_0123456789abcdef0123456789abcdef"
FRESH = ["managed-start", "catalog-setup", "initial", "auto-fp16", "initial-resources", "set-disabled",
         "get-disabled", "arbiter-disabled", "set-enabled", "get-enabled", "arbiter-enabled",
         "enable-unqualified", "after-refusal", "same-model-cpu"]


def check(condition, detail):
    if not condition:
        raise ValueError(detail)


class Service:
    """Follows the Core Arbiter contract section "Service-wide GPU use is opt-in" unless a fault is named."""

    def __init__(self, *, stored=None, fault=None):
        self.control = Path("control")
        self.fault = fault
        # `stored` is the configuration file as an earlier build wrote it.
        stored = stored or {}
        self.allow_gpu, self.opt_in = stored.get("allow_gpu", False), stored.get("gpu_opt_in")
        if self.allow_gpu and self.opt_in is None and fault != "keeps-earlier-default":
            self.allow_gpu, self.opt_in = False, "default_reset"
        if fault == "fresh-enabled":
            self.allow_gpu = True
        if fault == "fresh-notice":
            self.opt_in = "default_reset"
        self.revision = 3
        self.context = {"enabled": False, "selected_model": "arbiter", "device_policy": "cpu",
                        "gpu_precision": "fp16", "indexes": []}

    def status(self):
        context = dict(self.context)
        return {"schema": "airk.arbiter.status/3",
                "configuration": {"revision": self.revision, "context": context},
                "context": {"selected_model": context["selected_model"], "device_policy": context["device_policy"],
                            "active_profile": None}}

    def run(self, *arguments, check=True):
        words = [str(word) for word in arguments]
        done = lambda value, code=0, error="": types.SimpleNamespace(returncode=code, stdout=value, stderr=error)
        if "library" in words and "setup" in words:
            return done(json.dumps({"status": "catalog_index_setup_applied",
                                    "receipt": {"stage": "ready", "index_id": INDEX}}) + "\n")
        if "library" in words and "list" in words:
            return done(json.dumps({"status": "catalog_page",
                                    "page": {"items": [{"kind": "index", "value": {"id": INDEX}}]}}) + "\n")
        if words[-1] == "status":
            return done(json.dumps(self.status()))
        request = json.loads(Path(words[-1]).read_bytes())
        if request["expected_revision"] != self.revision:
            return done("", 1, json.dumps({"code": "arbiter_revision_conflict"}))
        if request["configuration"]["enabled"] and self.fault != "activates-unqualified":
            return done("", 1, json.dumps({"code": "arbiter_unavailable"}))
        self.revision += 1
        self.context = request["configuration"]
        return done(json.dumps(self.status()))

    def rpc(self, request):
        if request != "resources":
            allow = request["set_gpu_allowed"]["allow_gpu"]
            self.allow_gpu = allow
            self.opt_in = "user_choice" if allow else None
            if self.fault == "enables-without-record" and allow:
                self.opt_in = None
            if self.fault == "keeps-notice-after-answer" and not allow:
                self.opt_in = "default_reset"
            if self.fault == "setting-changes-stage":
                self.revision += 1
            if self.fault == "write-not-stored" and allow:
                answer = self.resources()
                self.allow_gpu, self.opt_in = False, None
                return answer
        return self.resources()

    def resources(self):
        configuration = {"allow_gpu": self.allow_gpu}
        if self.opt_in is not None or self.fault == "null-record":
            configuration["gpu_opt_in"] = self.opt_in
        configuration.update(query_concurrency=16, preset="balanced")
        return {"status": "resources", "observation": {"configuration": configuration}, "configuration_error": False}


class GpuOptInWorkflow(unittest.TestCase):
    def journey(self, service, **options):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        (root / "managed-start.json").write_text("{}")
        journal = []
        def record(name, value):
            journal.append((name, value))
            return value
        exercise(root, service, record, check, **options)
        return journal

    def test_fresh_configuration_is_off_and_only_the_setting_turns_gpu_on(self):
        service = Service()
        journal = self.journey(service)
        self.assertEqual([name for name, _ in journal], FRESH)
        seen = {name: value["observation"]["configuration"] for name, value in journal
                if isinstance(value, dict) and value.get("status") == "resources"}
        off = {"allow_gpu": False, "query_concurrency": 16, "preset": "balanced"}
        self.assertEqual(seen["initial-resources"], off)
        self.assertEqual(seen["set-disabled"], off)
        self.assertEqual(seen["get-disabled"], off)
        on = dict(off, allow_gpu=True, gpu_opt_in="user_choice")
        self.assertEqual(seen["set-enabled"], on)
        self.assertEqual(seen["get-enabled"], on)
        self.assertEqual((service.allow_gpu, service.opt_in), (True, "user_choice"))
        self.assertEqual((service.revision, service.context["device_policy"]), (5, "cpu"))

    def test_value_stored_under_the_earlier_default_reads_as_off_with_the_notice(self):
        service = Service(stored={"allow_gpu": True})
        journal = self.journey(service, existing_index=INDEX, stored_enabled_default=True)
        self.assertEqual([name for name, _ in journal],
                         ["managed-start", "existing-catalog-pages", *FRESH[2:]])
        seen = dict(journal)
        self.assertEqual(seen["initial-resources"]["observation"]["configuration"],
                         {"allow_gpu": False, "gpu_opt_in": "default_reset", "query_concurrency": 16,
                          "preset": "balanced"})
        # The user's answer `false` clears the notice and leaves GPU use off.
        self.assertNotIn("gpu_opt_in", seen["set-disabled"]["observation"]["configuration"])
        self.assertIs(seen["set-disabled"]["observation"]["configuration"]["allow_gpu"], False)

    def test_the_expected_starting_state_is_not_interchangeable(self):
        with self.assertRaisesRegex(ValueError, "must start off with the reset notice pending"):
            self.journey(Service(), stored_enabled_default=True)
        with self.assertRaisesRegex(ValueError, "must start off without a notice"):
            self.journey(Service(stored={"allow_gpu": True}))
        # A stored off value carries no notice. A value the user chose stays on, so the journey,
        # which starts from a configuration with GPU use off, refuses it.
        self.journey(Service(stored={"allow_gpu": False}))
        with self.assertRaisesRegex(ValueError, "must start off without a notice"):
            self.journey(Service(stored={"allow_gpu": True, "gpu_opt_in": "user_choice"}))

    def test_services_that_break_the_contract_fail(self):
        for fault, stored, message in (
                ("fresh-enabled", None, "must start off without a notice"),
                ("fresh-notice", None, "must start off without a notice"),
                ("keeps-earlier-default", {"allow_gpu": True}, "must start off with the reset notice pending"),
                ("null-record", None, "must start off without a notice"),
                ("enables-without-record", None, "native setting write failed"),
                ("keeps-notice-after-answer", {"allow_gpu": True}, "native setting write failed"),
                ("write-not-stored", None, "native setting read differs"),
                ("setting-changes-stage", None, "global GPU setting changed stage selections"),
                ("activates-unqualified", None, "unexpected native result for unqualified stage activation")):
            with self.subTest(fault=fault), self.assertRaisesRegex(ValueError, message):
                self.journey(Service(stored=stored, fault=fault), stored_enabled_default=stored is not None)


if __name__ == "__main__":
    unittest.main()
