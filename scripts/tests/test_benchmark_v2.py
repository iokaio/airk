# SPDX-License-Identifier: Apache-2.0
"""Original fixture identity and persistent MCP benchmark failure contracts."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest


SPEC = importlib.util.spec_from_file_location("benchmark_v2", Path(__file__).parents[1] / "benchmark-v2.py")
BENCHMARK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCHMARK)

SERVER = """
import json, sys
calls = 0
for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request:
        continue
    if request['method'] == 'initialize':
        result = {}
    else:
        calls += 1
        result = {'content': [{'type': 'text', 'text': json.dumps({'path': 'one.ts', 'request_id': str(calls)})}]}
    print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}), flush=True)
"""


class V2BenchmarkTests(unittest.TestCase):
    def test_fixture_identity_is_reproducible_and_tracks_edits(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory) / "first", Path(directory) / "second"
            BENCHMARK.fixture(first, 3)
            BENCHMARK.fixture(second, 3)
            before = BENCHMARK.SCALE.source_digest(first)
            self.assertEqual(before, BENCHMARK.SCALE.source_digest(second))
            self.assertEqual(before["files"], 5)
            (second / "module_0000.ts").write_bytes(b"export function changed() {}\n")
            self.assertNotEqual(before["inventory_sha256"], BENCHMARK.SCALE.source_digest(second)["inventory_sha256"])

    def test_nearest_rank_has_observed_tail_and_rejects_empty(self):
        self.assertEqual(BENCHMARK.percentiles([4, 1, 2, 3]), {"samples": 4, "p50_ms": 2, "p95_ms": 4})
        with self.assertRaises(ValueError):
            BENCHMARK.percentiles([])

    def test_warm_reads_share_worker_despite_distinct_request_ids(self):
        result = BENCHMARK.read_samples([sys.executable, "-u", "-c", SERVER], "one.ts", 2, 3)
        self.assertEqual(result["cold_worker"]["samples"], 2)
        self.assertEqual(result["warm_worker"]["samples"], 6)
        reader = BENCHMARK.McpReader([sys.executable, "-u", "-c", SERVER])
        try:
            self.assertEqual(reader.info("one.ts")["request_id"], "1")
            self.assertEqual(reader.info("one.ts")["request_id"], "2")
        finally:
            reader.close()
        self.assertIsNotNone(reader.process.returncode)
        self.assertFalse(reader.thread.is_alive())

    def test_tool_failure_is_never_a_latency_success(self):
        server = SERVER.replace("result = {'content':", "result = {'isError': True, 'content':")
        reader = BENCHMARK.McpReader([sys.executable, "-u", "-c", server])
        try:
            with self.assertRaisesRegex(RuntimeError, "MCP tool failed"):
                reader.info("one.ts")
        finally:
            reader.close()

    def test_metadata_changes_and_wrong_file_are_rejected(self):
        changed = SERVER.replace("'request_id': str(calls)", "'generation': calls")
        with self.assertRaisesRegex(AssertionError, "metadata changed"):
            BENCHMARK.read_samples([sys.executable, "-u", "-c", changed], "one.ts", 1, 1)
        with self.assertRaisesRegex(AssertionError, "different file"):
            BENCHMARK.read_samples([sys.executable, "-u", "-c", SERVER], "other.ts", 1, 1)

    def test_closed_worker_fails_promptly(self):
        with self.assertRaisesRegex(RuntimeError, "closed stdout"):
            BENCHMARK.McpReader([sys.executable, "-c", "pass"], timeout=2)

    def test_unresponsive_worker_is_bounded(self):
        with self.assertRaises(TimeoutError):
            BENCHMARK.McpReader([sys.executable, "-c", "import time; time.sleep(30)"], timeout=0.2)


if __name__ == "__main__":
    unittest.main()
