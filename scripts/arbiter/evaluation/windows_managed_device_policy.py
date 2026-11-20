# SPDX-License-Identifier: Apache-2.0
"""Exercise public Windows device controls through the staged original CPU pair and an isolated managed service."""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
product = runpy.run_path(str(HERE/"windows_cpu_product.py"))
ROOT,E,GIB,require = (product[k] for k in ("ROOT","E","GIB","require"))
HISTORIC = E/"windows-managed-resource-policy-r1"
REFUSED = E/"windows-managed-resource-policy-r2"
EXISTING = E/"windows-managed-resource-policy-r3"
OUT = E/"windows-managed-resource-policy-r4"
AUDIT = HERE/"audit-windows-device-reuse.ps1"
MANIFEST = "20ea74f79a7416ce268c0de8d29be950aaf4c199df614444692ea37c84564302"
BUILD = product["OUT"]
WORKFLOW = HERE/"linux_cuda/device_policy_workflow.py"
CLIENT = ROOT/"scripts/check-service-clients.py"


def worker():
    root = OUT/"fixture"
    fixture = runpy.run_path(str(CLIENT))["Fixture"]
    workflow = runpy.run_path(str(WORKFLOW))["exercise"]
    def record(name,value):
        with (OUT/(name+".json")).open("x") as stream:
            json.dump(value,stream,allow_nan=False)
        return value
    historical=HISTORIC/"fixture"
    selected=fixture(BUILD/"runtime",root,preserve_pair_metadata=True)
    selected.install=historical/"install"
    selected.control=historical/"state/control"
    images=selected.install/"runtimes"/MANIFEST
    selected.binary,selected.cli=images/"airk.exe",images/"airkctl.exe"
    with contextlib.ExitStack() as stack:
        stack.push(selected)
        inspected=json.loads(selected.service_run("runtime","inspect","--install-dir",selected.install).stdout)
        require(inspected["selection"]["active"]==MANIFEST and Path(inspected["executable"]).samefile(selected.binary)
                and Path(inspected["control_dir"]).samefile(selected.control),"physical installed selection differs")
        require(selected.run("service","health","--runtime-dir",selected.control,check=False).returncode!=0,
                "original managed service is already running")
        selected.managed_started=True
        started=json.loads(selected.service_run("runtime","start","--install-dir",selected.install).stdout)
        selected.managed_health=selected.rpc("health")["observation"]
        status=selected.managed_health
        require(status["runtime_sha256"]==MANIFEST and status["executable_sha256"]==
                hashlib.sha256(selected.binary.read_bytes()).hexdigest(),"actual reopened service identity differs")
        start={"original_stage_activation_sha256":hashlib.sha256((HISTORIC/"managed-start.json").read_bytes()).hexdigest(),
               "explicit_start":started,"inspection":inspected,"health":status,"new_runtime_copies":0}
        with (root/"managed-start.json").open("x") as output:
            json.dump(start,output)
        original=json.loads((HISTORIC/"catalog-setup.json").read_bytes())["responses"][-1]["receipt"]
        require(original["stage"]=="ready","original source index was not ready")
        workflow(root,selected,record,require,existing_index=original["index_id"])
    record("managed-stop",selected.managed_stop)


