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


class BaselineRelativeRuleControls(unittest.TestCase):
    """Approved change A6. Every expected figure is written by hand from the frozen decisions:
    control 77,594,624 B (the model-free ballast context), margin 16 MiB, reserve 2 GiB."""
    PEAK = 6_706_245_632
    LIVE = (6_706_245_632, 79_691_776, 79_691_776)
    ADMITTED = {"name":"baseline_relative_a6/1", "control":"ballast_process_shared_usage_before_resume",
                "control_shared_bytes":77_594_624, "margin_bytes":16_777_216, "reserve_bytes":2_147_483_648,
                "tier_bytes":12_884_901_888, "peak_shared_bytes":79_691_776, "peak_fresh_nonlocal_bytes":79_691_776,
                "peak_dedicated_bytes":6_706_245_632, "shared_excess_bytes":2_097_152,
                "nonlocal_excess_bytes":2_097_152, "dedicated_plus_reserve_bytes":8_853_729_280,
                "samples_verified":True, "satisfied":True, "refusal":None}
    RECORDED = {"peak_shared_bytes":79_691_776, "unknown_shared_samples":18, "all_process_samples":119,
                "verified":True, "peak_dedicated_bytes":6_706_245_632, "observations":101, "continuous_samples":True,
                "shared_ceiling_bytes":94_371_840, "unknown_live_samples":0, "segment_observations":119,
                "peak_local_bytes":6_706_245_632, "peak_fresh_local_bytes":6_706_245_632, "unknown_local_samples":18,
                "stale_local_samples":0, "peak_nonlocal_bytes":79_691_776, "peak_fresh_nonlocal_bytes":79_691_776,
                "unknown_nonlocal_samples":18, "stale_nonlocal_samples":0, "nonlocal_conflict":True,
                "dedicated_segment_evidence":False, "dedicated_only_verified":False}
    BINDING = {"tier":12, "model":"compact", "image":{"sha256":"a"*64}, "build":{"sha256":"b"*64},
               "probe_source_sha256":"c"*64}

    def pair(self, rows, control=77_594_624, gap_after=None, nonlocal_lag=0):
        series, clock = [], 0
        for index, (dedicated, shared, nonlocal_) in enumerate(rows):
            clock += 1_000_000_000 if gap_after is not None and index > gap_after else 1
            tick = index + 1
            series.append({"process":{"Dedicated Usage":dedicated, "Shared Usage":shared},
                           "segment_bytes":{"Local Usage":dedicated, "Non Local Usage":nonlocal_},
                           "counter_filetime_100ns":{"process/Dedicated Usage":tick, "process/Shared Usage":tick,
                                                     "segment_bytes/Local Usage":tick,
                                                     "segment_bytes/Non Local Usage":tick + nonlocal_lag},
                           "end_monotonic_ns":clock})
        limit = {"process_memory_limit_bytes":12*GIB, "job_memory_limit_bytes":12*GIB}
        native = {"owned":{"suspended_assignment":{"observed_limits":limit},
                           "before_GO":{"wddm":{"pid":1, "adapter_luid":"luid", "samples":series}}}}
        admission = {"candidate_pid":1, "snapshot":{"dxgi":{"luid":"luid"}},
                     "sample":{"process":{"Shared Usage":control}}}
        return native, admission

    def judged(self, rows, tier=12, **options):
        from protected_capacity_sweep import judge
        return judge(*self.pair(rows, **options), tier, 12*GIB)

    def test_admits_at_control_plus_two_mib_and_keeps_absolute_peaks(self):
        memory, rule = self.judged([self.LIVE]*3)
        self.assertEqual(rule, self.ADMITTED)
        self.assertEqual((memory["verified"], memory["shared_ceiling_bytes"], memory["unknown_live_samples"]),
                         (True, 94_371_840, 0))
        # The superseded dedicated-only reading still refuses this run and stays visible.
        self.assertEqual((memory["dedicated_only_verified"], memory["nonlocal_conflict"]), (False, True))
        self.assertEqual((memory["peak_shared_bytes"], memory["peak_nonlocal_bytes"]), (79_691_776, 79_691_776))

    def test_refuses_shared_usage_above_the_margin(self):
        _, rule = self.judged([(self.PEAK, 94_371_840, 79_691_776)]*3)
        self.assertEqual((rule["satisfied"], rule["shared_excess_bytes"]), (True, 16_777_216))
        memory, rule = self.judged([self.LIVE, (self.PEAK, 94_371_841, 79_691_776), self.LIVE])
        self.assertEqual((rule["satisfied"], rule["refusal"], rule["shared_excess_bytes"], rule["peak_shared_bytes"]),
                         (False, "shared_usage_exceeds_control_margin", 16_777_217, 94_371_841))
        self.assertEqual((memory["verified"], rule["samples_verified"]), (False, False))
        # The retained 6 GiB figures: 130 MiB over the control. Shared use is named before the reserve.
        _, rule = self.judged([(6_572_290_048, 213_909_504, 213_909_504)]*3, tier=6)
        self.assertEqual((rule["satisfied"], rule["refusal"], rule["shared_excess_bytes"]),
                         (False, "shared_usage_exceeds_control_margin", 136_314_880))
        # A spill before or after the dedicated interval counts as well.
        for rows in ([(0, 94_371_841, 0)]+[self.LIVE]*2, [self.LIVE]*2+[(0, 94_371_841, 0)]):
            self.assertEqual(self.judged(rows)[1]["refusal"], "shared_usage_exceeds_control_margin")

    def test_refuses_fresh_nonlocal_usage_above_the_margin(self):
        _, rule = self.judged([(self.PEAK, 79_691_776, 94_371_840)]*3)
        self.assertEqual((rule["satisfied"], rule["nonlocal_excess_bytes"]), (True, 16_777_216))
        memory, rule = self.judged([self.LIVE, (self.PEAK, 79_691_776, 94_371_841), self.LIVE])
        self.assertEqual((rule["satisfied"], rule["refusal"], rule["nonlocal_excess_bytes"]),
                         (False, "nonlocal_usage_exceeds_control_margin", 16_777_217))
        # The primary shared counter passed by itself; only the segment counter refused.
        self.assertEqual((memory["verified"], rule["shared_excess_bytes"]), (True, 2_097_152))

    def test_missing_or_stale_nonlocal_counter_refuses(self):
        # Never read: every other criterion passes, and the tier is still refused.
        memory, rule = self.judged([(self.PEAK, 79_691_776, None)]*3)
        self.assertEqual((rule["satisfied"], rule["refusal"], rule["peak_fresh_nonlocal_bytes"],
                          rule["nonlocal_excess_bytes"]), (False, "incomplete_residency_evidence", None, None))
        self.assertEqual((memory["verified"], rule["shared_excess_bytes"], rule["dedicated_plus_reserve_bytes"],
                          memory["unknown_nonlocal_samples"]), (True, 2_097_152, 8_853_729_280, 3))
        # Read, but its clock is one second (10,000,000 ticks) from the dedicated counter: no fresh reading.
        memory, rule = self.judged([self.LIVE]*3, nonlocal_lag=10_000_000)
        self.assertEqual((rule["satisfied"], rule["refusal"], rule["peak_fresh_nonlocal_bytes"]),
                         (False, "incomplete_residency_evidence", None))
        self.assertEqual((memory["stale_nonlocal_samples"], memory["peak_nonlocal_bytes"], memory["verified"]),
                         (3, 79_691_776, True))
        # A present, fresh counter within the margin still admits.
        memory, rule = self.judged([self.LIVE]*3)
        self.assertEqual((rule["satisfied"], rule["refusal"], rule["nonlocal_excess_bytes"],
                          memory["stale_nonlocal_samples"]), (True, None, 2_097_152, 0))
        # The other refusals keep their order: a spill is still named before the missing counter.
        _, rule = self.judged([(self.PEAK, 94_371_841, None)]*3)
        self.assertEqual(rule["refusal"], "shared_usage_exceeds_control_margin")
        _, rule = self.judged([(6_714_634_240, 79_691_776, None)]*3, tier=8)
        self.assertEqual(rule["refusal"], "dedicated_usage_exceeds_tier_reserve")

    def test_refuses_on_the_two_gib_reserve(self):
        # 6 GiB plus the 2 GiB reserve is exactly the 8 GiB tier. One more byte is refused.
        _, rule = self.judged([(6_442_450_944, 79_691_776, 79_691_776)]*3, tier=8)
        self.assertEqual((rule["satisfied"], rule["dedicated_plus_reserve_bytes"], rule["tier_bytes"]),
                         (True, 8_589_934_592, 8_589_934_592))
        _, rule = self.judged([(6_442_450_945, 79_691_776, 79_691_776)]*3, tier=8)
        self.assertEqual((rule["satisfied"], rule["refusal"], rule["dedicated_plus_reserve_bytes"]),
                         (False, "dedicated_usage_exceeds_tier_reserve", 8_589_934_593))
        # The retained 8 GiB Compact peak fails the reserve at 8 GiB and passes at 12 GiB.
        rows = [(6_714_634_240, 79_691_776, 79_691_776)]*3
        self.assertEqual(self.judged(rows, tier=8)[1]["refusal"], "dedicated_usage_exceeds_tier_reserve")
        self.assertEqual(self.judged(rows, tier=8)[1]["dedicated_plus_reserve_bytes"], 8_862_117_888)
        self.assertTrue(self.judged(rows, tier=12)[1]["satisfied"])

    def test_unknown_sample_inside_live_interval_refuses_and_outside_is_reported(self):
        for hole in ((self.PEAK, None, 79_691_776), (None, 79_691_776, 79_691_776)):
            with self.subTest(hole=hole):
                memory, rule = self.judged([self.LIVE, hole, self.LIVE])
                self.assertEqual((rule["satisfied"], rule["refusal"], rule["samples_verified"]),
                                 (False, "incomplete_residency_evidence", False))
                self.assertEqual((memory["verified"], memory["unknown_live_samples"]), (False, 1))
        memory, rule = self.judged([(None, None, None), self.LIVE, self.LIVE, (None, None, None)])
        self.assertTrue(rule["satisfied"])
        self.assertEqual((memory["unknown_shared_samples"], memory["unknown_live_samples"]), (2, 0))
        # A sampling gap longer than 500 ms inside the live interval is not continuous evidence.
        memory, rule = self.judged([self.LIVE]*3, gap_after=0)
        self.assertEqual((rule["refusal"], memory["continuous_samples"]), ("incomplete_residency_evidence", False))
        # No positive dedicated observation leaves nothing to judge.
        self.assertEqual(self.judged([(0, 0, 0)]*3)[1]["refusal"], "incomplete_residency_evidence")

    def test_control_must_be_a_known_same_run_figure(self):
        for control in (None, -1, 77_594_624.0, True):
            with self.subTest(control=control), self.assertRaisesRegex(ValueError, "control"):
                self.judged([self.LIVE]*3, control=control)

    def test_default_dedicated_only_reading_is_unchanged(self):
        from capacity_sweep import candidate_samples
        options = dict(limit_field="observed_limits", host_limit=12*GIB, require_shared_clock=True)
        self.assertEqual(candidate_samples(*self.pair([self.LIVE]*3), **options),
                         {"peak_shared_bytes":79_691_776, "unknown_shared_samples":0, "all_process_samples":3,
                          "verified":False, "peak_dedicated_bytes":6_706_245_632, "observations":3,
                          "continuous_samples":True})
        self.assertEqual(candidate_samples(*self.pair([(self.PEAK, 0, 0)]*3), **options),
                         {"peak_shared_bytes":0, "unknown_shared_samples":0, "all_process_samples":3,
                          "verified":True, "peak_dedicated_bytes":6_706_245_632, "observations":3,
                          "continuous_samples":True})
        # By default an unreadable sample outside the live interval still refuses.
        outside = candidate_samples(*self.pair([(None, None, None)]+[(self.PEAK, 0, 0)]*2), **options)
        self.assertEqual((outside["verified"], outside["unknown_shared_samples"]), (False, 1))
        self.assertEqual(candidate_samples(*self.pair([self.LIVE]*3), shared_ceiling=94_371_840, **options),
                         {"peak_shared_bytes":79_691_776, "unknown_shared_samples":0, "all_process_samples":3,
                          "verified":True, "peak_dedicated_bytes":6_706_245_632, "observations":3,
                          "continuous_samples":True, "shared_ceiling_bytes":94_371_840, "unknown_live_samples":0})
        for ceiling in (-1, True, 94_371_840.0):
            with self.subTest(ceiling=ceiling), self.assertRaisesRegex(ValueError, "ceiling"):
                candidate_samples(*self.pair([self.LIVE]*3), shared_ceiling=ceiling, **options)

    def prior(self):
        return {"complete":True, "tier_gib":12,
                "admission":{"pre_resume_host":{"minimum_spare_bytes":8_589_934_592},
                             "sample":{"process":{"Shared Usage":77_594_624}}},
                "trial":{"mode":"pressure-fit", "model":"compact", "capacity_admitted":True,
                         "observed_outcome":"admitted", "candidate_wddm":dict(self.RECORDED),
                         "capacity_rule":dict(self.ADMITTED), "image":{"sha256":"a"*64},
                         "build":{"sha256":"b"*64}, "probe_source_sha256":"c"*64}}

    def test_fixed20_accepts_a_fit_admitted_under_this_rule(self):
        import json
        from protected_capacity_sweep import require_prior_fit
        require_prior_fit(self.prior(), **self.BINDING)
        require_prior_fit(json.loads(json.dumps(self.prior())), **self.BINDING)

    def test_fixed20_refuses_an_old_rule_or_mismatched_fit(self):
        from protected_capacity_sweep import require_prior_fit
        # A fit admitted under the superseded dedicated-only rule carries no rule record.
        old = self.prior()
        del old["trial"]["capacity_rule"]
        old["trial"]["candidate_wddm"] = {"peak_shared_bytes":0, "unknown_shared_samples":0, "all_process_samples":119,
                                          "verified":True, "peak_dedicated_bytes":6_706_245_632, "observations":101,
                                          "continuous_samples":True, "nonlocal_conflict":False}
        with self.assertRaisesRegex(ValueError, "different capacity fit"):
            require_prior_fit(old, **self.BINDING)
        for name, value in (("tier", 16), ("model", "large"), ("image", {"sha256":"d"*64}),
                            ("build", {"sha256":"d"*64}), ("probe_source_sha256", "d"*64)):
            with self.subTest(binding=name), self.assertRaisesRegex(ValueError, "different capacity fit"):
                require_prior_fit(self.prior(), **dict(self.BINDING, **{name:value}))
        def changed(section, **values):
            receipt = self.prior()
            target = receipt if section is None else receipt["trial"] if section == "trial" else receipt["trial"][section]
            target.update(values)
            return receipt
        mismatched = [changed(None, complete=False), changed(None, tier_gib=16),
                      changed("trial", mode="pressure-fixed"), changed("trial", capacity_admitted=False),
                      changed("trial", observed_outcome="shared_usage_exceeds_control_margin"),
                      changed("capacity_rule", name="dedicated_only/1"),
                      changed("capacity_rule", margin_bytes=33_554_432),
                      changed("capacity_rule", reserve_bytes=0),
                      changed("capacity_rule", control_shared_bytes=79_691_776, shared_excess_bytes=0),
                      changed("capacity_rule", satisfied=False, refusal="incomplete_residency_evidence"),
                      changed("candidate_wddm", verified=False),
                      changed("candidate_wddm", unknown_live_samples=1),
                      changed("candidate_wddm", continuous_samples=False),
                      changed("candidate_wddm", peak_shared_bytes=94_371_841),
                      changed("candidate_wddm", peak_fresh_nonlocal_bytes=94_371_841),
                      changed("candidate_wddm", peak_fresh_nonlocal_bytes=None),
                      changed("candidate_wddm", peak_dedicated_bytes=10_737_418_241),
                      changed("candidate_wddm", shared_ceiling_bytes=111_149_056)]
        other_control = self.prior()
        other_control["admission"]["sample"]["process"]["Shared Usage"] = 0
        thin_reserve = self.prior()
        thin_reserve["admission"]["pre_resume_host"]["minimum_spare_bytes"] = 4_294_967_296
        for index, receipt in enumerate(mismatched+[other_control, thin_reserve]):
            with self.subTest(index=index), self.assertRaisesRegex(ValueError, "different capacity fit"):
                require_prior_fit(receipt, **self.BINDING)


if __name__ == "__main__":
    unittest.main()
