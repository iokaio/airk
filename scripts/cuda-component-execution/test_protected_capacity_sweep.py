# SPDX-License-Identifier: Apache-2.0
"""Pressure-grant controls with no device work."""
from pathlib import Path
import sys
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from protected_capacity_sweep import GIB, GRAPH, LARGE_GRAPH, PressureTrial, UUID, WINDOW


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

    def test_large_grant_rejects_compact_graph_and_reopens_own_gate(self):
        compact = self.make()
        trial = PressureTrial(16, compact.authorize, LARGE_GRAPH)
        with self.assertRaises(ValueError):
            self.grant(trial)
        original = "13da8267e7490e334b5a860ced90af8318a01f5c2d03759173feac45ca598b46"
        grant = trial.grant(749, self.now/1e9+20, "c"*64, original, self.now+20_000_000_000)
        self.assertEqual(grant["typed_graph_sha256"], original)
        with self.assertRaises(ValueError):
            compact.grant(749, self.now/1e9+20, "c"*64, original, self.now+20_000_000_000)

    def test_under_floor_and_unconstrained_adapter_refused(self):
        for changes in ({"cuda_free_after_touch":16*GIB-1},
                        {"nvidia_free":17*GIB,"wddm_free":17*GIB}):
            with self.assertRaises(ValueError):
                self.grant(self.make(changes))


