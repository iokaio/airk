# SPDX-License-Identifier: Apache-2.0
"""One exact retained original S8 numeric stage; no six-family, DEV, FINAL, compilation or retry."""
from pathlib import Path
import argparse,hashlib,importlib.util,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[4];HERE=Path(__file__).resolve().parent;MODELS=ROOT/"artifacts/advanced-split/models"
FAMILIES={"original-shipped-s8-ort130-evaluation"}
CONTROLS=set()
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load_anchor_support(packet,pins):
    source=Path(__file__).with_name("anchor_runtime.py").resolve()
    if pins.get(str(Path(__file__).resolve()))!=sha(Path(__file__).resolve()) or pins.get(str(source))!=sha(source):
        raise ValueError("reviewed native launcher/anchor source changed")
    spec=importlib.util.spec_from_file_location("_parent_bag_anchor",source)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    module.verified_sources(packet,pins)
    return module

class PhaseDeadline:
    """One original whole-phase deadline; injected clock is only a cheap test seam."""
    def __init__(self, seconds=1800, clock=time.monotonic):
        if seconds != 1800: raise ValueError("original1800second whole-phase cap required")
        self.clock=clock;self.started=clock();self.deadline=self.started+seconds
    def remaining(self):
        remaining=self.deadline-self.clock()
        if not 0 < remaining <= 1800: raise TimeoutError("original native whole-phase deadline expired or clock regressed")
        return remaining
    def elapsed(self):return self.clock()-self.started
    def within(self):return 0 <= self.elapsed() < 1800

