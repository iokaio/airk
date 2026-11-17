# SPDX-License-Identifier: Apache-2.0
"""One actual reviewed parent-bag native stage; no compilation/retry/FINAL."""
from pathlib import Path
import argparse,hashlib,importlib.util,json,os,subprocess,time
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir());HERE=Path(__file__).resolve().parent;MODELS=ROOT/"artifacts/advanced-split/models"
FAMILIES={"native-parent-bag-r1-u8","native-parent-bag-r1-s8"}
CONTROLS={"original-shipped-u8-ort130-evaluation","original-mixed-u8-ort130-evaluation"}
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument("--packet",type=Path,required=True);p.add_argument("--packet-sha256",required=True);p.add_argument("--stage",choices=["emit","numeric",*sorted(FAMILIES|CONTROLS)],required=True);p.add_argument("--execute-reviewed",action="store_true");a=p.parse_args()
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
    for name,pin in pins.items():
        if sha(name)!=pin:raise ValueError("native input/compiler/artifact drift: "+name)
    listing=subprocess.run([str(binary),"--exact",selector,"--ignored","--list"],capture_output=True,text=True,timeout=10)
    if listing.returncode or listing.stdout.count(selector+": test")!=1:raise ValueError("exactly one compiled native selector required")
    work=base/("execution-"+a.stage);output=work/"capture.json"
    environment["AIRK_NATIVE_ENCODER_OUTPUT"]=str(output)
    if a.stage in FAMILIES|CONTROLS:
        manifest=MODELS/"a1-component-dev-native-fixture-r1/manifest.private.json"
        environment.update({"AIRK_NATIVE_ENCODER_FAMILY":a.stage,"AIRK_NATIVE_DEV64_CORPUS":str(MODELS/"a1-component-development-materialized-r1/corpus.private.json"),"AIRK_NATIVE_DEV64_MASK":str(MODELS/"a1-component-dev-evaluation-protocol-r1/quality-mask.json"),"AIRK_NATIVE_DEV64_SOURCE_MANIFEST":str(manifest),"AIRK_NATIVE_DEV64_SOURCE_MANIFEST_SHA256":pins[str(manifest)],"AIRK_NATIVE_DEV64_DATA":str(work/"data"),"AIRK_NATIVE_DEV64_OUTPUT":str(output),"AIRK_NATIVE_DEV64_STOP_FILE":str(work/"STOP")})
    if work.exists():raise ValueError("no overwrite or replay of native stage")
    argv=[str(binary),"--exact",selector,"--ignored","--nocapture","--test-threads=1"]
    if not a.execute_reviewed:print(json.dumps({"preflight_passed":True,"argv":argv,"stage_started":False}));return
    work.mkdir(parents=True);(work/"stdin.txt").write_bytes(b"")
    (work/"request.json").write_text(json.dumps({"argv":argv,"environment":environment,"pins":pins,"packet_sha256":a.packet_sha256,"limits":packet["limits"],"selector_list":listing.stdout},indent=2)+"\n")
    spec=importlib.util.spec_from_file_location("_owned_parent_bag_native",owner);native=importlib.util.module_from_spec(spec);spec.loader.exec_module(native);env=dict(os.environ);env.update(environment);started=time.monotonic()
    def suspended(api,job,info,deadline):
        members,active,total=native.job_snapshot(api,job)
        if members!=[int(info.pid)] or active!=1 or total!=1:raise ValueError("one exact native process required")
        return {"members":members,"active":active,"total":total,"image":native.image_proof(api,info.process,packet["binary"])}
    with (work/"stdout.log").open("xb") as log,(work/"stdin.txt").open("rb") as stdin:owned=native.run_windows_owned_training(argv,ROOT,log,1800,env,stdin,8589934592,before_resume=suspended)
    drift=[name for name,pin in pins.items() if sha(name)!=pin]
    complete=owned["exit_code"]==0 and not owned["timed_out"] and not owned["owned_job_forced_termination"] and owned["owned_descendants_settled"] and owned["owned_job_final_active_processes"]==0 and owned["owned_total_processes"]==1 and owned["log_oversized"] is False and owned["owned_job_final_member_pids"]==[] and owned["unexpected_third_process"] is False and not drift
    if a.stage!="emit":
        capture=json.loads(output.read_bytes()) if output.is_file() else {};complete=complete and capture.get("complete") is True
        if a.stage=="numeric":complete=complete and len(capture.get("families",[]))==2 and all(len(r["cases"])==12 for r in capture["families"])
        else:complete=complete and len(capture.get("records",[]))==64 and capture.get("quality_denominator")==20 and capture.get("alignment_refusals")==44
    else:complete=complete and master.is_file()
    result={"owned":owned,"elapsed_seconds":time.monotonic()-started,"post_input_drift":drift,"complete":complete,"release_qualified":False,"stage":a.stage,"packet_sha256":a.packet_sha256,"master_sha256":sha(master) if master.is_file() else None,"capture_sha256":sha(output) if output.is_file() else None}
    (work/"execution.json").write_text(json.dumps(result,indent=2)+"\n");print(json.dumps(result))
    if not complete:raise SystemExit(1)
if __name__=="__main__":main()
