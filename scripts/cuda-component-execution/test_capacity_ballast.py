# SPDX-License-Identifier: Apache-2.0
"""Model-free controls of the WDDM capacity and residency decision."""

from pathlib import Path
import sys
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import capacity_ballast as owner
import capacity_ballast_worker as worker
from capacity_sweep import candidate_samples, require_proof


def sample(dedicated, shared=0, committed=None):
    return {"process":{"Dedicated Usage":dedicated,"Shared Usage":shared,
                       "Total Committed":dedicated if committed is None else committed},
            "adapter":{"Dedicated Usage":40*worker.GIB},"end_monotonic_ns":time.monotonic_ns()}


def adapter(free=8192):
    return {"dxgi":{"dedicated_video_bytes":48*worker.GIB, "luid":"adapter"},
            "nvidia_smi_mib":{"uuid":"GPU-"+worker.UUID,"driver_model.current":"WDDM",
                              "driver_version":"597.06","memory.free":free}}


class BallastControls(unittest.TestCase):
    def test_independent_system_commit_and_physical_headroom(self):
        from ballast_host_commit import reserve
        observed = {"commit_bytes":50*worker.GIB,"commit_limit_bytes":62*worker.GIB,"physical_available_bytes":12*worker.GIB}
        reserve(observed,4*worker.GIB)
        for field in ("commit_limit_bytes","physical_available_bytes"):
            with self.assertRaisesRegex(ValueError,"reserve"):
                reserve(dict(observed,**{field:observed[field]-1}),4*worker.GIB)

    def test_inherited_sizing_channel_preserves_separate_grants(self):
        import json
        import subprocess
        import tempfile
        # The first child reads the same inherited regular-file handle twice. A
        # loopback socket only signals publication; both grants arrive on stdin.
        import socket
        with tempfile.TemporaryFile(mode="w+b",buffering=0) as channel, socket.socket() as listener:
            listener.bind(("127.0.0.1",0))
            listener.listen(1)
            listener.settimeout(5)
            program = """import socket,sys,runpy,time
worker=runpy.run_path(sys.argv[1])
with socket.create_connection(('127.0.0.1',int(sys.argv[2])),timeout=5) as s:
    for ordinal in (1,2):
        assert s.recv(1)==b'G'
        grant=worker['read_pressure_grant'](sys.stdin.buffer.raw,'test',ordinal,ordinal*64,time.monotonic_ns())
        s.sendall(bytes([grant['nvidia']]))
"""
            child = subprocess.Popen([sys.executable,"-I","-B","-S","-c",program,worker.__file__,
                                      str(listener.getsockname()[1])],stdin=channel)
            try:
                with listener.accept()[0] as sync:
                    sync.settimeout(5)
                    for ordinal in (1,2):
                        channel.seek(0)
                        channel.truncate()
                        channel.write(json.dumps(dict(nonce="test",sequence=ordinal,touched=ordinal*64,
                                                      sample_start_ns=time.monotonic_ns(),nvidia=ordinal)).encode())
                        sync.sendall(b"G")
                        self.assertEqual(sync.recv(1),bytes([ordinal]))
                self.assertEqual(child.wait(timeout=5),0)
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=5)

    def test_pressure_sizing_uses_tighter_observed_counter_and_rechecks(self):
        g, m = worker.GIB, worker.MIB
        maximum = 32*g
        # Real 16 GiB refusal: CUDA alone was above the floor while both independent counters were below.
        with self.assertRaisesRegex(ValueError, "floor"):
            worker.pressure_step((17245929472,16061*m,16832385024),16,32639025152,maximum)
        self.assertEqual(worker.pressure_step((40*g,39*g,38*g),16,0,maximum),2*g)
        for independent in ((16*g+300*m,17*g),(17*g,16*g+300*m)):
            self.assertEqual(worker.pressure_step((17*g,*independent),16,29*g,maximum),236*m)
        self.assertEqual(worker.pressure_step((17*g,16*g+64*m,17*g),16,30*g,maximum),0)
        for observations,total in (((16*g,17*g,17*g),31*g),((17*g,17*g,17*g),maximum)):
            with self.assertRaisesRegex(ValueError,"no bounded increment"):
                worker.pressure_step(observations,16,total,maximum)

    def test_all_tiers_bounded(self):
        for floor in (6,8,12,16,24):
            size = worker.initial_allocation(47*worker.GIB,floor,(48-floor)*worker.GIB)
            self.assertEqual(size, (47-floor)*worker.GIB-64*worker.MIB)
        for args in [(47*worker.GIB,4,40*worker.GIB),(5*worker.GIB,6,2*worker.GIB),
                     (47*worker.GIB,8,32*worker.GIB),(48*worker.GIB,8,41*worker.GIB)]:
            with self.assertRaises(ValueError):
                worker.initial_allocation(*args)

    def test_residency_requires_all_local_and_no_shared_growth(self):
        baseline = {"Dedicated Usage":128*worker.MIB,"Shared Usage":0}
        amount = 64*worker.MIB
        self.assertTrue(owner.resident(sample(192*worker.MIB),baseline,amount))
        for event in (sample(191*worker.MIB), sample(192*worker.MIB,1), sample(192*worker.MIB,0,1),
                      sample(None,0,0)):
            self.assertFalse(owner.resident(event,baseline,amount))

    def test_tier_requires_current_headroom_and_original_device(self):
        row = sample(1)
        held = {"free":8*worker.GIB}
        self.assertEqual(owner.headroom(adapter(),row,held,8)["wddm_free"],8*worker.GIB)
        with self.assertRaises(ValueError):
            owner.headroom(adapter(8100),row,held,8)
        row["adapter"]["Dedicated Usage"] -= worker.GIB
        with self.assertRaises(ValueError):
            owner.headroom(adapter(9216),row,held,8)
        foreign=adapter()
        foreign["nvidia_smi_mib"]["uuid"]="other"
        with self.assertRaisesRegex(ValueError,"foreign"):
            owner.headroom(foreign,row,held,8)

    def test_native_pre_resume_single_gate_and_stale_sample(self):
        now=time.monotonic()
        gate=owner.Gate(8,False,now+300,"adapter")
        gate.baseline={"Dedicated Usage":0,"Shared Usage":0}
        gate.held={"touched":12,"free":8*worker.GIB}
        gate.latest=sample(12)
        with patch.object(owner,"snapshot",return_value=adapter()):
            gate.latest["end_monotonic_ns"]-=worker.GIB
            with self.assertRaisesRegex(ValueError,"stale"):
                gate.authorize(17,now+120)
            gate.latest=sample(12)
            self.assertEqual(gate.authorize(17,now+120)["candidate_pid"],17)
            with self.assertRaisesRegex(ValueError,"one native"):
                gate.authorize(18,now+120)

    def test_candidate_spill_gaps_and_unobserved_counters_refuse_admission(self):
        rows=[sample(100),sample(110),sample(120)]
        for index,row in enumerate(rows):
            row["counter_filetime_100ns"]={"process/Dedicated Usage":index+1}
            row["end_monotonic_ns"]=1_000_000_000+index*100_000_000
        proof={"owned":{"suspended_assignment":{"job_limits":{"process_memory_limit_bytes":32*worker.GIB,
               "job_memory_limit_bytes":32*worker.GIB}},"before_GO":{"wddm":{"pid":17,"adapter_luid":"adapter",
               "samples":rows}}}}
        admission={"candidate_pid":17,"snapshot":adapter()}
        self.assertTrue(candidate_samples(proof,admission)["verified"])
        rows[1]["process"]["Shared Usage"]=65536
        self.assertFalse(candidate_samples(proof,admission)["verified"])
        rows[1]["process"]["Shared Usage"]=None
        self.assertFalse(candidate_samples(proof,admission)["verified"])
        rows[1]["process"]["Shared Usage"]=0
        rows[2]["end_monotonic_ns"]+=1000000000
        self.assertFalse(candidate_samples(proof,admission)["verified"])

    def test_cuda_allocation_refusal_survives_natural_worker_closure(self):
        failure={"kind":"worker_failure","cuda_status":2,"cuda_operation":"cuMemAlloc_v2","message":"out of memory"}
        messages=[{"kind":"context"},{"kind":"chunk","chunks":1,"touched_bytes":256*worker.MIB},
                  {"kind":"released","freed_chunks":1,"allocated_chunks":1,"touched_bytes":256*worker.MIB},failure]
        evidence=owner.terminal(messages,False,False,314)
        self.assertEqual(evidence["worker_error"],failure)
        self.assertEqual(evidence["messages"][-2]["freed_chunks"],1)
        with self.assertRaisesRegex(ValueError,"ambiguous"):
            owner.terminal(messages+[failure],False,False,314)
        with self.assertRaisesRegex(ValueError,"missing natural"):
            owner.terminal(messages[:-1],False,False,314)

    def test_forced_job_has_no_capacity_verdict(self):
        job={"exit_code":0,"timed_out":False,"log_oversized":False,"owned_descendants_settled":True,
             "owned_job_final_active_processes":0,"owned_job_final_member_pids":[],"owned_total_processes":1,
             "owned_job_forced_termination":False,"child_assigned_before_resume":True}
        self.assertTrue(owner.natural(job))
        for key,value in (("exit_code",1),("timed_out",True),("owned_job_forced_termination",True),
                          ("owned_total_processes",2),("owned_job_final_active_processes",1)):
            self.assertFalse(owner.natural(dict(job,**{key:value})))


if __name__=="__main__":
    unittest.main()