def main():
    phase=PhaseDeadline()
    p=argparse.ArgumentParser(description=__doc__);p.add_argument("--packet",type=Path,required=True);p.add_argument("--packet-sha256",required=True);p.add_argument("--stage",choices=["numeric"],required=True);p.add_argument("--execute-reviewed",action="store_true");a=p.parse_args()
    phase.remaining()
    if sha(a.packet)!=a.packet_sha256:raise ValueError("root-reviewed native packet changed")
    packet=json.loads(a.packet.read_bytes());pins=dict(packet["pins"]);binary=Path(packet["binary"]["path"]);master=Path(packet["master"]);base=Path(packet["output"]);owner=ROOT/"tools/retrieval-train/training_execution.py"
    environment={"OMP_NUM_THREADS":"2","MKL_NUM_THREADS":"2","OPENBLAS_NUM_THREADS":"2","CUDA_VISIBLE_DEVICES":"","AIRK_ORIGINAL_S8_NATIVE_INPUT":str(master),"AIRK_ORIGINAL_S8_NATIVE_INPUT_SHA256":pins[str(master)],"AIRK_ORIGINAL_S8_RETENTION_DECISION":str(MODELS/"original-minilm-retention-decision-r1.json")}
    descriptor=json.loads(master.read_bytes())
    if len(descriptor)!=1 or descriptor[0]["family"]!="original-shipped-s8-ort130-evaluation" or descriptor[0]["oracle_result"]!="original-shipped-s8-ort130-eval":raise ValueError("exact retained original S8 family only")
    selector="inference_owner::evaluation_tests::original_s8::original_shipped_s8_matches_twelve_exact_native_oracles_and_serial_b1"
    if packet["limits"]!={"seconds":1800,"host_bytes":8589934592,"max_job_processes":2}:raise ValueError("original native resource limits required")
    anchor_support=load_anchor_support(packet,pins);anchor=anchor_support.anchor_path(packet,a.stage)
    anchor_support.require_fresh(anchor)
    environment["AIRK_PARENT_BAG_PRIVATE_BASE"]=str(anchor)
    for name,pin in pins.items():
        phase.remaining()
        if sha(name)!=pin:raise ValueError("native input/compiler/artifact drift: "+name)
        phase.remaining()
    listing=subprocess.run([str(binary),"--exact",selector,"--ignored","--list"],capture_output=True,text=True,timeout=min(10,phase.remaining()))
    phase.remaining()
    if listing.returncode or listing.stdout.count(selector+": test")!=1:raise ValueError("exactly one compiled native selector required")
    work=base/("execution-"+a.stage);output=work/"capture.json"
    environment["AIRK_ORIGINAL_S8_NATIVE_OUTPUT"]=str(output)
    if work.exists():raise ValueError("no overwrite or replay of native stage")
    argv=[str(binary),"--exact",selector,"--ignored","--nocapture","--test-threads=1"]
    if not a.execute_reviewed:print(json.dumps({"preflight_passed":True,"argv":argv,"private_anchor":str(anchor),"anchor_created":False,"stage_started":False}));return
    if packet.get("status")!="ROOT_RELEASED":raise ValueError("separate Root release absent")
    work.mkdir(parents=True);(work/"stdin.txt").write_bytes(b"")
    (work/"request.json").write_text(json.dumps({"argv":argv,"environment":environment,"pins":pins,"packet_sha256":a.packet_sha256,"limits":packet["limits"],"selector_list":listing.stdout},indent=2)+"\n")
    try:
        anchor_before=anchor_support.create(packet,pins,anchor,phase=phase)
        phase.remaining()
    except BaseException as error:
        (work/"execution.json").write_text(json.dumps({"complete":False,"native_started":False,"phase_seconds":1800,"phase_expired":not phase.within(),"elapsed_seconds":phase.elapsed(),
            "failure":{"type":type(error).__name__,"message":str(error)},"packet_sha256":a.packet_sha256,
            "stage":a.stage,"release_qualified":False},indent=2)+"\n")
        raise
    (work/"anchor-before.json").write_text(json.dumps(anchor_before,indent=2)+"\n")
    spec=importlib.util.spec_from_file_location("_owned_parent_bag_native",owner);native=importlib.util.module_from_spec(spec);spec.loader.exec_module(native);env=dict(os.environ);env.update(environment)
    def suspended(api,job,info,deadline):
        anchor_support.require_same(anchor_before,anchor_support.observe(packet,pins,anchor,phase=phase))
        phase.remaining()
        members,active,total=native.job_snapshot(api,job)
        if members!=[int(info.pid)] or active!=1 or total!=1:raise ValueError("one exact native process required")
        proof={"members":members,"active":active,"total":total,"image":native.image_proof(api,info.process,packet["binary"])}
        phase.remaining()
        return proof
    owned=None;anchor_after=None;failure=None
    try:
        with (work/"stdout.log").open("xb") as log,(work/"stdin.txt").open("rb") as stdin:
            owned=native.run_windows_owned_training(argv,ROOT,log,phase.remaining(),env,stdin,8589934592,before_resume=suspended)
    except BaseException as error:
        failure={"type":type(error).__name__,"message":str(error)}
    try:
        anchor_after=anchor_support.observe(packet,pins,anchor,phase=phase)
        anchor_support.require_same(anchor_before,anchor_after)
        phase.remaining()
    except BaseException as error:
        failure={"native_error":failure,"anchor_error":{"type":type(error).__name__,"message":str(error)}}
    if failure is not None:
        (work/"execution.json").write_text(json.dumps({"owned":owned,"complete":False,"phase_seconds":1800,"phase_expired":not phase.within(),"elapsed_seconds":phase.elapsed(),"failure":failure,
            "native_completion_unknown":owned is None,"anchor_before":anchor_before,"anchor_after":anchor_after,
            "packet_sha256":a.packet_sha256,"stage":a.stage,"release_qualified":False},indent=2)+"\n")
        raise RuntimeError("native stage or actual private-anchor proof failed; retained evidence is not a pass")
    drift=[name for name,pin in pins.items() if sha(name)!=pin]
    complete=owned["exit_code"]==0 and not owned["timed_out"] and not owned["owned_job_forced_termination"] and owned["owned_descendants_settled"] and owned["owned_job_final_active_processes"]==0 and owned["owned_total_processes"]==1 and owned["log_oversized"] is False and owned["owned_job_final_member_pids"]==[] and owned["unexpected_third_process"] is False and len(owned.get("retained_member_exits",[]))==1 and all(row["exit_observed"] and row["exit_code"]==0 for row in owned["retained_member_exits"]) and not drift
    capture=json.loads(output.read_bytes()) if output.is_file() else {}
    complete=complete and capture.get("complete") is True and capture.get("family")=="original-shipped-s8-ort130-evaluation" and len(capture.get("cases",[]))==12 and capture.get("mixed_length_execution",{}).get("workers")==1 and all(row["max_component_error"]<=1e-5 for row in capture.get("cases",[]))
    complete=complete and phase.within()
    result={"phase_seconds":1800,"phase_expired":not phase.within(),"owned":owned,"anchor_before":anchor_before,"anchor_after":anchor_after,"anchor_unchanged":anchor_before==anchor_after,"elapsed_seconds":phase.elapsed(),"post_input_drift":drift,"complete":complete,"release_qualified":False,"stage":a.stage,"packet_sha256":a.packet_sha256,"master_sha256":sha(master) if master.is_file() else None,"capture_sha256":sha(output) if output.is_file() else None}
    (work/"execution.json").write_text(json.dumps(result,indent=2)+"\n")
    if not phase.within():
        complete=False;result.update(complete=False,phase_expired=True,elapsed_seconds=phase.elapsed())
        (work/"execution.json").write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result))
    if not complete:raise SystemExit(1)
if __name__=="__main__":main()
