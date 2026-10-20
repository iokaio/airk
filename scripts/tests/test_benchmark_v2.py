# SPDX-License-Identifier: Apache-2.0
"""Original fixture identity and persistent MCP benchmark failure contracts."""
import importlib.util
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


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
    def test_local_model_campaign_checks_cache_counts_and_exact_query_evidence(self):
        for fault in (None, "counts", "citation"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root, state = Path(temporary) / "sources", Path(temporary) / "state"
                step = 0
                def run(label, command, *args):
                    nonlocal step
                    if command == "index":
                        step += 1
                        return {"generation": 2 if step == 3 else 1,
                                "indexed": (2, 0, 1)[step - 1], "reused": (0, 2, 1)[step - 1],
                                "embedding_inferences": (2, 0, 1)[step - 1], "embedding_no_vector": 0,
                                "embedding_cache_hits": int(fault == "counts")}
                    if command == "search":
                        response = self.read_response("login.txt", (root / "login.txt").read_bytes(), 2)
                        response.pop("read")
                        if fault == "citation":
                            response["hits"][0]["citation"]["content_hash"] = "0" * 64
                        return response
                    if command == "read":
                        return self.read_response(args[0], (root / args[0]).read_bytes(), 2)
                    self.fail("unexpected model command")
                if fault:
                    with self.assertRaises(AssertionError):
                        BENCHMARK.local_model_workload(root, state, run)
                else:
                    receipt = BENCHMARK.local_model_workload(root, state, run)
                    vocabulary = (state / "models/local/vocabulary.json").read_bytes()
                    self.assertEqual(receipt["model"]["sha256"], hashlib.sha256(vocabulary).hexdigest())
                    self.assertEqual(len(json.loads(vocabulary)), 2)
                    self.assertIn("unqualified", receipt["quality"])

    def test_mixed_fixture_has_real_mapped_sources_and_repeated_boilerplate(self):
        fixture = BENCHMARK.mixed_fixture()
        self.assertEqual(fixture, BENCHMARK.mixed_fixture())
        notebook = json.loads(fixture["mixed/notes.ipynb"])
        self.assertEqual(notebook["nbformat"], 4)
        self.assertIn("mapped_metric", "".join(notebook["cells"][1]["source"]))
        self.assertIn(b"\r\n<script setup", fixture["mixed/component.vue"])
        self.assertEqual(fixture["mixed/boilerplate-a.txt"], fixture["mixed/boilerplate-b.txt"])

    @staticmethod
    def read_response(path, content, generation=2):
        return {"generation": generation, "truncated": False, "read": {"status": "content"},
                "hits": [{"text": content.decode(), "truncated": False, "citation": {
                    "path": path, "content_hash": hashlib.sha256(content).hexdigest(),
                    "start_line": 1, "end_line": len(content.splitlines())}}]}

    def test_exact_retained_read_rejects_wrong_hash_bytes_generation_or_bounds(self):
        content = "Original café\r\nSecond line\r\n".encode()
        valid = self.read_response("one.md", content)
        BENCHMARK.verify_read(valid, "one.md", content, 2)
        mutations = [lambda r: r.update(generation=3), lambda r: r.update(truncated=True),
                     lambda r: r["hits"][0].update(text="unindexed live text"),
                     lambda r: r["hits"][0]["citation"].update(content_hash="0" * 64),
                     lambda r: r["hits"][0]["citation"].update(end_line=99),
                     lambda r: r["hits"][0]["citation"].update(path="other.md")]
        for mutate in mutations:
            response = json.loads(json.dumps(valid))
            mutate(response)
            with self.assertRaises(AssertionError):
                BENCHMARK.verify_read(response, "one.md", content, 2)

    def extended_backend(self, root, invalid_branch=False, live_reads=False):
        retained = {p.relative_to(root).as_posix(): p.read_bytes() for p in root.rglob("*") if p.is_file()}
        generation = 1
        names = {"mixed/code.rs": "mixed_metric", "mixed/component.vue": "mappedMetric",
                 "mixed/notes.ipynb": "mapped_metric", "mixed/requirements.md": "REQ-900"}

        def run(label, command, *args):
            nonlocal retained, generation
            if command == "index":
                current = {p.relative_to(root).as_posix(): p.read_bytes() for p in root.rglob("*") if p.is_file()}
                indexed = sum(retained.get(path) != content for path, content in current.items())
                removed = len(retained.keys() - current.keys())
                if current != retained:
                    generation += 1
                retained = current
                return {"generation": generation, "indexed": indexed + int(invalid_branch and label == "branch-replacement"),
                        "reused": len(current) - indexed, "removed": removed}
            if command == "read":
                content = (root / args[0]).read_bytes() if live_reads else retained[args[0]]
                return self.read_response(args[0], content, generation)
            if command == "outline":
                return {"facts": [{"name": names[args[0]]}]}
            self.fail("unexpected backend command: " + command)
        return run

    def test_extended_campaign_checks_branch_reuse_and_committed_source_reads(self):
        for fault, message in ((None, None), ("branch", "branch replacement"),
                               ("live", "retained citation/text")):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as directory:
                root, state = Path(directory) / "source", Path(directory) / "state"
                BENCHMARK.fixture(root, 3)
                state.mkdir()
                run = self.extended_backend(root, invalid_branch=fault == "branch", live_reads=fault == "live")
                if fault:
                    with self.assertRaisesRegex(AssertionError, message):
                        BENCHMARK.extended_workloads(root, state, run, 1, 3, 0)
                else:
                    report = BENCHMARK.extended_workloads(root, state, run, 1, 3, 0)
                    self.assertEqual(report["child_edits"]["indexed"], 2)
                    self.assertEqual(len(report["branch_replacement"]["removed"]), 2)
                    self.assertEqual(report["warm_index"]["indexed"], 0)
                    self.assertTrue(report["correctness_checks_passed"])

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

    def test_worker_exits_before_initialize_send_and_cleanup_preserves_error(self):
        popen = BENCHMARK.subprocess.Popen

        def already_exited(*args, **kwargs):
            process = popen(*args, **kwargs)
            process.wait(timeout=5)
            return process

        # Guarantee that the child has exited before the initializer can write,
        # rather than relying on the runner's thread/process scheduling order.
        reader = BENCHMARK.McpReader.__new__(BENCHMARK.McpReader)
        with mock.patch.object(BENCHMARK.subprocess, "Popen", side_effect=already_exited):
            with self.assertRaisesRegex(RuntimeError, "closed stdout or input pipe") as caught:
                reader.__init__([sys.executable, "-c", "pass"], timeout=2)
        self.assertIsInstance(caught.exception.__cause__, OSError)
        self.assertIsNotNone(reader.process.returncode)
        self.assertFalse(reader.thread.is_alive())
        self.assertTrue(reader.process.stdin.closed)
        self.assertTrue(reader.process.stdout.closed)
        self.assertTrue(reader.errors.closed)
        reader.close()  # Cleanup also remains safe when called twice.

    def test_unresponsive_worker_is_bounded(self):
        with self.assertRaises(TimeoutError):
            BENCHMARK.McpReader([sys.executable, "-c", "import time; time.sleep(30)"], timeout=0.2)


if __name__ == "__main__":
    unittest.main()