def launch(args):
    ort = runpy.run_path(str(HERE/"windows_cpu_ort.py"))
    c,b,parent,owner,guard = ort["runtime"]()
    original = b.pin(E/"windows-stock-r2-custody-complete.json")
    require(original["sha256"]==args.product_sha256,"different original product")
    built=b.read(original["path"])
    require(built["passed"] and built["job"]["owned_descendants_settled"] and built["job"]["owned_job_final_active_processes"]==0,
            "closed original CPU product required")
    for entry in built["files"]:
        b.pinned(entry)
    previous=b.pin(HISTORIC/"execution.json")
    require(previous["sha256"]=="e03e30190498acfb24814efb05d6e002bedb4766b97fc695ce43bd3e08bbdd73" and
            b.pin(HISTORIC/"managed-start.json")["sha256"]=="99b06a66f4ac151bd41e94e658ad77eebfee49a56872783080ff93d94edacfe8" and
            b.pin(HISTORIC/"managed-stop.json")["sha256"]=="c770c4d5e9468b973b7380ddce558c4b7909948b443be839cad9056d957bcaeb",
            "original native stage, stop and postguard disposition required")
    def inspect(after=False):
        command=[guard["powershell"]["path"],"-NoProfile","-NonInteractive","-File",str(AUDIT)]
        return json.loads(subprocess.check_output(command+(["-After"] if after else []),text=True,timeout=60))
    refused=b.pin(REFUSED/"execution.json")
    require(refused["sha256"]=="1b88e901ff72fddbb3aa4142ab51c23f93278e73d67931d62c2fe329a8041426" and
            b.read(refused["path"])["job"]["owned_job_final_active_processes"]==0 and
            "service_explicitly_stopped" in (REFUSED/"native.log").read_text(),"original ensure must have refused a stopped runtime")
    require(b.pin(HISTORIC/"catalog-setup.json")["sha256"]=="9698a84bfddd288b0a5b69d049d911af6c96d98127b474495dee31a6cb10f631" and
            b.pin(EXISTING/"execution.json")["sha256"]=="79c0b5c454f1ac9877a9fd93a14714fd4f690634613f29a9e50ecc3999b967aa" and
            b.read(EXISTING/"fixture/managed-stop-result.json")["record"]["phase"]=="complete",
            "expected original existing-index attempt and real stopped service")
    custody_before=inspect()
    staged=HISTORIC/"fixture/install/runtimes"/MANIFEST
    def recheck():
        expected={Path(f["path"]).relative_to(BUILD/"runtime").as_posix():f["sha256"] for f in built["files"]}
        actual={p.relative_to(staged).as_posix():b.pin(p)["sha256"] for p in staged.rglob("*") if p.is_file()}
        require(b.read(staged/"approval.json")==b.read(HISTORIC/"managed-start.json")["stage"],
                "original native approved manifest grant differs")
        expected["approval.json"]="50039aaab340f09eb3eb5bdeeb8322bf973ca72f275d274c97eb7f9d9e5bf87d"
        require(actual==expected,"installed original complete offline file roster differs")
    recheck()
    remaining=built["remaining_storage"]-built["product_allocated_upper_bytes"]-product["allocated"](HISTORIC)-product["allocated"](REFUSED)-product["allocated"](EXISTING)-(4<<20)
    require(remaining>product["allocated"](BUILD/"runtime"),"remaining combined original allowance cannot stage the service")
    host=runpy.run_path(str(ROOT/"scripts/cuda-component-execution/ballast_host_commit.py"))["host"]()
    require(args.other_host_bytes>=0 and min(host["physical_available_bytes"],host["commit_limit_bytes"]-host["commit_bytes"])
            >= args.other_host_bytes+14*GIB,"insufficient physical or commit for isolated fixture")
    placement_pid=args.cal_pid
    placement,cores=product["quiet_cpus"](placement_pid)
    require(not OUT.exists(),"fresh service control required")
    c["mkdir"](guard,OUT)
    fixture,temp=OUT/"fixture",OUT/"temp"
    for path in [fixture,temp,fixture/"fixture-git-config",fixture/"fixture-git-config/xdg"]:
        c["mkdir"](guard,path)
    require((HISTORIC/"fixture/source/device-check.txt").read_text()=="Windows managed device policy fixture.\n",
            "original source corpus differs")
    env=os.environ|{"TMP":str(temp),"TEMP":str(temp),"TMPDIR":str(temp),"PYTHONDONTWRITEBYTECODE":"1",
                    "AIRK_PRIVATE_CENSUS_BASE":str(fixture),"RAYON_NUM_THREADS":"1","TOKIO_WORKER_THREADS":"2"}
    names = (Path(__file__).resolve(),HERE/"windows_cpu_product.py",HERE/"windows_cpu_ort.py",AUDIT,WORKFLOW,CLIENT,
             ROOT/"scripts/check-runtime-upgrade.py")
    sources=[b.pin(p) for p in names]
    deadline=time.monotonic()+300
    suspended=product["suspended_owner"](c,b,parent,owner,b.pin(Path(sys.executable)),deadline,cores,6*GIB)
    peak=0
    def observe(api,job,info,until):
        nonlocal peak
        while api.WaitForSingleObject(info.process,300)==258:
            peak=max(peak,product["allocated"](OUT))
            require(time.monotonic()<until and peak<remaining-(32<<20),"original service time or combined disk bound reached")
        return {"peak_output_upper_bytes":peak},[]
    cmd=[sys.executable,"-I","-B","-S",str(Path(__file__).resolve()),"--worker"]
    stdin,log=OUT/"stdin",OUT/"native.log"
    c["guard_many"](guard,[stdin,log])
    with stdin.open("xb"):
        pass
    report={"passed":False,"product":original,"sources":sources,"argv":cmd,"model_forward_requested":False,
            "control":str(HISTORIC/"fixture/state/control"),"prior_native":previous,"custody_before":custody_before,"inno_installer_exercised":False,
            "placement_observed_pid":placement_pid,"concurrent_cal_pid":args.cal_pid}
    try:
        with stdin.open("rb") as inp,log.open("xb") as sink:
            job=owner.run_windows_owned_training(cmd,ROOT,sink,deadline-time.monotonic(),env,inp,6*GIB,
                before_resume=suspended,after_resume=observe,absolute_deadline=deadline)
        report["job"]=job
        require(job["exit_code"]==0 and not job["timed_out"] and not job["owned_job_forced_termination"] and
                job["owned_descendants_settled"] and job["owned_job_final_active_processes"]==0 and
                job["owned_job_final_member_pids"]==[],"native service did not complete naturally")
        for entry in [original,*sources,*built["files"]]:
            b.pinned(entry)
        checks=[p for p in OUT.glob("*.json")]
        require((OUT/"managed-stop.json") in checks and (OUT/"same-model-cpu.json") in checks
                and (OUT/"after-refusal.json") in checks,"native milestones missing")
        recheck()
        report["custody_after"]=inspect(True)
        files=[p for p in OUT.rglob("*") if p.is_file()]
        for i in range(0,len(files),64):
            c["guard_many"](guard,files[i:i+64],True)
        report.update(passed=True,events=[b.pin(p) for p in sorted(checks)])
    except BaseException as exc:
        report["error"]=repr(exc)
        raise
    finally:
        report.update(log=b.pin(log),peak_output_upper_bytes=peak,remaining_allowance=remaining)
        c["write_new"](guard,OUT/"execution.json",report)


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--worker",action="store_true")
    p.add_argument("--product-sha256")
    p.add_argument("--other-host-bytes",type=int)
    p.add_argument("--cal-pid",type=int)
    args=p.parse_args()
    worker() if args.worker else launch(args)
