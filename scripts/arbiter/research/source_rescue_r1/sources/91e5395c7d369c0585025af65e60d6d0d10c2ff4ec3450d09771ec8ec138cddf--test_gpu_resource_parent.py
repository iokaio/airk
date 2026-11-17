# SPDX-License-Identifier: Apache-2.0
"""Literal private predicate/clock/capacity controls, never native qualification."""
import ast,copy,importlib.util,json,tempfile,unittest
from pathlib import Path
import gpu_parent_policy as policy
import launch_resource20_gpu as parent
import capacity_resource20_gpu as capacity
import native_operations_watchdog_gpu as operations
import bind_resource20_gpu as factory

MODELS = (
    ("arbiter-focus-qwen3-reranker-06b",4<<30,8<<30),
    ("arbiter-context-qwen3-17b",16<<30,32<<30),
    ("arbiter-context-qwen3-4b",40<<30,48<<30),
)
def literals(model=MODELS[1][0],host=16<<30,job=32<<30):
    # Independent small predicate fixture, NOT a native request or emitted output.
    resource_policy={"deadline_ms":120000}
    forwards=["warmup-0"]+[f"sample-{n:02}" for n in range(1,21)]
    roster=[{"kind":"load","ordinal":0,"input_digest":None},
            {"kind":"warmup","ordinal":0,"input_digest":"a"*64}]+[
            {"kind":"sample","ordinal":n,"input_digest":"a"*64} for n in range(1,21)]
    wrapper={"schema":"airk.private.cuda-resource-benchmark-request/1","outer_deadline_ms":3600000,
        "host_limit_bytes":host,"job_limit_bytes":job,"cold_bytes":10,"warm_bytes":0,"headroom_bytes":2,
        "device_limit_bytes":12,"gpu_input":{"path":"native-input","bytes":10,"sha256":"a"*64},
        "runtime_manifest":{"path":"runtime","bytes":10,"sha256":"b"*64},
        "expectations":{"path":"expectations","bytes":10,"sha256":"c"*64}}
    request={"numeric_fixture":None,"policy":resource_policy,"policy_digest":"d"*64}
    profile={"model":model,"manifest":"e"*64,"cpu_fit":None,"device_execution":{"opaque":"GPU"},
             "memory_reservation_bytes":host}
    expected={"schema":"airk.private.cuda-resource-benchmark-expectations/1","native_input":wrapper["gpu_input"],
        "model":model,"profile":"e"*64,"policy":resource_policy,"policy_digest":"d"*64,
        "expected_resource_input":{"opaque":"complete compiler output"},"native_operation_roster":roster,"forward_roster":forwards}
    native_policy={"schema":"airk.private.cuda-resource-benchmark-policy/1","resource_policy":resource_policy,
        "whole_process_ms":3600000,"load_operations":1,"warmup_operations":1,"sample_operations":20,
        "gpu_process_peak_required_for_final_qualification":True,"forward_roster":forwards,
        "model_limits":[{"model":m,"host_limit_bytes":h,"process_containment_bytes":j,"job_limit_bytes":j} for m,h,j in MODELS]}
    plan={"model":model,"profile":"e"*64,"cpu_fit":None,"reservation_bytes":host,
        "limits":{"whole_process_seconds":3600,"host_committed_bytes":job,"log_bytes":64<<20,
                  "operation_journal_bytes":131072,"per_operation_ms":120000},
        "policy":resource_policy,"policy_digest":"d"*64,"native_input":{"path":"wrapper","bytes":10,"sha256":"f"*64},
        "expected_resource_input":expected["expected_resource_input"],"native_operation_roster":roster,"forward_roster":forwards}
    body={"model":model,"schema":"airk.private.cuda-native-resource/1","complete":True,"latency_host_placement_passed":True,
        "final_gpu_qualification":False,"release_qualified":False,"numeric_status":"unavailable_in_resource_only_run",
        "gpu_peak_status":"unavailable_complete_process_observer","measured_peak_device_bytes":None,"cpu_fit":None,
        "device_execution":profile["device_execution"],"bounds":wrapper,"native_input":wrapper["gpu_input"],
        "runtime_manifest":wrapper["runtime_manifest"],"request":plan["native_input"],"expectations":wrapper["expectations"],
        "native_operation_roster":roster,"native_control_retained_for_unknown_cuda":False,
        "native_control_cleanup_observed":True,"native_control_cleanup_error":None,"native_host_charge_release_observed":True,
        "process_cuda_poisoned":False,"final_host_peak_error":None,"peak_process_rss_bytes":host,
        "native_host_reservation_bytes":host,"actual_job":{"process_memory_limit_bytes":job,"job_memory_limit_bytes":job,"flags":0x2300},
        "resource":{"complete":True,"attempted_samples":20,"samples":[{"opaque":n} for n in range(20)],
                    "failure":None,"reservation_bytes":host},
        "input":expected["expected_resource_input"],"forward_roster":forwards,"completed_forwards":forwards}
    return plan,wrapper,request,profile,expected,native_policy,body

