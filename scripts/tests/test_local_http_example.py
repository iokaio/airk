# SPDX-License-Identifier: Apache-2.0
"""The standalone Python developer example reads over real loopback HTTP (local trust plan C)."""
import importlib.util
import json
from pathlib import Path
import unittest
import uuid


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ROOT = Path(__file__).resolve().parents[2]
BASE = load("local_api_http_fixture", ROOT / "scripts/tests/test_http_fixture.py")
EXAMPLE = load("local_http_example", ROOT / "examples/local_http_client.py")


class ScopeArgumentTests(unittest.TestCase):
    def test_scope_options_merge_into_tool_arguments(self):
        workspace = str(Path(__file__).resolve().parent)
        self.assertEqual(EXAMPLE.scoped({"query": "q", "index_ids": ["a"]}, ["a", "b"], ["s"], workspace),
                         {"query": "q", "index_ids": ["a", "b"], "index_set_ids": ["s"], "workspace": workspace})
        self.assertEqual(EXAMPLE.scoped({"query": "q"}), {"query": "q"})

    def test_relative_or_conflicting_workspace_is_refused(self):
        with self.assertRaisesRegex(ValueError, "workspace"):
            EXAMPLE.scoped({}, workspace="relative/folder")
        absolute = str(Path(__file__).resolve().parent)
        with self.assertRaisesRegex(ValueError, "workspace"):
            EXAMPLE.scoped({"workspace": absolute + "-other"}, workspace=absolute)
        with self.assertRaisesRegex(ValueError, "index_ids"):
            EXAMPLE.scoped({"index_ids": "a"}, ["b"])


class LocalHttpExampleTests(unittest.TestCase):
    setUpClass = classmethod(BASE.ActualHttpFixtureTests.setUpClass.__func__)
    tearDownClass = classmethod(BASE.ActualHttpFixtureTests.tearDownClass.__func__)
    server = BASE.ActualHttpFixtureTests.server

    def control(self, endpoint):
        """A fresh service control directory holding http-1/endpoint.json alone."""
        control = self.root / ("control-" + uuid.uuid4().hex)
        native = control / "http-1"
        native.mkdir(parents=True)
        (native / "endpoint.json").write_text(json.dumps(endpoint), encoding="utf-8")
        (native / "endpoint.json").chmod(0o600)
        return str(control)

    def test_real_http_returns_complete_tool_result_without_authorization(self):
        with self.server() as (connection, observed):
            workspace = str(self.root)
            arguments = EXAMPLE.scoped({"query": "original fictional evidence"}, ["alpha"], ["team"], workspace)
            status, result = EXAMPLE.query(self.control(connection["endpoint"]), "search", arguments)
            self.assertEqual(status, 200)
            self.assertEqual(result, {"content": [], "structuredContent": {"arguments": {"query": "original fictional evidence", "index_ids": ["alpha"], "index_set_ids": ["team"], "workspace": workspace}, "future_field": {"retained": True}}, "isError": False})
            self.assertEqual(observed, [(None, None)])

    def test_legacy_https_unready_or_named_host_endpoint_sends_nothing(self):
        with self.server() as (connection, observed):
            endpoint = dict(connection["endpoint"], url=connection["endpoint"]["url"].replace("http://", "https://"))
            with self.assertRaisesRegex(ValueError, "loopback"):
                EXAMPLE.query(self.control(endpoint), "search", {})
            endpoint = dict(connection["endpoint"], state="stopping")
            with self.assertRaisesRegex(ValueError, "not ready"):
                EXAMPLE.query(self.control(endpoint), "search", {})
            endpoint = dict(connection["endpoint"], url=connection["endpoint"]["url"].replace("127.0.0.1", "localhost"))
            with self.assertRaisesRegex(ValueError, "loopback"):
                EXAMPLE.query(self.control(endpoint), "search", {})
            with self.assertRaisesRegex(ValueError, "absolute"):
                EXAMPLE.query("relative-control", "search", {})
            self.assertEqual(observed, [])


if __name__ == "__main__":
    unittest.main()
