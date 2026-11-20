# SPDX-License-Identifier: Apache-2.0
"""Pressure-grant controls with no device work."""
from pathlib import Path
import sys
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from protected_capacity_sweep import GIB, GRAPH, PressureTrial, UUID, WINDOW


class ProtectedPressureControls(unittest.TestCase):
    def make(self, free=None):
        self.now = time.monotonic_ns()
        self.available = {"nvidia_free":16*GIB, "wddm_free":16*GIB+WINDOW, "cuda_free_after_touch":16*GIB}
        if free:
            self.available.update(free)
        def authorize(pid, deadline):
            return {"candidate_pid":pid, "snapshot":{"dxgi":{"luid":"luid_0x00000000_0x00013c88"},
                    "nvidia_smi_mib":{"uuid":"GPU-"+UUID,"driver_model.current":"WDDM","driver_version":"597.06"}},
                    "observed_monotonic_ns":self.now,"bytes":self.available}
        return PressureTrial(16, authorize)

    def grant(self, trial):
        return trial.grant(749, self.now/1e9+20, "c"*64, GRAPH, self.now+20_000_000_000)

    def test_one_pid_one_original_observation(self):
        trial = self.make()
        grant = self.grant(trial)
        self.assertEqual(grant["candidate_pid"], 749)
        with self.assertRaises(ValueError):
            self.grant(trial)
        observed = {name:grant[name] for name in ("adapter_luid","adapter_uuid","tier_gib","observed_monotonic_ns","expires_monotonic_ns")}
        observed.update(grant_sha256="b"*64, native_free_bytes=16*GIB, arena_limit_bytes=16*GIB,
                        pre_resume_available_bytes=grant["available_bytes"])
        trial.verify_native(observed, "b"*64)
        for key,value in (("native_free_bytes",16*GIB-1),("arena_limit_bytes",24*GIB),
                          ("adapter_luid","other"),("expires_monotonic_ns",self.now+25_000_000_000),
                          ("grant_sha256","0"*64)):
            with self.subTest(field=key), self.assertRaises(ValueError):
                trial.verify_native(dict(observed, **{key:value}), "b"*64)

    def test_under_floor_and_unconstrained_adapter_refused(self):
        for changes in ({"cuda_free_after_touch":16*GIB-1},
                        {"nvidia_free":17*GIB,"wddm_free":17*GIB}):
            with self.assertRaises(ValueError):
                self.grant(self.make(changes))


class PressureResidencyControls(unittest.TestCase):
    def coverage(self, rows):
        from capacity_sweep import candidate_samples
        series = []
        for i,(dedicated,shared) in enumerate(rows):
            series.append({"process":{"Dedicated Usage":dedicated,"Shared Usage":shared},
                "counter_filetime_100ns":{"process/Dedicated Usage":i+1,"process/Shared Usage":i+1},
                "end_monotonic_ns":i+1})
        limit = {"process_memory_limit_bytes":16*GIB,"job_memory_limit_bytes":16*GIB}
        native = {"owned":{"suspended_assignment":{"observed_limits":limit},
                   "before_GO":{"wddm":{"pid":1,"adapter_luid":"luid","samples":series}}}}
        admission = {"candidate_pid":1,"snapshot":{"dxgi":{"luid":"luid"}}}
        return candidate_samples(native,admission,limit_field="observed_limits",host_limit=16*GIB,require_shared_clock=True)

    def test_shared_peak_and_unknown_survive_outside_dedicated_interval(self):
        live = [(GIB,0),(GIB,0)]
        self.assertTrue(self.coverage([(0,0)]+live+[(0,0)])["verified"])
        for value in (None,64<<20):
            for rows in ([(0,value)]+live,live+[(0,value)]):
                result = self.coverage(rows)
                self.assertFalse(result["verified"])
                self.assertEqual(result["peak_shared_bytes"], value or 0)
                self.assertEqual(result["unknown_shared_samples"],int(value is None))

    def test_live_host_credit_and_hard_reserve(self):
        from capacity_ballast import pressure_headroom
        from ballast_host_commit import reserve
        baseline = {"private_commit_bytes":2*GIB,"working_set_bytes":3*GIB}
        # 16 GiB candidate + 4 GiB outstanding concurrent growth + 8 GiB spare.
        host = {"commit_limit_bytes":64*GIB,"commit_bytes":38*GIB,"physical_available_bytes":27*GIB}
        proof = pressure_headroom(host,20*GIB,baseline,GIB)
        self.assertEqual((proof["pending_commit_bytes"],proof["pending_physical_bytes"]),(18*GIB,19*GIB))
        for changed in (dict(host,commit_bytes=host["commit_bytes"]+1),
                        dict(host,physical_available_bytes=host["physical_available_bytes"]-1)):
            with self.assertRaises(ValueError):
                pressure_headroom(changed,20*GIB,baseline,GIB)
        with self.assertRaises(ValueError):
            reserve(dict(host,physical_available_bytes=8*GIB-1),0)
        with self.assertRaises(ValueError):
            pressure_headroom(host,20*GIB,baseline,3*GIB)


if __name__ == "__main__":
    unittest.main()
