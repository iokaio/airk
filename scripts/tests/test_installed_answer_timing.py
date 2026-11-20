# SPDX-License-Identifier: Apache-2.0
"""Product identity, timeout accounting, and the complete weighted timing comparison."""
import copy
import json
from pathlib import Path
import runpy
import unittest

HERE = Path(__file__).resolve().parents[1] / "arbiter/analysis/development_margins"
compare = runpy.run_path(str(HERE / "compare_installed_answer_timings.py"))
m = compare["obs"]
ORDER = ["case" + str(i) for i in range(13)] + ["case" + str(i) for i in range(7)]


def rows(ns):
    return [{"ordinal":i,"case":case,"request_sha256":case,"elapsed_ns":ns,"state":"observed",
             "native":{"selected_context":0,"context":[]}} for i,case in enumerate(ORDER)]


class Timing(unittest.TestCase):
    def test_only_authorized_campaign_roots_or_original_personal_record(self):
        owner = compare["owner"]
        campaign = owner["E"]
        personal = owner["INSTALL"]
        resolve = owner["managed"]["record_path"]
        self.assertEqual(resolve({},campaign,personal), personal)
        for wrong in (personal, campaign/"other"/"install-record.json",campaign/"windows-managed-cuda-r1"/"install-record.json"):
            with self.subTest(wrong=wrong), self.assertRaises(ValueError):
                resolve({"managed_install":{"variant":"cpu","record":str(wrong)}},campaign,personal)

    def test_native_managed_inspection_requires_original_payload_and_current_image(self):
        from unittest.mock import patch
        verify = compare["owner"]["managed"]["validate_inspection"]
        root = HERE
        source,install,control = (root/n for n in ("program","managed","control"))
        approval = {"manifest_sha256":"manifest","version":"1.8.207"}
        policy = {"source":str(source),"install":str(install),"approval":approval,"executable_sha256":"image"}
        native = {"payload_verified":True,"state_error":None,"selection":{"active":"manifest"},"approval":approval,
                  "managed_policy":policy,"executable":str(source/"airk.exe"),"control_dir":str(control)}
        with patch.dict(verify.__globals__,path_in=lambda p,_:Path(p),sha=lambda p:"image" if p.name=="airk.exe" else "manifest"):
            check = lambda n: verify(n,root,install,control,"image",{"version":"1.8.207"},"manifest")
            self.assertEqual(check(native),source)
            for changed in ({"managed_policy":None},{"state_error":"foreign"},{"selection":{"active":"another"}},
                            {"executable":str(control/"airk.exe")},{"control_dir":str(root)},{"payload_verified":False}):
                with self.subTest(changed=changed), self.assertRaises(ValueError):
                    check(native | changed)

    def test_inspection_retains_exact_selection_and_control(self):
        import hashlib
        helper = compare["owner"]["managed"]
        native = {"payload_verified":True,"selection":{"active":"digest"},"approval":{},"executable":"main",
                  "managed_policy":{"source":"current"},"control_dir":"owned","state_error":None}
        def obs(obj):
            text = json.dumps(obj)
            return {"argv":["real","runtime","inspect"],"original_record":"record","record_sha256":"record-hash",
                    "source":"current","variant":"cpu","installer_sha256":"package","stdout":text,
                    "stdout_sha256":hashlib.sha256(text.encode()).hexdigest()}
        original = obs(native)
        helper["same_inspection"](original,obs(native | {"lifecycle":{"status":"reported"}}))
        for field,value in (("managed_policy",None),("executable","fixture"),("control_dir","foreign"),
                            ("payload_verified",False),("selection",{"active":"other"})):
            with self.subTest(field=field), self.assertRaises(ValueError):
                helper["same_inspection"](original,obs(native | {field:value}))
        with self.assertRaises(ValueError):
            helper["same_inspection"](original | {"stdout":"{}"},original)

    def test_native_disposition_and_elapsed_deadline(self):
        state = m["native_state"]
        req = {"budget":{"deadline_ms":120000}}
        for stop,term,time,expected in (
                ("budget","budget",120_000_000_000,"observed"),("covered","deadline",1,"timeout"),
                ("deadline","refused",1,"timeout"),("no_progress","pending",1,"native_pending"),
                ("refused","no_progress",1,"native_refused"),("stale","stale",1,"native_stale"),
                ("no_valid_read","unsupported",1,"native_unsupported"),("covered","covered",120_000_000_001,"timeout")):
            self.assertEqual(state({"stop":stop,"termination":term},time,req),expected)
        with self.assertRaisesRegex(ValueError,"unknown"):
            state({"stop":"pending","termination":"pending"},1,req)

    def test_successful_cli_with_unparseable_stdout_preserves_invalid_native(self):
        classify = compare["owner"]["classify_stdout"]
        result = classify(b"{",{},None,{},None,{},1)
        self.assertEqual(set(result),{"state","error"})
        self.assertEqual(result["state"],"invalid_native")

    def test_uncounted_context_cannot_manufacture_selection(self):
        a, b = rows(8_000_000_000), rows(5_500_000_000)
        def arm(r,device):
            return ({"model":"context_compact","provider":device,"roster_sha256":"roster","terminal":"settled",
                     "install_record_sha256":"install","rows":r},m["summary"](r,ORDER))
        joined = compare["compare"](arm(a,"cpu"),arm(b,"cuda"))
        self.assertTrue(joined["complete_workload_time_target_met"])
        self.assertEqual(joined["matched_selected_context_rows"], 0)
        self.assertEqual(joined["context_speed_evidence_status"], "unmeasured_no_matched_selected_inference")

    def test_genuine_registered_stdout_needs_the_applied_original_draft(self):
        read = compare["owner"]["registration_receipt"]
        receipt = {"schema":1,"stage":"ready","request_digest":"source"}
        draft = {"draft":{},"request_digest":"source"}
        raw = lambda d: (json.dumps(d)+"\n"+json.dumps({"receipt":receipt})).encode()
        self.assertEqual(read(raw(draft)), receipt)
        with self.assertRaisesRegex(ValueError, "draft"):
            read(raw(draft | {"request_digest":"changed"}))
        with self.assertRaisesRegex(ValueError, "ready"):
            read(json.dumps({"receipt":receipt | {"stage":"pending"}}).encode())

    def test_timeout_and_unmeasured_do_not_disappear(self):
        r = rows(10)
        r[-1].update(state="timeout",elapsed_ns=120_000_000_000)
        r[-2].update(state="timeout",elapsed_ns=121_000_000_000)
        s = m["summary"](r,ORDER)
        self.assertEqual((s["denominator"],s["complete"],s["timeout"],s["p95_elapsed_ns"]), (20,18,2,120_000_000_000))
        r[-3].update(state="deferred",elapsed_ns=None)
        self.assertIsNone(m["summary"](r,ORDER)["p95_elapsed_ns"])
        with self.assertRaises(ValueError):
            m["summary"](r[:-1],ORDER)

    def test_large_absolute_needs_verified_no_cpu_and_actual_cuda_activity(self):
        r = rows(12_000_000_000)
        run = {"provider":"cuda","model":"context_large","rows":r,"terminal":"settled","active":{"qualified_same_model_cpu_variants":[]}}
        def actual():
            return compare["large_absolute"]((run,m["summary"](r,ORDER)))
        self.assertFalse(actual()["context_absolute_latency_supported"])
        r[0]["native"] = {"selected_context":1,"context":[{"status":"selected","advice":{"runtime":"onnxruntime-cuda/1.30.0",
            "outcome":{"status":"accepted"},"observed_input_tokens":64,"work":{"inference_us":27},"input_digest":"row"}}]}
        self.assertTrue(actual()["context_absolute_latency_supported"])
        run["active"]["qualified_same_model_cpu_variants"] = ["ready cpu"]
        self.assertEqual(actual()["complete_workload_absolute_status"],"requires_qualified_cpu_comparison")
        run["active"]["qualified_same_model_cpu_variants"] = []
        r[-1]["elapsed_ns"] = 120_000_000_001
        self.assertFalse(actual()["context_absolute_latency_supported"])
        self.assertEqual(actual()["summary"]["deadline_overruns"],1)

    def test_current_native_warm_selection_refuses_fallback(self):
        cfg = {"revision":1}
        status = {"schema":"airk.arbiter.status/3","configuration":cfg}
        for stage, model, task, device in (("focus","focus","airk.focus_rank.v1",None),
                ("context","context_compact","airk.context_action.v1","cuda")):
            sel = {"manifest":stage,"execution_identity":stage+"-runtime","backend":device,"precision":"float16" if device else None}
            variant = sel | {"availability":"ready","model_admission":"qualified","tasks":[task],"device_evidence":{"placement":"p"}}
            cfg[stage] = {"enabled":True,"selected_model":model,"indexes":[{"index_id":"i","tasks":[task]}]}
            status[stage] = {"queued":0,"running":0,"availability":"ready","active_profile":stage,"selected_model":model,
                             "selection":{"model":model,"selected":sel,"operation_failure":None},
                             "models":[{"model":model,"variants":[variant]}]}
        spec = {"model":"context_compact","provider":"cuda","context_manifest":"context"}
        self.assertEqual(m["stages"](status,["i"],spec,"focus")["stages"]["context"]["selected"]["backend"],"cuda")
        tainted = copy.deepcopy(status)
        tainted["context"]["selection"]["selected"]["backend"] = None
        with self.assertRaisesRegex(ValueError, "fallback"):
            m["stages"](tainted,["i"],spec,"focus")
        with self.assertRaisesRegex(ValueError, "enrollment"):
            m["stages"](status,["j"],spec,"focus")


if __name__ == "__main__":
    unittest.main()
