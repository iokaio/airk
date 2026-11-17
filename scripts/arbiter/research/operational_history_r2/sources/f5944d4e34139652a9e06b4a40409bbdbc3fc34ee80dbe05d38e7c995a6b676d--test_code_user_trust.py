# SPDX-License-Identifier: Apache-2.0
"""Mocked trust lifecycle contracts. Never accesses an OS trust store."""
import hashlib
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location("code_user_trust", Path(__file__).with_name("code-user-trust.py"))
trust = importlib.util.module_from_spec(spec)
spec.loader.exec_module(trust)


class CodeUserTrustTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.owner = Path(self.temporary.name).resolve()
        install = self.owner / "native-service"
        install.mkdir()
        binary = install / "candidate.exe"
        binary.write_bytes(b"sealed native")
        control = self.owner / "control"
        (control / "http-1").mkdir(parents=True)
        (control / "http-1/ca.pem").write_text("public CA fixture")
        self.digest = hashlib.sha256(b"der").hexdigest()
        self.fixture = SimpleNamespace(install=install, binary=binary, control=control, run=Mock())
        self.pem = patch.object(trust.ssl, "PEM_cert_to_DER_cert", return_value=b"der")
        self.pem.start()
        self.addCleanup(self.pem.stop)

    def reply(self, outcome, owned, present=True, trusted=True):
        return SimpleNamespace(returncode=0, stderr="", stdout=json.dumps({"outcome": outcome,
            "ca_sha256":self.digest,"owned":owned,"present":present,"trusted":trusted}))

    def enroll(self):
        self.fixture.run.return_value=self.reply("installed",True)
        return trust.install(self.fixture,self.owner)

    def test_install_and_remove_use_exact_owned_binary_install_and_digest(self):
        intent=self.enroll()
        self.assertEqual(self.fixture.run.call_args.args,tuple(trust.command(intent,"install")[1:]))
        invoke=Mock(return_value=self.reply("removed",False,False,False))
        trust.remove(self.owner,invoke)
        invoke.assert_called_once_with(trust.command(intent,"remove"))
        self.assertEqual(json.loads((self.owner/"code-trust-remove.json").read_text())["status"]["outcome"],"removed")

    def test_uncertain_install_keeps_intent_and_both_results_for_one_remove_attempt(self):
        self.fixture.run.side_effect=TimeoutError("uncertain install")
        with self.assertRaises(TimeoutError):trust.install(self.fixture,self.owner)
        self.assertTrue((self.owner/"code-trust-intent.json").exists())
        invoke=Mock(side_effect=TimeoutError("uncertain remove"))
        with self.assertRaises(TimeoutError):trust.remove(self.owner,invoke)
        self.assertEqual(self.fixture.run.call_count,1)
        self.assertEqual(invoke.call_count,1)
        for action in ("install","remove"):
            self.assertIn("error",json.loads((self.owner/f"code-trust-{action}.json").read_text()))

    def test_changed_binary_refuses_remove_without_mutation(self):
        self.enroll()
        self.fixture.binary.write_bytes(b"changed")
        invoke=Mock()
        with self.assertRaises(AssertionError):trust.remove(self.owner,invoke)
        invoke.assert_not_called()
        self.assertEqual(json.loads((self.owner/"code-trust-remove.json").read_text())["mutation_dispatches"],0)

    def test_no_install_dispatch_does_not_mutate_trust(self):
        invoke=Mock()
        trust.remove(self.owner,invoke)
        invoke.assert_not_called()

    def test_preexisting_unowned_trust_is_not_claimed_as_an_owned_install(self):
        self.fixture.run.return_value=self.reply("already_trusted",False)
        with self.assertRaises(AssertionError):trust.install(self.fixture,self.owner)
        self.assertEqual(self.fixture.run.call_count,1)


if __name__=="__main__":unittest.main()
