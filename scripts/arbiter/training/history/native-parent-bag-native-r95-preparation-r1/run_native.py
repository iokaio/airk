# SPDX-License-Identifier: Apache-2.0
"""One actual reviewed parent-bag native stage; no compilation/retry/FINAL."""
from pathlib import Path
import argparse,hashlib,importlib.util,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[4];HERE=Path(__file__).resolve().parent;MODELS=ROOT/"artifacts/advanced-split/models"
FAMILIES={"native-parent-bag-r1-u8","native-parent-bag-r1-s8"}
CONTROLS={"original-shipped-u8-ort130-evaluation","original-mixed-u8-ort130-evaluation"}
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
    p=argparse.ArgumentParser(description=__doc__);p.add_argument("--packet",type=Path,required=True);p.add_argument("--packet-sha256",required=True);p.add_argument("--stage",choices=["emit","numeric",*sorted(FAMILIES|CONTROLS)],required=True);p.add_argument("--execute-reviewed",action="store_true");a=p.parse_args()
    phase.remaining()
    if sha(a.packet)!=a.packet_sha256:raise ValueError("root-reviewed native packet changed")
    packet=json.loads(a.packet.read_bytes());pins=dict(packet["pins"]);binary=Path(packet["binary"]["path"]);master=Path(packet["master"]);base=Path(packet["output"]);owner=ROOT/"tools/retrieval-train/training_execution.py"
    environment={"OMP_NUM_THREADS":"2","MKL_NUM_THREADS":"2","OPENBLAS_NUM_THREADS":"2","AIRK_NATIVE_ENCODER_INPUT":str(master)}
    if a.stage=="emit":
        selector="emit_parent_bag_epoch1_native_manifests"
        environment.update({"AIRK_PARENT_BAG_EXPORTS":str(MODELS/"native-parent-bag-exports-r1"),"AIRK_PARENT_BAG_ORACLE":str(MODELS/"native-parent-bag-native-inputs-r1/receipt.json"),"AIRK_PARENT_BAG_MASTER_OUTPUT":str(master),"AIRK_PARENT_BAG_STAGE_SEAL":str(MODELS/"native-parent-bag-export-execution-r1/seal.json")})
    else:
        emission=json.loads((base/"execution-emit/execution.json").read_bytes())
        if emission.get("complete") is not True or emission.get("packet_sha256")!=a.packet_sha256 or emission.get("master_sha256")!=sha(master):raise ValueError("same-packet complete native emission proof required")
        descriptor=json.loads(master.read_bytes());pins[str(master)]=emission["master_sha256"]
        if a.stage in FAMILIES|CONTROLS:
            numeric=json.loads((base/"execution-numeric/execution.json").read_bytes())
            if numeric.get("complete") is not True or numeric.get("packet_sha256")!=a.packet_sha256 or numeric.get("master_sha256")!=pins[str(master)]:raise ValueError("same-image/master complete native numeric proof required before DEV")
        if len(descriptor)!=2 or {r["family"] for r in descriptor}!=FAMILIES:raise ValueError("exact Rust-emitted two-family descriptor required")
        environment["AIRK_PARENT_BAG_ENCODER_INPUT_SHA256"]=pins[str(master)]
        for row in descriptor:
            manifest=Path(row["directory"])/"model.json";pins[str(manifest)]=row["model_json_sha256"]
        if a.stage=="numeric":selector="parent_bag_two_families_match_independent_native_bucket_oracles"
        else:selector="parent_bag_dev64_questions_use_current_kernel_with_fixed20_quality_mask"
    if a.stage in CONTROLS:
        descriptor=Path(packet["closed_controls"])
        environment["AIRK_NATIVE_ENCODER_INPUT"]=str(descriptor)
        selector="inference_owner::evaluation_tests::development::native_dev64_questions_use_current_kernel_with_fixed20_quality_mask"
    else:selector="inference_owner::evaluation_tests::native_parent_bag::"+selector
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
    environment["AIRK_NATIVE_ENCODER_OUTPUT"]=str(output)
    if a.stage in FAMILIES|CONTROLS:
        manifest=MODELS/"a1-component-dev-native-fixture-r1/manifest.private.json"
        environment.update({"AIRK_NATIVE_ENCODER_FAMILY":a.stage,"AIRK_NATIVE_DEV64_CORPUS":str(MODELS/"a1-component-development-materialized-r1/corpus.private.json"),"AIRK_NATIVE_DEV64_MASK":str(MODELS/"a1-component-dev-evaluation-protocol-r1/quality-mask.json"),"AIRK_NATIVE_DEV64_SOURCE_MANIFEST":str(manifest),"AIRK_NATIVE_DEV64_SOURCE_MANIFEST_SHA256":pins[str(manifest)],"AIRK_NATIVE_DEV64_DATA":str(work/"data"),"AIRK_NATIVE_DEV64_OUTPUT":str(output),"AIRK_NATIVE_DEV64_STOP_FILE":str(work/"STOP")})
    if work.exists():raise ValueError("no overwrite or replay of native stage")
    argv=[str(binary),"--exact",selector,"--ignored","--nocapture","--test-threads=1"]
    if not a.execute_reviewed:print(json.dumps({"preflight_passed":True,"argv":argv,"private_anchor":str(anchor),"anchor_created":False,"stage_started":False}));return
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
    complete=owned["exit_code"]==0 and not owned["timed_out"] and not owned["owned_job_forced_termination"] and owned["owned_descendants_settled"] and owned["owned_job_final_active_processes"]==0 and owned["owned_total_processes"]==1 and owned["log_oversized"] is False and owned["owned_job_final_member_pids"]==[] and owned["unexpected_third_process"] is False and not drift
    if a.stage!="emit":
        capture=json.loads(output.read_bytes()) if output.is_file() else {};complete=complete and capture.get("complete") is True
        if a.stage=="numeric":complete=complete and len(capture.get("families",[]))==2 and all(len(r["cases"])==12 for r in capture["families"])
        else:complete=complete and len(capture.get("records",[]))==64 and capture.get("quality_denominator")==20 and capture.get("alignment_refusals")==44
    else:complete=complete and master.is_file()
    complete=complete and phase.within()
    result={"phase_seconds":1800,"phase_expired":not phase.within(),"owned":owned,"anchor_before":anchor_before,"anchor_after":anchor_after,"anchor_unchanged":anchor_before==anchor_after,"elapsed_seconds":phase.elapsed(),"post_input_drift":drift,"complete":complete,"release_qualified":False,"stage":a.stage,"packet_sha256":a.packet_sha256,"master_sha256":sha(master) if master.is_file() else None,"capture_sha256":sha(output) if output.is_file() else None}
    (work/"execution.json").write_text(json.dumps(result,indent=2)+"\n")
    if not phase.within():
        complete=False;result.update(complete=False,phase_expired=True,elapsed_seconds=phase.elapsed())
        (work/"execution.json").write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result))
    if not complete:raise SystemExit(1)
if __name__=="__main__":main()
