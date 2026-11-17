# SPDX-License-Identifier: Apache-2.0
"""Execute only the separately reviewed bounded CPU export; never train or use GPU."""
from pathlib import Path
import argparse,hashlib,importlib.util,json,os,time
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir());HERE=Path(__file__).resolve().parent
OWNER=ROOT/"tools/retrieval-train/training_execution.py"
OWNER_SHA="d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975"
def sha(p):
    with Path(p).open("rb") as stream:return hashlib.file_digest(stream,"sha256").hexdigest()
def natural(r):
    return r.get("exit_code")==0 and r.get("timed_out") is False and r.get("log_oversized") is False and r.get("owned_descendants_settled") is True and r.get("owned_job_forced_termination") is False and r.get("owned_job_final_active_processes")==0 and r.get("owned_job_final_member_pids")==[] and r.get("owned_total_processes")==2 and r.get("unexpected_third_process") is False

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument("--seal",type=Path,required=True);p.add_argument("--seal-sha256",required=True);p.add_argument("--execute-reviewed",action="store_true");a=p.parse_args()
    if os.name!="nt" or sha(a.seal)!=a.seal_sha256 or sha(OWNER)!=OWNER_SHA:raise ValueError("reviewed Windows owner/seal required")
    seal=json.loads(a.seal.read_bytes())
    if seal["limits"]!={"whole_seconds":600,"aggregate_host_bytes":8589934592,"job_processes":2,"CPU_only":True}:raise ValueError("unchanged export resources required")
    for row in seal["files"]:
        if sha(row["path"])!=row["sha256"]:raise ValueError("runtime/source/input changed: "+row["path"])
    work=Path(seal["output"]);log=work/"stdout.log";receipt=work/"execution.json"
    if log.exists() or receipt.exists():raise ValueError("no overwrite/replay of a CPU stage")
    argv=[seal["file_roles"]["venv_launcher"]["path"],"-B",str(HERE/"stage.py"),str(a.seal.resolve()),a.seal_sha256]
    if not a.execute_reviewed:print(json.dumps({"preflight_passed":True,"argv":argv,"execution_started":False}));return
    spec=importlib.util.spec_from_file_location("_owned_CPU_export",OWNER);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
    environment=dict(os.environ);environment.update({"PYTHONDONTWRITEBYTECODE":"1","CUDA_VISIBLE_DEVICES":"-1","OMP_NUM_THREADS":"2","MKL_NUM_THREADS":"2","OPENBLAS_NUM_THREADS":"2","HF_HUB_OFFLINE":"1","TRANSFORMERS_OFFLINE":"1"})
    (work/"stdin.txt").write_bytes(b"")
    def suspended(api,job,info,deadline):
        members,active,total=owner.job_snapshot(api,job)
        if members!=[int(info.pid)] or active!=1 or total!=1:raise ValueError("exact suspended venv leader required")
        return {"members":members,"active":active,"total":total,"image":owner.image_proof(api,info.process,seal["file_roles"]["venv_launcher"])}
    started=time.monotonic()
    with log.open("xb") as output,(work/"stdin.txt").open("rb") as stdin:
        owned=owner.run_windows_owned_training(argv,ROOT,output,600,environment,stdin,8589934592,before_resume=suspended)
    drift=[row["path"] for row in seal["files"] if sha(row["path"])!=row["sha256"]]
    result={"argv":argv,"seal_sha256":a.seal_sha256,"owned":owned,"elapsed_seconds":time.monotonic()-started,"post_input_drift":drift,"CPU_only":True,"complete":natural(owned) and not drift,"release_qualified":False}
    receipt.write_text(json.dumps(result,indent=2)+"\n");print(json.dumps(result))
    if not result["complete"]:raise SystemExit(1)
if __name__=="__main__":main()
