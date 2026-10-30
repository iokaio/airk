# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("github_read", Path(__file__).parents[1] / "github-read.py")
reads = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reads)


class ReadRetries(unittest.TestCase):
    def test_transient_errors_retry_get_but_auth_and_invalid_data_do_not(self):
        for status, attempts in ((502, 4), (504, 4), (429, 4), (401, 1), (403, 1), (404, 1)):
            response = subprocess.CompletedProcess([], 1, b"", f"gh: error (HTTP {status})".encode())
            with self.subTest(status=status), patch.object(reads.subprocess, "run", return_value=response) as run, \
                    patch.object(reads.time, "sleep"):
                with self.assertRaises(RuntimeError):
                    reads.get("repos/iokaio/airk/actions/runs/123")
                self.assertEqual(run.call_count, attempts)
                self.assertEqual(run.call_args.args[0][-2:], ["--method", "GET"])
        response = subprocess.CompletedProcess([], 0, b"not JSON", b"")
        with patch.object(reads.subprocess, "run", return_value=response) as run, self.assertRaises(ValueError):
            reads.get("repos/iokaio/airk/actions/runs/123")
        self.assertEqual(run.call_count, 1)

    def test_missing_is_only_an_explicit_404_and_retry_deadline_is_bounded(self):
        response = subprocess.CompletedProcess([], 1, b"", b"gh: missing (HTTP 404)")
        with patch.object(reads.subprocess, "run", return_value=response) as run:
            self.assertIsNone(reads.get("repos/iokaio/airk/releases/tags/v1.7.3", missing=True))
            self.assertEqual(run.call_count, 1)
        with patch.object(reads.time, "monotonic", side_effect=[0, 0, 90]), \
                patch.object(reads.subprocess, "run", side_effect=subprocess.TimeoutExpired("gh", 30)) as run, \
                patch.object(reads.time, "sleep") as sleep:
            with self.assertRaises(subprocess.TimeoutExpired):
                reads.get("repos/iokaio/airk/actions/runs/123")
            self.assertEqual(run.call_count, 1)
            sleep.assert_not_called()

    def test_partial_download_is_truncated_before_retry(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "artifact.zip"
            calls = []
            def transfer(command, stdout, **kwargs):
                calls.append(command)
                stdout.write(b"partial stale bytes" if len(calls) == 1 else b"verified")
                return subprocess.CompletedProcess(command, 1 if len(calls) == 1 else 0, None,
                                                   b"HTTP 502" if len(calls) == 1 else b"")
            with patch.object(reads.subprocess, "run", side_effect=transfer), patch.object(reads.time, "sleep"):
                reads.get("repos/iokaio/airk/actions/artifacts/7/zip", output=output)
            self.assertEqual(output.read_bytes(), b"verified")
            self.assertEqual(len(calls), 2)
