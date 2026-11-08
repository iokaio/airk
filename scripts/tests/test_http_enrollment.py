# SPDX-License-Identifier: Apache-2.0
"""Actual protected input and exact subprocess contract; no console setup bypass."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import time
from types import SimpleNamespace
import unittest

SPEC = importlib.util.spec_from_file_location("native_enroll_fixture", Path(__file__).resolve().parents[1] / "http_fixture.py")
HTTP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HTTP)


class EnrollmentTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.control = Path(self.temp.name).resolve()
        self.private = self.control / "http-1"
        self.private.mkdir(mode=0o700)
        self.identity = "a" * 32
        self.selection = {"kind": "scoped", "selection": {"expected_catalog_revision": 7, "grant_id": "exact", "project_ids": ["one"]}}
        self.connection = {"schema": 1, "connection_id": self.identity, "selection": copy.deepcopy(self.selection),
                           "credential_file": str(self.private / f"credential-{self.identity}.token"),
                           "ca_certificate_file": str(self.private / "ca.pem")}
        for field in ("credential_file", "ca_certificate_file"):
            Path(self.connection[field]).touch(mode=0o600)
        self.calls = []

    def invoke(self, *args, timeout):
        self.calls.append(args)
        self.assertEqual(args[:2], ("runtime", "http-connection"))
        self.assertEqual(args[2:4], ("--install-dir", str(self.control / "install")))
        self.assertEqual(args[6:8], ("--connection-id", self.identity))
        self.assertEqual(args[8:10], ("--client", "copilot-cli"))
        path = Path(args[5])
        self.assertEqual(path.parent, self.private)
        self.assertEqual(json.loads(path.read_bytes()), self.selection)
        self.assertLessEqual(timeout, 5)
        if os.name != "nt":
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        return SimpleNamespace(returncode=0, stdout=json.dumps(self.connection))

    def enroll(self, run=None, deadline=None):
        return HTTP.enroll_native(run or self.invoke, self.control / "install", self.control,
                                  self.selection, self.identity, "copilot_cli",
                                  deadline=deadline or time.monotonic() + 5)

    def test_exact_native_command_private_input_and_bare_response(self):
        self.connection["selection"]["selection"]["expected_catalog_revision"] = 8
        self.assertEqual(self.enroll(), self.connection)
        self.assertEqual(len(self.calls), 1)
        self.assertFalse(list(self.private.glob("fixture-selection-*")))

    def test_failure_is_not_replayed_and_input_is_removed(self):
        def failed(*args, **kwargs):
            self.invoke(*args, **kwargs)
            return SimpleNamespace(returncode=1, stdout="{}")
        with self.assertRaises(ValueError): self.enroll(failed)
        self.assertEqual(len(self.calls), 1)
        self.assertFalse(list(self.private.glob("fixture-selection-*")))

    def test_crossed_scope_id_reference_and_backwards_clock_refused(self):
        original = copy.deepcopy(self.connection)
        for key, value in (("connection_id", "b" * 32), ("credential_file", self.connection["ca_certificate_file"]),
                           ("selection", {"kind": "scoped", "selection": {"expected_catalog_revision": 6, "grant_id": "exact", "project_ids": ["one"]}}),
                           ("selection", {"kind": "scoped", "selection": {"expected_catalog_revision": 8, "grant_id": "broad", "project_ids": ["one"]}})):
            with self.subTest(key=key, value=value):
                self.connection = {**copy.deepcopy(original), key: value}
                with self.assertRaises(ValueError): self.enroll()

    def test_expired_original_deadline_never_submits(self):
        with self.assertRaises(TimeoutError): self.enroll(deadline=time.monotonic() - 1)
        self.assertFalse(self.calls)
        self.assertFalse(list(self.private.glob("fixture-selection-*")))

    @unittest.skipUnless(os.name == "nt", "actual Windows verbatim path identity")
    def test_native_verbatim_paths_keep_exact_file_and_containment_checks(self):
        for field in ("credential_file", "ca_certificate_file"):
            plain = Path(self.connection[field])
            self.connection[field] = "\\\\?\\" + str(plain)
            self.assertNotEqual(plain.resolve(), Path(self.connection[field]).resolve())
            self.assertTrue(plain.samefile(self.connection[field]))
        self.assertEqual(self.enroll(), self.connection)
        self.assertEqual(HTTP.private_bytes(self.connection["ca_certificate_file"], self.control, 10), b"")
        # Same basename outside the owned ancestry is never accepted.
        with tempfile.TemporaryDirectory() as outside:
            foreign = Path(outside).resolve() / "ca.pem"
            foreign.touch(mode=0o600)
            self.connection["ca_certificate_file"] = "\\\\?\\" + str(foreign)
            with self.assertRaisesRegex(ValueError, "escaped"):
                self.enroll()

    def test_outside_hardlink_cannot_borrow_owned_ancestry(self):
        with tempfile.TemporaryDirectory() as outside:
            foreign = Path(outside).resolve() / "ca.pem"
            os.link(self.connection["ca_certificate_file"], foreign)
            self.assertTrue(foreign.samefile(self.connection["ca_certificate_file"]))
            with self.assertRaisesRegex(ValueError, "escaped"):
                HTTP.private_bytes(foreign, self.control, 10)
            # The original file's hardlink count is still enforced as before.
            with self.assertRaisesRegex(ValueError, "private regular file"):
                HTTP.private_bytes(self.connection["ca_certificate_file"], self.control, 10)


if __name__ == "__main__":
    unittest.main()