class Controls(unittest.TestCase):
    def test_all_original_native_model_pairs(self):
        for model,host,job in MODELS:
            with self.subTest(model=model):
                p,w,r,f,e,n,b=literals(model,host,job)
                self.assertEqual(policy.documents(p,w,r,f,e,n),(host,job))
                policy.result(b,p,w,r,f,e)
    def test_native_model_case_omission_substitution_or_float_refuses(self):
        for mode in ("missing","substitution","float"):
            p,w,r,f,e,n,b=literals()
            if mode=="missing":n["model_limits"].pop()
            elif mode=="substitution":n["model_limits"][1]["job_limit_bytes"]=24<<30
            else:n["model_limits"][0]["host_limit_bytes"]=float(4<<30)
            with self.assertRaises(ValueError):policy.documents(p,w,r,f,e,n)
    def test_wrapper_nested_expectations_and_cpu_identity_refuse(self):
        for key in ("wrapper","expected","cpu","profile"):
            p,w,r,f,e,n,b=literals()
            if key=="wrapper":w["outer_deadline_ms"]=3600001
            elif key=="expected":e["native_input"]={"foreign":"pin"}
            elif key=="cpu":f["cpu_fit"]={"not":"GPU"}
            else:f["manifest"]="f"*64
            with self.assertRaises(ValueError):policy.documents(p,w,r,f,e,n)
    def test_complete_population_and_original_policy_cannot_shrink(self):
        for mode in ("sample","forward","deadline"):
            p,w,r,f,e,n,b=literals()
            if mode=="sample":p["native_operation_roster"]=p["native_operation_roster"][:-1]
            elif mode=="forward":p["forward_roster"]=p["forward_roster"][:-1]
            else:p["limits"]["per_operation_ms"]=120001
            with self.assertRaises(ValueError):policy.documents(p,w,r,f,e,n)
    def test_partial_or_cpu_native_result_cannot_promote(self):
        for key,value in (("schema","airk.arbiter.native-resource/2"),("complete",False),
                          ("final_gpu_qualification",True),("measured_peak_device_bytes",1),
                          ("cpu_fit",{}),("latency_host_placement_passed",False)):
            p,w,r,f,e,n,b=literals();b[key]=value
            with self.subTest(key=key):
                with self.assertRaises(ValueError):policy.result(b,p,w,r,f,e)
    def test_unknown_settlement_missing_release_and_late_peak_refuse(self):
        for key,value in (("native_control_retained_for_unknown_cuda",True),
                          ("native_control_cleanup_observed",False),("native_host_charge_release_observed",False),
                          ("process_cuda_poisoned",True),("peak_process_rss_bytes",None),
                          ("peak_process_rss_bytes",(16<<30)+1)):
            p,w,r,f,e,n,b=literals();b[key]=value
            with self.subTest(key=key):
                with self.assertRaises(ValueError):policy.result(b,p,w,r,f,e)
    def test_full_resource_input_samples_and_native_completed_roster_refuse(self):
        for mode in ("input","samples","completed"):
            p,w,r,f,e,n,b=literals()
            if mode=="input":b["input"]={"different":"native input"}
            elif mode=="samples":b["resource"]["samples"]=b["resource"]["samples"][:-1]
            else:b["completed_forwards"]=b["completed_forwards"][:-1]
            with self.assertRaises(ValueError):policy.result(b,p,w,r,f,e)
    def test_capacity_live_peaks_enforce_each_original_host_pair(self):
        for model,host,job in MODELS:
            c=capacity.Capacity(host,job,clock=lambda:0)
            c.observe({},{"peak_rss":host,"peak_job_commit":host})
            with self.assertRaises(ValueError):c.observe({},{"peak_rss":host,"peak_job_commit":host+1})
        with self.assertRaises(ValueError):capacity.Capacity(16<<30,24<<30)
    def test_capacity_retention1s_does_not_decimate_live_guards(self):
        now=[0.0];c=capacity.Capacity(4<<30,8<<30,clock=lambda:now[0])
        for index in range(10):
            now[0]=index/10;c.observe({},{"peak_rss":index+1,"peak_job_commit":index+1})
        self.assertEqual(c.observations,10);self.assertEqual(len(c.samples),1)
        with self.assertRaises(ValueError):c.observe({},{"peak_rss":(4<<30)+1,"peak_job_commit":10})
    def test_clipped_operation_allows_no_renewal_or_expansion(self):
        identity={"literal":"native identity"}
        state=operations.Operations(identity,[{"kind":"load","ordinal":0}],120000)
        event={"schema":"airk.arbiter.native-operation/1","sequence":1,"identity":identity,
            "kind":"load","ordinal":0,"operation_input_digest":None,"phase":"started",
            "clock":"windows_performance_counter","started_monotonic_ns":1,"expires_monotonic_ns":5,
            "observed_monotonic_ns":1,"native_call_elapsed_us":None,"native_success":None}
        raw=(json.dumps(event)+"\n").encode();state.feed(raw,2)
        returned=dict(event,sequence=2,phase="returned",observed_monotonic_ns=3,native_call_elapsed_us=0,native_success=True)
        state.feed(raw+(json.dumps(returned)+"\n").encode(),4)
        with self.assertRaises(operations.ObservationFailure):state.check_expiry(5)
        for expires in (1,120000000002):
            bad=operations.Operations(identity,[{"kind":"load","ordinal":0}],120000)
            with self.assertRaises(operations.ObservationFailure):bad.feed((json.dumps(dict(event,expires_monotonic_ns=expires))+"\n").encode(),2)
        bad=operations.Operations(identity,[{"kind":"load","ordinal":0}],120000);bad.feed(raw,2)
        with self.assertRaises(operations.ObservationFailure):
            bad.feed(raw+(json.dumps(dict(returned,expires_monotonic_ns=6))+"\n").encode(),4)
    def test_late_stdout_hash_keeps_original_failure_and_clears_pass(self):
        receipt={"post_input_drift":[],"complete":True,"measurement_passed":True,"failure":None}
        now=[1.0]
        def final_hash(path):now[0]=11.0;return "a"*64
        with tempfile.TemporaryDirectory(dir=Path(__file__).parent) as d:
            p=Path(d)/"log";p.write_bytes(b"raw")
            parent.finish_observations(receipt,0,10,p,clock=lambda:now[0],hash_file=final_hash)
        self.assertTrue(receipt["whole_phase_expired"]);self.assertFalse(receipt["measurement_passed"])
    def test_original_ownership_and_final_clock_bodies_are_mechanically_preserved(self):
        original=Path(parent.BASE/"launch_resource20_current.py").read_text()
        revised=Path(parent.__file__).read_text()
        def function(text,name):
            return ast.dump(next(n for n in ast.parse(text).body if isinstance(n,ast.FunctionDef) and n.name==name),include_attributes=False)
        for name in ("pin","check_combined_source_bridge","finish_observations","write"):
            self.assertEqual(function(original,name),function(revised,name))
        self.assertIn("absolute_deadline=deadline",revised)
        self.assertLess(revised.index("receipt['attention_dispatch']=resource_result"),revised.rindex("finish_observations(receipt"))
        self.assertIn("self.reader=WindowsRead(containment_bytes)",Path(__file__).with_name("watch_resource20_gpu.py").read_text())
    def test_g5_missing_actual_native_plan_refuses_even_after_other_controls(self):
        p,w,r,f,e,n,b=literals();p["attention_observation_required"]=True
        with self.assertRaises(ValueError):parent.resource_result(b,p,w,r,f,e,b"")
        p["attention_observation_required"]=False;b["attention_backend_plan"]={"unexpected":"must not bypass"}
        with self.assertRaises(ValueError):parent.resource_result(b,p,w,r,f,e,b"")


    def test_factory_preserves_nested_native_bytes_and_current_source_bindings(self):
        p,w,r,f,e,n,b=literals()
        with tempfile.TemporaryDirectory(dir=Path(__file__).parent) as d:
            root=Path(d)
            def save(name,value):
                path=root/name;path.write_text(json.dumps(value),encoding="utf8")
                return factory.file_pin(path,parent.digest(path))
            asset=root/"asset.bin";asset.write_bytes(b"literal predicate data, not a model")
            f["assets"]=[{"name":"literal","bytes":asset.stat().st_size,"sha256":parent.digest(asset)}]
            r["files"]={"literal":str(asset)}
            r["profile"]=save("profile.json",f);r["source_manifest"]=save("source.json",{"literal":"source"})
            core_pin=save("core.json",r);w["gpu_input"]=core_pin;e["native_input"]=core_pin
            w["expectations"]=save("expected.json",e);w["runtime_manifest"]=save("runtime.json",{"literal":"runtime"})
            wrapper_pin=save("wrapper.json",w);policy_pin=save("policy.json",n)
            emission_pin=save("emission.json",{"literal":"not actual native evidence"})
            observer_pin=save("observer.json",{"literal":"source pin only in this non-executing factory control"})
            template={"input_seals":{},"required_observer_source_changes":{"original":"a"*64},
                      "unchanged_scoring_source_pins":[{"path":"original-source","sha256":"b"*64}]}
            before={path:path.read_bytes() for path in root.iterdir()}
            result=factory.bind(template,wrapper_pin,policy_pin,emission_pin,observer_pin,False)
            self.assertEqual(result["required_observer_source_changes"],{"original":"a"*64})
            self.assertEqual(result["unchanged_scoring_source_pins"],template["unchanged_scoring_source_pins"])
            self.assertEqual(result["native_input"],wrapper_pin)
            self.assertEqual(result["forward_roster"],e["forward_roster"])
            self.assertIn(str(asset),result["input_seals"])
            self.assertEqual({path:path.read_bytes() for path in before},before)
    def test_g5_positive_complete_roster_remains_unqualified(self):
        p,w,r,f,e,n,b=literals();p["attention_observation_required"]=True
        observer=Path(__file__).parent.parent/"gpu-attention-backend-observer-proposal-r1"/"attention_backend.py"
        p["attention_observer"]={"path":str(observer),"sha256":parent.digest(observer)}
        f["device_execution"]={"graph":"a"*64,"math_policy":"b"*64,"provider_options":"c"*64}
        b["device_execution"]=f["device_execution"]
        names=("/airk_g5/attention/0/efficient","/airk_g5/attention/1/efficient")
        b["attention_backend_plan"]={"schema":"airk.native.cuda-fp16-attention-plan/1","model":f["model"],
            "graph":"a"*64,"math_policy":"b"*64,"provider_options":"c"*64,"dtype":"fp16",
            "domain":"com.microsoft","operator":"GroupQueryAttention","backend":"FLASH_ATTENTION",
            "actual_backend_observed":False,"forward_completion_claimed":False,"qualification":False,
            "nodes":[{"name":name,"operator":"GroupQueryAttention","domain":"com.microsoft"} for name in names]}
        raw="".join(f"Operator=GroupQueryAttention Node={name} DataType=fp16 SdpaKernel=FLASH_ATTENTION\n"
                    for _ in range(21) for name in names).encode()
        observed=parent.resource_result(b,p,w,r,f,e,raw)
        self.assertEqual(len(observed["groups"]),21);self.assertFalse(observed["qualification_granted"])
        b["completed_forwards"]=b["completed_forwards"][:-1]
        with self.assertRaises(ValueError):parent.resource_result(b,p,w,r,f,e,raw)

if __name__=="__main__":unittest.main()
