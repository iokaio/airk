# SPDX-License-Identifier: Apache-2.0
import copy
import json
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest


m = runpy.run_path(str(Path(__file__).resolve().parents[1]/"arbiter-fp16-execution/compact_cutlass_cal.py"))


def sample():
    env = {"AIRK_CUDA_CAL_LAUNCH_AUDIT":"audit","AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256":"a","AIRK_CUDA_CAL_PHASE":"1",
           "AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT":"projection","AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT_SHA256":"p",
           "AIRK_COMPACT_CUTLASS_TEST_IMAGE_SHA":"binary","AIRK_COMPACT_CUTLASS_EXPIRES_NS":"999999"}
    refusal = {"ordinal":8,"id":"q8","status":"no_native_assessment_request","captures":[]}
    original = {"ordinal":9,"id":"q9","status":"eligible","captures":["binding"],"remaining_ms":1,"remaining_input_tokens":4,
                "compiled":{"input_ids":[1,2,3,4],"input_digest":"item","option_mask":[True,True]},
                "roster":{"cards":[1,2]}}
    score = {k:original[k] for k in ("ordinal","id","captures","remaining_ms","compiled","roster")}
    score.update(status="raw_scores_only",input_digest="item",scores=[.25,.75],compile_us=1,inference_us=10,native_call_elapsed_us=4)
    exclusion = {k:refusal[k] for k in ("ordinal","id","status")}
    native = {"schema":"airk.private.compact-protected-cutlass-model/1","model_sha256":m["GRAPH"],"weight_sha256":m["WEIGHTS"],
              "typed_graph_sha256":m["TYPED"],"placement":{"complete":True},"route_refusal":None,"cpu_refusal":None,
              "graph":None,"score":None,"timing":None,"product_admission":False,"qualification":False,"cal_native_forwards":1,
              "cal":{"phase":1,"original_projection":{"path":"projection","sha256":"p"},"launch_audit":{"path":"audit","sha256":"a"},
                     "rows":[exclusion,score],"qualification":False,"calibration_fitted":False}}
    start = {"schema":"airk.arbiter.native-operation/1","kind":"numeric","clock":"windows_performance_counter",
             "identity":{"process_id":53,"native_binary_sha256":"binary","input_sha256":"p","profile":m["TYPED"]},
             "phase":"started","ordinal":2,"sequence":1,"expires_monotonic_ns":999999,"operation_input_digest":"item",
             "started_monotonic_ns":100,"observed_monotonic_ns":100,"native_success":None,"native_call_elapsed_us":None}
    stop = start | {"phase":"returned","sequence":2,"observed_monotonic_ns":9500,"native_success":True,"native_call_elapsed_us":4}
    keys = [f"Operator=MultiHeadAttention Node=/airk_compact_cutlass/{k}/efficient DataType=fp32 SdpaKernel=EFFICIENT_ATTENTION" for k in range(28)]
    scratch = ["flash=0, efficient=1, has_qkv_workspace=0, workspace=0"]*28
    return [refusal,original],env,native,[start,stop],keys+scratch


def verify(rows,env,native,journal,lines):
    raw = "\n".join([*lines,"AIRK_COMPACT_CUTLASS_CAL="+json.dumps(native),"test result: ok. 1 passed; 0 failed"]).encode()
    j = SimpleNamespace(is_file=lambda:True,read_bytes=lambda:"\n".join(map(json.dumps,journal)).encode())
    return m["observation"](raw,j,rows,env,53,m["TYPED"])


class OriginalCal(unittest.TestCase):
    def test_local_journal_and_global_source_ordinals_survive_refusal(self):
        self.assertEqual(verify(*sample())[0]["cal"]["rows"][1]["ordinal"],9)

    def test_fully_refused_phase_uses_empty_journal_and_no_kernel(self):
        rows,env,native,_,_ = sample()
        native["cal"]["rows"] = native["cal"]["rows"][:1]
        native["cal_native_forwards"] = 0
        self.assertEqual(verify(rows[:1],env,native,[],[])[0]["cal_native_forwards"],0)
        with self.assertRaisesRegex(ValueError,"missing or extra"):
            verify(rows[:1],env,native,[{}],[])

    def test_foreign_pid_and_absolute_instead_of_phase_local_operation_rejected(self):
        for alter in ("pid","ordinal","renewed"):
            row,env,native,journal,lines = sample()
            if alter=="pid":
                journal[0]["identity"]["process_id"]=54
            elif alter=="ordinal":
                journal[0]["ordinal"]=9
            else:
                journal[1]["expires_monotonic_ns"]+=1
            with self.subTest(alter=alter), self.assertRaisesRegex(ValueError,"operation"):
                verify(row,env,native,journal,lines)

    def test_refusal_or_expired_source_never_scored(self):
        for alter in ("refusal","budget","compiler","kernel","missing return"):
            row,env,native,journal,lines = copy.deepcopy(sample())
            if alter=="refusal":
                native["cal"]["rows"][0]["scores"]=[0]
            elif alter=="budget":
                native["cal"]["rows"][1]["compile_us"]=1000
            elif alter=="compiler":
                native["cal"]["rows"][1]["compiled"]["input_ids"]=[9,9,9,9]
                # Break a shared fixture reference to keep the original independent.
                row[1]["compiled"]=row[1]["compiled"] | {"input_ids":[1,2,3,4]}
            elif alter=="kernel":
                lines.pop(0)
            else:
                journal.pop()
            with self.subTest(alter=alter), self.assertRaises(ValueError):
                verify(row,env,native,journal,lines)


if __name__=="__main__":
    unittest.main()
