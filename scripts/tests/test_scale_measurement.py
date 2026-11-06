# SPDX-License-Identifier: Apache-2.0
"""Exercise benchmark process accounting and child failure handling."""
import importlib.util
import json
from pathlib import Path
import platform
import sys
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("benchmark_scale", Path(__file__).parents[1] / "benchmark-scale.py")
BENCHMARK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCHMARK)


class ScaleMeasurementTests(unittest.TestCase):
    def test_child_resources_are_observed_without_a_usage_ledger(self):
        with tempfile.TemporaryDirectory() as directory:
            # Generate measurable CPU work as well as an RSS sampling window;
            # sleeping alone does not guarantee a nonzero process CPU sample.
            response, metrics = BENCHMARK.measured(
                [sys.executable, "-c", "import json,time\nvalues=list(range(100000))\n"
                 "deadline=time.process_time()+0.06\n"
                 "while time.process_time()<deadline:\n    sum(values)\n"
                 "time.sleep(0.12)\nprint(json.dumps({'sum':sum(values)}))"],
                Path(directory), "child", timeout=10)
            self.assertEqual(response, {"sum": 4_999_950_000})
            self.assertEqual(metrics["exit_code"], 0)
            self.assertGreater(metrics["wall_ms"], 0)
            if platform.system() in ("Windows", "Linux"):
                self.assertGreater(metrics["cpu_us"], 0)
                self.assertGreater(metrics["peak_rss_bytes"], 0)

    @unittest.skipUnless(platform.system() == "Linux", "Linux fork/exec RSS accounting")
    def test_launcher_memory_is_not_reported_as_executable_rss(self):
        # Keep the parent resident through launch so wait4 can expose its
        # inherited high-water mark even though exec discards this allocation.
        launcher_buffer = bytearray(128 * 1024 * 1024)
        try:
            with tempfile.TemporaryDirectory() as directory:
                _, metrics = BENCHMARK.measured(
                    [sys.executable, "-c", "import time; time.sleep(0.15); print('{}')"],
                    Path(directory), "small-child")
            self.assertGreater(metrics["rss_samples"], 0)
            self.assertLess(metrics["peak_rss_bytes"], len(launcher_buffer) // 2)
            self.assertGreater(metrics["child_lifetime_peak_rss_bytes"], metrics["peak_rss_bytes"])
        finally:
            del launcher_buffer

    def test_failed_child_cannot_be_reported_as_success(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(RuntimeError, "deliberate fixture failure"):
                BENCHMARK.measured(
                    [sys.executable, "-c", "import sys; print('deliberate fixture failure', file=sys.stderr); sys.exit(7)"],
                    Path(directory), "failure")


    def test_only_explicit_read_path_can_return_nonzero_typed_rejection(self):
        hint = {"status": "retry", "retryable": True, "code": "selection_busy",
                "retry_after_ms": 25, "action": "repeat_same_request"}
        envelope = {"isError": True, "content": [{"type": "text", "text": json.dumps(hint)}]}
        command = [sys.executable, "-c", "import sys; print(" + repr(json.dumps(envelope)) + "); sys.exit(1)"]
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(RuntimeError):
                BENCHMARK.measured(command, Path(directory), "ordinary", timeout=10)
            actual, metrics = BENCHMARK.measured(command, Path(directory), "read", timeout=10,
                                                 allow_typed_read_rejection=True)
            self.assertEqual(actual, envelope)
            self.assertEqual(metrics["exit_code"], 1)
            self.assertEqual(BENCHMARK.clients.read_retry_delay(actual), 0.025)

    def test_exit_two_with_typed_stdout_is_terminal_without_read_replay(self):
        hint = {"status": "retry", "retryable": True, "code": "selection_busy",
                "retry_after_ms": 25, "action": "repeat_same_request"}
        envelope = {"isError": True, "content": [{"type": "text", "text": json.dumps(hint)}]}
        command = [sys.executable, "-c", "import sys; print(" + repr(json.dumps(envelope)) + "); sys.exit(2)"]
        with tempfile.TemporaryDirectory() as directory:
            owner = Path(directory)
            with self.assertRaises(RuntimeError):
                BENCHMARK.measured_read(command, owner, "terminal-exit-two", timeout=10)
            self.assertEqual(json.loads((owner / "terminal-exit-two.json").read_text()), envelope)
            self.assertFalse((owner / "terminal-exit-two-retry-1.json").exists())
            saved = json.loads((owner / "terminal-exit-two-attempts.json").read_text())
            self.assertFalse(saved["completed"])
            self.assertEqual(saved["read_attempt_count"], 1)
            self.assertEqual(saved["read_retry_count"], 0)
            self.assertEqual(saved["attempts"][0]["failure_type"], "RuntimeError")

    def test_same_read_retry_preserves_original_deadline_and_all_attempt_costs(self):
        clock = [0.0]
        hint = {"status": "retry", "retryable": True, "code": "selection_busy",
                "retry_after_ms": 25, "action": "repeat_same_request"}
        rejected = {"isError": True, "content": [{"text": json.dumps(hint), "type": "text"}]}
        success = {"isError": False, "content": [{"text": '{"hits":["exact"]}', "type": "text"}]}
        command = ["unchanged.exe", "search", "sentinel", "--limit", "1"]
        calls = []

        def measure(actual, directory, label, timeout, environment, *, allow_typed_read_rejection):
            calls.append((list(actual), label, timeout, environment, allow_typed_read_rejection))
            clock[0] += 0.2
            ordinal = len(calls)
            return (rejected if ordinal == 1 else success), {
                "label": label, "exit_code": 1 if ordinal == 1 else 0,
                "cpu_us": ordinal * 10, "read_bytes": ordinal * 20,
                "peak_rss_bytes": ordinal * 100, "wall_ms": 200}

        with tempfile.TemporaryDirectory() as directory, \
                mock.patch.object(BENCHMARK, "measured", side_effect=measure), \
                mock.patch.object(BENCHMARK.time, "monotonic", side_effect=lambda: clock[0]), \
                mock.patch.object(BENCHMARK.time, "sleep", side_effect=lambda delay: clock.__setitem__(0, clock[0] + delay)):
            value, metrics = BENCHMARK.measured_read(command, Path(directory), "same", timeout=1,
                                                    environment={"fixture": "same"})
            self.assertIs(value, success)
            self.assertEqual([call[0] for call in calls], [command, command])
            self.assertEqual([call[1] for call in calls], ["same", "same-retry-1"])
            self.assertEqual(calls[0][2], 1)
            self.assertAlmostEqual(calls[1][2], 0.775)
            self.assertTrue(all(call[3:] == ({"fixture": "same"}, True) for call in calls))
            self.assertEqual(metrics["read_attempt_count"], 2)
            self.assertEqual(metrics["read_retry_count"], 1)
            self.assertEqual(metrics["cpu_us"], 30)
            self.assertEqual(metrics["read_bytes"], 60)
            self.assertEqual(metrics["peak_rss_bytes"], 200)
            self.assertNotIn("write_bytes", metrics)  # Missing is not zero.
            self.assertGreaterEqual(metrics["wall_ms"], 425)
            saved = json.loads((Path(directory) / "same-attempts.json").read_text())
            self.assertTrue(saved["completed"])
            self.assertEqual([item["exit_code"] for item in saved["attempts"]], [1, 0])

    def test_read_deadline_and_attempt_exhaustion_stay_failed_and_retained(self):
        hint = {"status": "retry", "retryable": True, "code": "publication_busy",
                "retry_after_ms": 25, "action": "repeat_same_request"}
        rejected = {"isError": True, "content": [{"text": json.dumps(hint), "type": "text"}]}
        for timeout, elapsed, expected_attempts in [(0.21, 0.2, 1), (10, 0, 41)]:
            with self.subTest(timeout=timeout), tempfile.TemporaryDirectory() as directory:
                clock = [0.0]

                def measure(*args, **kwargs):
                    clock[0] += elapsed
                    return rejected, {"label": args[2], "exit_code": 1}

                with mock.patch.object(BENCHMARK, "measured", side_effect=measure) as measured, \
                        mock.patch.object(BENCHMARK.time, "monotonic", side_effect=lambda: clock[0]), \
                        mock.patch.object(BENCHMARK.time, "sleep", side_effect=lambda delay: clock.__setitem__(0, clock[0] + delay)):
                    with self.assertRaisesRegex(AssertionError, "no further safe replay"):
                        BENCHMARK.measured_read(["same.exe", "search", "exact"], Path(directory), "exhausted", timeout=timeout)
                    self.assertEqual(measured.call_count, expected_attempts)
                saved = json.loads((Path(directory) / "exhausted-attempts.json").read_text())
                self.assertFalse(saved["completed"])
                self.assertEqual(saved["read_attempt_count"], expected_attempts)
                self.assertTrue(all(attempt["exit_code"] == 1 for attempt in saved["attempts"]))

    def test_terminal_read_failure_is_not_replayed_and_summary_stays_failed(self):
        with tempfile.TemporaryDirectory() as directory, \
                mock.patch.object(BENCHMARK, "measured", side_effect=RuntimeError("non-retry failure")) as measured:
            with self.assertRaisesRegex(RuntimeError, "non-retry failure"):
                BENCHMARK.measured_read(["same.exe", "search", "exact"], Path(directory), "terminal", timeout=1)
            self.assertEqual(measured.call_count, 1)
            saved = json.loads((Path(directory) / "terminal-attempts.json").read_text())
            self.assertFalse(saved["completed"])
            self.assertEqual(saved["attempts"][0]["failure_type"], "RuntimeError")

    def test_native_index_and_configuration_never_enter_read_retry_path(self):
        with tempfile.TemporaryDirectory() as directory:
            owner = Path(directory)
            fixture = mock.Mock()
            fixture.binary = Path("native.exe")
            fixture.arguments.return_value = ["--collection", "probe"]
            fixture.projects = {"probe": {"binding": "owned"}}
            runner = BENCHMARK.native_runner(fixture, owner, owner / "state", owner, "probe", [], 1)
            with mock.patch.object(BENCHMARK, "measured", side_effect=RuntimeError("single mutation")) as measured, \
                    mock.patch.object(BENCHMARK, "measured_read") as retried:
                with self.assertRaisesRegex(RuntimeError, "single mutation"):
                    runner("index", "index")
                self.assertEqual(measured.call_count, 1)
                retried.assert_not_called()
            configuration = owner / "configuration.json"
            configuration.write_text('{"operation":"vectors","enabled":false}')
            fixture.configure.reset_mock()
            fixture.configure.side_effect = RuntimeError("single configuration")
            with mock.patch.object(BENCHMARK, "measured_read") as retried:
                with self.assertRaisesRegex(RuntimeError, "single configuration"):
                    runner("configure", "configure", "--file", str(configuration))
                self.assertEqual(fixture.configure.call_count, 1)
                retried.assert_not_called()


if __name__ == "__main__":
    unittest.main()
