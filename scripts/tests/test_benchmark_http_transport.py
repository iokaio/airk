# SPDX-License-Identifier: Apache-2.0
"""Real HTTPS benchmark transport; native enrollment is separately mocked."""
from concurrent.futures import ThreadPoolExecutor
import copy
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import os
import unittest
from unittest.mock import patch

import test_http_fixture as TLS

SPEC = importlib.util.spec_from_file_location("benchmark_https", Path(__file__).parents[1] / "benchmark-mcp-investigation.py")
BENCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCH)


class BenchmarkHttpsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        TLS.ActualHttpsFixtureTests.setUpClass()

    @classmethod
    def tearDownClass(cls):
        TLS.ActualHttpsFixtureTests.tearDownClass()

    def test_actual_tls_concurrent_requests_and_one_session_delete_without_frontend(self):
        fixture = TLS.ActualHttpsFixtureTests()
        with fixture.server() as (connection, observed):
            config = {"transport":"https", "control":str(fixture.root), "expected_version":"1.8.14"}
            with patch.object(BENCH, "enroll_https", return_value=connection), patch.object(BENCH.subprocess,"Popen",side_effect=AssertionError("no frontend process")):
                with BENCH.Session(config, 5, prime_status=False) as session:
                    before = session.tx_bytes, session.rx_bytes
                    with ThreadPoolExecutor(max_workers=2) as pool:
                        replies = list(pool.map(lambda _:session.request("ping",{}), range(4)))
                    self.assertTrue(all(reply[0]["fixture"] for reply in replies))
                    self.assertTrue(all(reply[1]["request_bytes"] > 0 and reply[1]["response_bytes"] > 0 for reply in replies))
                    self.assertEqual(session.tx_bytes-before[0],sum(reply[1]["request_bytes"] for reply in replies))
                    self.assertEqual(session.rx_bytes-before[1],sum(reply[1]["response_bytes"] for reply in replies))
                self.assertTrue(session.cleanup["http_session_delete"])
                self.assertFalse(session.cleanup["frontend_process_created"])
                self.assertGreaterEqual(session.cleanup["owned_tls_connections"],2)
            self.assertEqual(sum(method == "DELETE" for method,_ in observed),1)
            self.assertEqual(sum(method == "initialize" for method,_ in observed),1)
            self.assertEqual(sum(method == "ping" for method,_ in observed),4)

    def test_case_authority_is_narrowed_without_mutating_original(self):
        config = {"transport":"https", "selection":{"kind":"combined", "selection":{
            "grant_id":"unchanged", "project_ids":["p1","p2"], "index_ids":["i1","i2"]}}}
        original = copy.deepcopy(config)
        narrowed = BENCH.configuration_for_case(config,{"project_ids":["p1"],"index_ids":["i1"]})
        self.assertEqual(config,original)
        self.assertEqual(narrowed["selection"]["selection"],{"grant_id":"unchanged","project_ids":["p1"],"index_ids":["i1"]})
        for case in ({"project_ids":["outside"],"index_ids":["i1"]}, {"project_ids":["p1"],"index_ids":["outside"]}):
            with self.assertRaisesRegex(ValueError,"case_widens"):
                BENCH.configuration_for_case(config,case)

    def test_modern_configuration_rejects_embedded_secret_and_mixed_stdio(self):
        fixture = TLS.ActualHttpsFixtureTests()
        config = {"schema":2,"transport":"https","args":[],"route":"isolated-approved-runtime",
                  "control":str(fixture.root),"selection":{"kind":"combined","selection":{"project_ids":["p"]}},
                  "connections_file": str(fixture.root / "frozen-connections.json")}
        self.assertEqual(BENCH.modern_configuration(config,combined=True),config)
        for delta in ({"headers":{"Authorization":"PRIVATE_DUMMY"}}, {"args":["mcp"]}, {"selection":{"kind":"scoped","selection":{}}}):
            with self.assertRaises(ValueError):
                BENCH.modern_configuration({**config,**delta},combined=True)

    def test_exact_native_pre_enrollment_is_read_only_and_never_broadens_case(self):
        with tempfile.TemporaryDirectory() as folder:
            control = Path(folder).resolve()
            private = control / "http-1"; private.mkdir()
            identity = "a" * 32
            selected = {"kind":"combined", "selection":{"project_ids":["p"], "index_ids":["i"], "expected_catalog_revision":3}}
            endpoint = {"service_instance":"native-owner", "user":"fixture", "namespace":"native-namespace", "executable_sha256":"native-image"}
            health = {"instance":"native-owner", "user":"fixture", "catalog_clock":{"namespace":"native-namespace"}, "executable_sha256":"native-image"}
            connection = {"schema":1, "connection_id":identity, "selection":copy.deepcopy(selected), "endpoint":endpoint,
                          "credential_file":str(private / f"credential-{identity}.token"), "ca_certificate_file":str(private / "ca.pem")}
            for field in ("credential_file", "ca_certificate_file"):
                Path(connection[field]).write_text("fixture-only")
                os.chmod(connection[field], 0o600)
            roster_file = private / "frozen.json"
            def write_roster(rows, client="unknown"):
                roster_file.write_text(json.dumps({"schema":1,"client":client,"connections":rows}))
                os.chmod(roster_file, 0o600)
            write_roster([connection])
            config = {"command":"native-fixture", "control":str(control), "selection":selected, "connections_file":str(roster_file)}
            def run(command, **kwargs):
                self.assertEqual(command, ["native-fixture", "service", "rpc", "--runtime-dir", str(control), "--stdin"])
                self.assertEqual(json.loads(kwargs["input"]), "health", "private enrollment must never cross public console")
                self.assertGreater(kwargs["timeout"], 0)
                return SimpleNamespace(returncode=0, stdout=json.dumps({"status":"health","observation":health}).encode())
            with patch.object(BENCH.subprocess, "run", side_effect=run) as observed:
                self.assertEqual(BENCH.enroll_https(config, 5), connection)
                self.assertEqual(observed.call_count, 1)
                write_roster([connection, connection])
                with self.assertRaisesRegex(ValueError, "exact_selection"):
                    BENCH.enroll_https(config, 5)
                broader = copy.deepcopy(connection); broader["selection"]["selection"]["index_ids"].append("outside")
                write_roster([broader])
                with self.assertRaisesRegex(ValueError, "exact_selection"):
                    BENCH.enroll_https(config, 5)
                write_roster([connection], "codex")
                with self.assertRaisesRegex(ValueError, "roster_invalid"):
                    BENCH.enroll_https(config, 5)


if __name__ == "__main__":
    unittest.main()