class PressureResidencyControls(unittest.TestCase):
    def test_tight_single_host_grant_binds_actual_caps_and_rejects_overrun(self):
        from capacity_ballast import pressure_headroom
        from protected_capacity_sweep import verify_host_grant
        from runpy import run_path
        cap = run_path(str(Path(__file__).resolve().parents[1]/"arbiter-fp16-execution/probe_compact_cutlass.py"))["host_cap"]
        limit = 12*GIB
        self.assertEqual(cap("pressure-fit","8192"),limit)
        probe = run_path(str(Path(__file__).resolve().parents[1]/"arbiter-fp16-execution/probe_compact_cutlass.py"))
        original = dict(schema="airk.private.compact-protected-cutlass-model/1",
                        model_sha256="147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9",
                        weight_sha256="0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3",
                        placement={"complete":True},graph=None,score=None,timing={"profiled":False})
        import json
        from capacity_ballast_worker import require
        stdout = ("AIRK_COMPACT_CUTLASS_MODEL="+json.dumps(original)+"\n1 passed; 0 failed\n").encode()
        self.assertEqual(probe["model_output"](stdout,"pressure-fit",{"require":require})[0],original)
        with self.assertRaisesRegex(ValueError,"uninstrumented"):
            probe["model_output"](b"Operator=unexpected\n"+stdout,"pressure-fit",{"require":require})
        self.assertEqual([cap(mode,"8192") for mode in ("pressure-fixed","fixed","mixed")],[16*GIB]*3)
        limits = dict(process_memory_limit_bytes=limit,job_memory_limit_bytes=limit)
        native = {"owned":{"suspended_assignment":{"observed_limits":limits},"owned_leader_pid":41,
                          "peak_host_committed_bytes":limit},"mode":"pressure-fit","image":{"sha256":"b"*64},
                  "pressure_grant":{"sha256":"a"*64},"original_limits":{"host_bytes":limit,"campaign_host_ceiling_bytes":32*GIB}}
        grant = {"schema":"airk.private.cuda-pressure-host-admission/1","candidate_pid":41,"mode":"pressure-fit",
                 "limits":limits,"device_grant_sha256":"a"*64,"executable_sha256":"b"*64,"campaign_host_ceiling_bytes":32*GIB}
        verify_host_grant(grant,native,limit)
        with self.assertRaisesRegex(ValueError,"host grant"):
            verify_host_grant(grant,native,limit,48*GIB)
        for changed in (dict(grant,candidate_pid=42),dict(grant,device_grant_sha256="c"*64),
                        dict(grant,limits=dict(limits,job_memory_limit_bytes=16*GIB))):
            with self.assertRaisesRegex(ValueError,"host grant"):
                verify_host_grant(changed,native,limit)
        native["owned"]["peak_host_committed_bytes"] += 1
        with self.assertRaisesRegex(ValueError,"host grant"):
            verify_host_grant(grant,native,limit)
        host = dict(commit_bytes=0,commit_limit_bytes=20*GIB,physical_available_bytes=20*GIB)
        pre = dict(private_commit_bytes=0,working_set_bytes=0)
        self.assertEqual(pressure_headroom(host,limit,pre,0,limit)["minimum_spare_bytes"],8*GIB)
        with self.assertRaisesRegex(ValueError,"reserve"):
            pressure_headroom(dict(host,physical_available_bytes=20*GIB-1),limit,pre,0,limit)

    def test_large_process_and_aggregate_limits_and_observed_peak(self):
        from protected_capacity_sweep import verify_host_grant
        limit = 22*GIB
        limits = {"process_memory_limit_bytes":limit, "job_memory_limit_bytes":limit}
        native = {"owned":{"owned_leader_pid":93, "suspended_assignment":{"observed_limits":limits},
                           "peak_host_committed_bytes":17_730_695_168}, "mode":"pressure-fit", "image":{"sha256":"c"*64},
                  "pressure_grant":{"sha256":"a"*64}, "original_limits":{"host_bytes":limit,"campaign_host_ceiling_bytes":48*GIB}}
        grant = {"schema":"airk.private.cuda-pressure-host-admission/1","mode":"pressure-fit","candidate_pid":93,
                 "limits":limits,"device_grant_sha256":"a"*64,"executable_sha256":"c"*64,"campaign_host_ceiling_bytes":48*GIB}
        verify_host_grant(grant,native,limit,48*GIB)
        for key, wrong in (("process_memory_limit_bytes",48*GIB), ("job_memory_limit_bytes",32*GIB)):
            with self.subTest(key=key), self.assertRaisesRegex(ValueError,"host grant"):
                verify_host_grant(dict(grant,limits=dict(limits,**{key:wrong})),native,limit,48*GIB)
        native["owned"]["peak_host_committed_bytes"] = limit + 1
        with self.assertRaisesRegex(ValueError,"host grant"):
            verify_host_grant(grant,native,limit,48*GIB)

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

    def test_shared_zero_cannot_mask_nonlocal_or_missing_segment_measurements(self):
        from capacity_sweep import segment_evidence
        def observed(amount, tick):
            return {"process":{"Shared Usage":0}, "segment_bytes":{"Local Usage":GIB,"Non Local Usage":amount},
                    "counter_filetime_100ns":{"process/Dedicated Usage":tick,"segment_bytes/Local Usage":tick,
                                             "segment_bytes/Non Local Usage":tick}}
        clean = {"samples":[observed(0,1),observed(0,2)]}
        self.assertTrue(segment_evidence(clean)["dedicated_segment_evidence"])
        self.assertFalse(segment_evidence(clean)["nonlocal_conflict"])
        self.assertTrue(segment_evidence({"samples":[observed(0,1),observed(GIB,2)]})["nonlocal_conflict"])
        stale = segment_evidence({"samples":[observed(0,1),observed(GIB,1)]})
        self.assertFalse(stale["nonlocal_conflict"])
        self.assertEqual(stale["stale_nonlocal_samples"],1)
        for first,last in ((observed(GIB,1),observed(0,2)),(observed(0,1),observed(GIB,2)),
                           (observed(None,1),observed(0,2)),(observed(0,1),observed(0,1))):
            with self.subTest(first=first,last=last):
                self.assertFalse(segment_evidence({"before_resume":first,"samples":[last]})["dedicated_segment_evidence"])
        missing = segment_evidence({"samples":[{"counter_filetime_100ns":{}}]})
        self.assertEqual(missing["unknown_nonlocal_samples"],1)
        self.assertFalse(missing["nonlocal_conflict"])
        self.assertFalse(missing["dedicated_segment_evidence"])

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
