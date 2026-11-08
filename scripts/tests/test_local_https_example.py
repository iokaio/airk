# SPDX-License-Identifier: Apache-2.0
"""The standalone developer example uses real TLS without ambient trust changes."""
import importlib.util
import json
from pathlib import Path
import unittest


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ROOT = Path(__file__).resolve().parents[2]
BASE = load("local_api_tls_fixture", ROOT / "scripts/tests/test_http_fixture.py")
EXAMPLE = load("local_https_example", ROOT / "examples/local_https_client.py")


@unittest.skipUnless(BASE.OPENSSL, "OpenSSL required for the isolated TLS fixture")
class LocalHttpsExampleTests(unittest.TestCase):
    setUpClass = classmethod(BASE.ActualHttpsFixtureTests.setUpClass.__func__)
    tearDownClass = classmethod(BASE.ActualHttpsFixtureTests.tearDownClass.__func__)
    server = BASE.ActualHttpsFixtureTests.server

    def save(self, connection):
        path = self.root / "connection.json"
        path.write_text(json.dumps(connection), encoding="utf-8")
        path.chmod(0o600)
        return str(path)

    def test_real_tls_returns_complete_tool_result(self):
        with self.server() as (connection, observed):
            status, result = EXAMPLE.query(self.save(connection), "search", {"query": "original fictional evidence"})
            self.assertEqual(status, 200)
            self.assertEqual(result, {"content": [], "structuredContent": {"arguments": {"query": "original fictional evidence"}, "future_field": {"retained": True}}, "isError": False})
            self.assertEqual(len(observed), 1)
            self.assertEqual(observed[0][1], "Bearer " + (self.root / "credential.token").read_text())

    def test_crossed_ca_or_token_identity_sends_no_credential(self):
        with self.server() as (connection, observed):
            original = connection["endpoint"]["ca_sha256"]
            connection["endpoint"]["ca_sha256"] = "00" * 32
            with self.assertRaisesRegex(ValueError, "CA identity"):
                EXAMPLE.query(self.save(connection), "search", {})
            connection["endpoint"]["ca_sha256"] = original
            connection["connection_id"] = "44" * 16
            with self.assertRaisesRegex(ValueError, "credential identity"):
                EXAMPLE.query(self.save(connection), "search", {})
            self.assertEqual(observed, [])
