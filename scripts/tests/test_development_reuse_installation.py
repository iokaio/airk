# SPDX-License-Identifier: Apache-2.0
"""Controls for the previous native lifetime and its exact physical control."""
from pathlib import Path
import hashlib
import json
import runpy
import tempfile
import unittest
from unittest.mock import patch


api = runpy.run_path(str(Path(__file__).resolve().parents[1] / "arbiter/analysis/development_margins/reuse_installation.py"))
restart = api["restart_inputs"]
E, R4, CONTROL = (api[k] for k in ("E", "R4", "CONTROL"))


class Reuse(unittest.TestCase):
    def original(self, count=13, active=False, foreign=False):
        entries = [{"path": str(R4 / f"lifetime-{i:02}" / "lifetime-closure.json"), "sha256": str(i)} for i in range(count)]
        base = {k: {"path": str(E / k), "sha256": k} for k in ("ready", "closed", "status")}
        current = {"queued": 1 if active else 0, "running": 0}
        status = {"focus": current, "context": current, "configuration": {"catalog": "catalog"}}
        owner = {"owned_descendants_settled": True, "owned_job_forced_termination": False, "owned_job_final_active_processes": 0,
                 "owned_job_final_member_pids": [], "timed_out": False, "exit_code": 0}
        terminal = {"plan": {"sha256": api["PLAN"]}, "ordered_questions": 100, "lifetimes": entries}
        def linked(pin):
            if pin == base["status"]:
                return status
            if "id" in pin:
                return {"instance": pin["id"], "control_root": str(CONTROL.parent if foreign else CONTROL),
                        "shutdown_completed": True, "service_lease_reacquired": True}
            ordinal = entries.index(pin)
            return {"phase": ordinal, "instance": ordinal, "closed": {"id": ordinal}}
        fields = {"latest_cal_receipt": lambda: R4 / "outer-owned.json", "inventory": lambda _: (base, {"health": {"catalog_clock": {"namespace": "catalog"}}}, {}),
                  "read": lambda path, seal=None: terminal if path.name == "fixture-closure.json" else owner, "linked": linked}
        with patch.dict(restart.__globals__, fields), patch.object(Path, "samefile", lambda a, b: a == b):
            return restart({}, R4 / "outer-owned.json", "frozen")

    def test_snapshot_keeps_mutable_bytes_and_authenticates_model_in_place(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            control, output = root / "control", root / "new-case"
            control.mkdir()
            output.mkdir()
            (control / "asset.onnx").write_bytes(b"pinned model")
            (control / "catalog.db").write_bytes(b"old")
            manifest = {"asset.onnx": hashlib.sha256(b"pinned model").hexdigest()}
            custody = api["custody"]
            with patch.dict(custody.__globals__, {"CONTROL": control}):
                before = custody(output, manifest, True)
                self.assertEqual((output / "control-before/catalog.db").read_bytes(), b"old")
                self.assertFalse((output / "control-before/asset.onnx").exists())
                (control / "catalog.db").write_bytes(b"new")
                after = custody(output, manifest, False)
                for pin, value in ((before, b"old"), (after, b"new")):
                    raw = Path(pin["path"]).read_bytes()
                    self.assertEqual(hashlib.sha256(raw).hexdigest(), pin["sha256"])
                    self.assertEqual(json.loads(raw)["files"]["catalog.db"], hashlib.sha256(value).hexdigest())
                (control / "asset.onnx").write_bytes(b"changed")
                with self.assertRaisesRegex(ValueError, "installed asset content changed"):
                    custody(output, manifest, False)

    def test_original_anchor(self):
        result, assets = self.original()
        self.assertEqual(set(result), {"ready", "closed", "status"})
        self.assertEqual(assets, {})

    def test_partial_lifetime_and_busy_stage_refused(self):
        with self.assertRaisesRegex(ValueError, "complete closure"):
            self.original(count=12)
        with self.assertRaisesRegex(ValueError, "idle status"):
            self.original(active=True)

    def test_foreign_physical_control_refused(self):
        with self.assertRaisesRegex(ValueError, "original installation"):
            self.original(foreign=True)


if __name__ == "__main__":
    unittest.main()
