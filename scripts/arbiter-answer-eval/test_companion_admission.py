# SPDX-License-Identifier: Apache-2.0
"""Reject stale native service bindings and any activity by the disabled companion model stages."""
import hashlib
import json
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import common
import companion_admission as admission


class AdmissionTest(unittest.TestCase):
    def test_bound_instance_and_stage_counters(self):
        with TemporaryDirectory() as folder:
            base = Path(folder)
            (base / "fixture").mkdir()
            control = base / "control"
            control.mkdir()
            digest = "a" * 64
            handle = base / "handle.json"
            handle.write_text(json.dumps({"state": "ready", "instance": "S", "search": "none", "service_pid": 42,
                                          "image": {"service": {"sha256": digest}}, "control": str(control)}))
            observers = {}
            for name, file in (("dispatch", "composed-observer.jsonl"), ("warm", "warm-completions.jsonl")):
                target = base / "fixture" / file
                target.write_bytes(b"")
                observers[name] = {"path": str(target), "rows": 0, "bytes": 0, "sha256": hashlib.sha256(b"").hexdigest()}
            stage = {"enabled": False, "queued": 0, "running": 0}
            proof = {"schema": "airk.eval.companion-admission/1", "search": "none", "service_instance": "S",
                     "service_pid": 42, "handle_path": str(handle), "handle_sha256": common.sha256_file(handle),
                     "service_sha256": digest, "stage_status": {"focus": stage, "context": stage},
                     "observers": observers, "embedding": {"model": "MiniLM", "route": "cpu", "asset_sha256": digest,
                                                           "graph_sha256": digest, "pipeline_digest": digest}}
            proof_path = base / "proof.json"
            proof_path.write_text(json.dumps(proof))
            binding = SimpleNamespace(path=handle, control=str(control), sha256=common.sha256_file(handle),
                                      cli=("checked-test-cli",))
            native = {"configuration": {"focus": {"enabled": False}, "context": {"enabled": False}},
                      "focus": {"queued": 0, "running": 0}, "context": {"queued": 0, "running": 0}}
            with patch.object(admission.product_skill_http.http_fixture, "read_endpoint", return_value={
                        "service_instance": "S", "executable_sha256": digest}), patch.object(admission.subprocess, "run") as cmd:
                cmd.return_value.stdout = json.dumps(native).encode()
                self.assertEqual(admission.observe(binding, proof_path)["legacy_stages"], proof["stage_status"])
                native["focus"]["running"] = 1
                cmd.return_value.stdout = json.dumps(native).encode()
                with self.assertRaisesRegex(common.HarnessError, "pending work"):
                    admission.observe(binding, proof_path)
                native["focus"]["running"] = 0
                cmd.return_value.stdout = json.dumps(native).encode()
                Path(observers["warm"]["path"]).write_bytes(b"unexpected\n")
                with self.assertRaisesRegex(common.HarnessError, "observer contains activity"):
                    admission.observe(binding, proof_path)