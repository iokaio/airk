# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch


HERE = Path(__file__).resolve().parents[1] / "arbiter/analysis/development_margins"
group = runpy.run_path(str(HERE / "grouped_reuse.py"))
reuse = runpy.run_path(str(HERE / "reuse_installation.py"))


def put(path, data):
    raw = json.dumps(data).encode()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(raw)
    return {"path": str(path), "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)}


def fixture(root, success, forced=False):
    r3 = root / "cal100-focus1024-capture-r3"
    r4 = root / "cal100-focus1024-capture-r4"
    r5 = root / "cal100-focus1024-capture-r5"
    control = str(r3 / "fixture/control")
    original = {"control_root": control, "inputs_sha256": "roster", "profiles": ["focus", "context"],
                "health": {"instance": "r3", "executable_sha256": "exe"}}
    status = {"configuration": {"revision": 7}}
    for stage in ("focus", "context"):
        status[stage] = {"availability": "ready", "queued": 0, "running": 0, "active_profile": stage}
    installation = {"ready": put(r3 / "fixture/ready.json", original),
                    "status": put(r3 / "commands/cal-ready.stdout", status)}
    settled = {"owned_descendants_settled": True, "owned_job_forced_termination": False, "timed_out": False,
               "owned_job_final_active_processes": 0, "owned_job_final_member_pids": []}
    previous = put(r4 / "outer-owned.json", settled | {"exit_code": 1})
    acquired = put(r5 / "control-mutex-acquired.json", {"name": group["MUTEX"], "owner_pid": 42, "at_ms": 700})
    phases = [list(range(4)), list(range(4,8)), list(range(8,13))]
    plan = put(r5 / "lifetime-plan.json", {"installation": installation, "prior_attempt": previous,
               "installation_mutex": acquired, "phase_groups": phases})
    links = []
    result_code = 0 if success else 1
    for number in range(3 if success else 2):
        folder = r5 / f"lifetime-{number:02}"
        instance = "r5-" + str(number)
        put(folder / "fixture/ready.json", original | {"health": original["health"] | {"instance": instance}})
        closed = put(folder / "fixture/closed.json", {"instance": instance, "control_root": control,
                     "service_lease_reacquired": True, "shutdown_completed": True})
        label = "stage-after" if success or number == 0 else "failure-stage-at-shutdown"
        put(folder / "commands" / (label + ".stdout"), status)
        put(folder / "commands" / (label + ".command.json"), {"argv": ["airkctl", "--output", "json",
                    "--service-runtime-dir", control, "arbiter", "status-legacy"], "exit_code": 0})
        if label == "stage-after":
            links.append(put(folder / "lifetime-closure.json", {"group": number, "phases": phases[number],
                         "instance": instance, "closed": closed, "plan": plan}))
    if success:
        put(r5 / "fixture-closure.json", {"schema": "airk.private.cal100-grouped-fixture/1", "ordered_questions": 100,
                                         "plan": plan, "lifetimes": links})
    outer = put(r5 / "outer-owned.json", settled | {"exit_code": result_code, "owned_job_forced_termination": forced,
                "argv": ["interpreter", "--output", str(r5), "--owned-plan-sha256", plan["sha256"]]})
    release = put(r5 / "control-mutex-released.json", {"acquired": acquired, "outer": outer, "name": group["MUTEX"],
                  "owner_pid": 42, "at_ms": 1000})
    freeze = put(root / "cal100-focus1024-capture-r5-freeze.json", [])
    custody = put(root / "cal100-focus1024-capture-r5-custody.json", {"exit_code": result_code,
                  "owner_sid": group["SID"], "changed_input_count": 0, "output_count": 12, "freeze_sha256": freeze["sha256"]})
    return root, release["sha256"], custody["sha256"], installation


class GroupedHandoff(unittest.TestCase):
    def test_acquired_group_refuses_stale_r4_even_before_release(self):
        latest = reuse["latest_cal_receipt"]
        with tempfile.TemporaryDirectory() as tmp:
            r4, r5 = Path(tmp) / "r4", Path(tmp) / "r5"
            with patch.dict(latest.__globals__, {"R4": r4, "R5": r5}):
                self.assertEqual(latest(), r4 / "outer-owned.json")
                put(r5 / "control-mutex-acquired.json", {"owner_pid": 42})
                self.assertEqual(latest(), r5 / "control-mutex-released.json")
                self.assertFalse(latest().exists())

    def test_success_selects_final_idle_status(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = fixture(Path(tmp).resolve(), True)
            status, _ = group["verify"](*args)
            self.assertTrue(status["path"].endswith(str(Path("lifetime-02/commands/stage-after.stdout"))))

    def test_failed_group_needs_fresh_failure_status(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = fixture(Path(tmp).resolve(), False)
            status, _ = group["verify"](*args)
            self.assertTrue(status["path"].endswith("failure-stage-at-shutdown.stdout"))
            Path(status["path"]).unlink()
            with self.assertRaises(FileNotFoundError):
                group["verify"](*args)

    def test_forced_or_abandoned_owner_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = fixture(Path(tmp).resolve(), False, forced=True)
            with self.assertRaisesRegex(ValueError, "process settlement"):
                group["verify"](*args)
            release = Path(tmp) / "cal100-focus1024-capture-r5/control-mutex-released.json"
            release.unlink()
            with self.assertRaises(FileNotFoundError):
                group["verify"](*args)


if __name__ == "__main__":
    unittest.main()
