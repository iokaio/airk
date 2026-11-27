# SPDX-License-Identifier: Apache-2.0
"""Build the local unsigned installer with the genuine stock compiler and the tested debug CPU pair."""
import json
import os
from pathlib import Path
import runpy
import shutil
import sys
import time

HERE = Path(__file__).resolve().parent
product = runpy.run_path(str(HERE/"windows_cpu_product.py"))
ROOT,E,GIB,require = (product[k] for k in ("ROOT","E","GIB","require"))
VIEW = E.parent/"iv"
OUT = E/"windows-native-local-installer-r2"
PACKAGES = VIEW/"artifacts/local-service-installer"
ISCC = Path("C:/Users/tdotn/AppData/Local/Programs/Inno Setup 6/ISCC.exe")


def launch(concurrent):
    import psutil
    ort = runpy.run_path(str(HERE/"windows_cpu_ort.py"))
    c,b,parent,owner,guard = ort["runtime"]()
    def seal(path, expected):
        value = b.pin(path)
        require(value["sha256"]==expected,f"original {path.name} changed")
        return value
    closed = seal(E/"windows-stock-r2-custody-complete.json","a18970067ce882077eaa36fedaf2d3245e2f1ac7c9612c3762edb098eda99b35")
    built = b.read(closed["path"])
    for row in built["files"]:
        b.pinned(row)
    view = seal(E/"windows-stock-installer-source-view-r1.json","fa844bb047fb4298e0d40048f12241bc513e0d0128cf81abf785dd603a74ffba")
    compiler = seal(ISCC,"eb6f4410c8db367a5f74127e8025ad2ccacc0afabbe783959d237df3050f97fb")
    script = VIEW/"scripts/build-local-service-installer.py"
    stock = seal(script,"4c5cdf39a15383c6d30ee3960f7297bcb661ad3aace526dda6f9d842e954305a")
    require(b.pin(ROOT/"scripts/build-local-service-installer.py")["sha256"]==stock["sha256"],"non-stock installer implementation")
    source = runpy.run_path(str(VIEW/"scripts/build-service-runtime.py"))["source_identity"]
    require(source()==b.read(view["path"])["source"]==built["source_identity"],"checked source view differs")
    host=runpy.run_path(str(ROOT/"scripts/cuda-component-execution/ballast_host_commit.py"))["host"]()
    require(concurrent>=0 and min(host["physical_available_bytes"],host["commit_limit_bytes"]-host["commit_bytes"])>=concurrent+14*GIB,
            "six-GiB packaging Job, OS floor or concurrent owners do not fit")
    require(not OUT.exists() and not PACKAGES.exists() and Path(os.environ["CARGO_HOME"])==E/"windows-offline-cargo-r1/home",
            "fresh stock output and original D Cargo required")
    moved=seal(E/"windows-stock-installer-source-transfer-r2.json","4c7bc4d057ff09a298427a3452ae4487a0d5fd300d624f0ef65f01f3789d88bc")
    previous=seal(E/"windows-native-local-installer-r1/execution.json","e366fd99c751812b5cfb6cdcb41bb6fee0f27b12e0a74d910160489be3b7ef5b")
    first_wrapper=seal(E/"windows-stock-cpu-installer-original-r1.py","342bef40bcedb0f90b97d5d9d52a6465d9e81a18ff25683014718450ac88ac68")
    paths=seal(E/"windows-iscc-long-path-controls-r1/receipt.json","04456bcbf3de77e4c44b4d34b1addf4508693f2bab6d5864329c746839787c6a")
    require(b.read(paths["path"])["short_only_compiles"] and Path(b.read(moved["path"])["source"])==VIEW,
            "original path-length diagnosis or physical transfer differs")
    historical=sum(product["allocated"](path) for path in (E/"iv",E/"windows-native-local-installer-r1",
                   E/"windows-iscc-long-path-controls-r1") if path.is_dir())
    historical+=sum(((path.stat().st_size+4095)//4096)*4096 for path in E.glob("windows-stock*") if path.is_file())
    consumed=historical+product["allocated"](VIEW)+built["product_allocated_upper_bytes"]+sum(
        product["allocated"](p) for p in E.glob("windows-managed-resource-policy-r*") if p.is_dir())
    allowance=built["remaining_storage"]-consumed
    needed=3*product["allocated"](product["OUT"]/"runtime")
    require(needed+128*1024**2<allowance,"original stock transient inputs cannot fit")
    _,cores=product["quiet_cpus"](None)
    for path in (OUT,OUT/"temp",OUT/"appdata",OUT/"localappdata",VIEW/"artifacts",PACKAGES):
        if not path.exists():
            c["mkdir"](guard,path)
    env=os.environ.copy()
    env.update(runpy.run_path(str(ROOT/"scripts/static-runtime-environment.py"))["load_environment_file"](
        E/"windows-fresh-cpu-ort-r3/cpu-environment.json",E/"q","x86_64-pc-windows-msvc"))
    env.update(TEMP=str(OUT/"temp"),TMP=str(OUT/"temp"),TMPDIR=str(OUT/"temp"),APPDATA=str(OUT/"appdata"),
               LOCALAPPDATA=str(OUT/"localappdata"),PYTHONDONTWRITEBYTECODE="1",CARGO_TARGET_DIR=str(ROOT/"target"),
               AIRK_BUNDLED_MODEL_DIR=str(ROOT/"artifacts/indexing-plan/minilm-fp16-build-inputs-r1"))
    cmd=[sys.executable,"-I","-B","-S",str(script),"--runtime-dir",str(product["OUT"]/"runtime"),
         "--admin-inventory",str(product["ADMIN"]),"--unsigned","--iscc",str(ISCC)]
    saved_support=seal(E/"windows-stock-installer-prospective-private-links-source.txt",
                       b.read(view["path"])["support"]["scripts/build-local-service-installer.py"])
    pins=[closed,view,compiler,stock,saved_support,moved,previous,first_wrapper,paths,b.pin(product["ADMIN"]),b.pin(Path(__file__).resolve())]
    deadline=time.monotonic()+900
    before=product["suspended_owner"](c,b,parent,owner,b.pin(Path(sys.executable)),deadline,cores,6*GIB)
    highwater=0
    observed={}
    def monitor(api,job,info,until):
        nonlocal highwater
        while api.WaitForSingleObject(info.process,250)==258:
            require(time.monotonic()<until,"original installer deadline expired")
            size=product["allocated"](PACKAGES)+product["allocated"](OUT)
            highwater=max(highwater,size)
            product["budget"](size,allowance,shutil.disk_usage(ROOT).free)
            members,_,_=owner.job_snapshot(api,job)
            for pid in members:
                try:
                    proc=psutil.Process(pid)
                    name=proc.name().lower()
                    if name not in ("iscc.exe","islzma.exe","cargo.exe") or pid in observed:
                        continue
                    path=Path(proc.exe())
                    actual=proc.environ()
                    require(all(Path(actual[k])==Path(env[k]) for k in ("TMP","TEMP","TMPDIR","APPDATA","LOCALAPPDATA","CARGO_HOME")),
                            "native child escaped D scratch")
                    if name=="iscc.exe":
                        require(path.samefile(ISCC),"wrong Inno compiler")
                    if name=="cargo.exe":
                        require("metadata" in proc.cmdline() and path.samefile(env["CARGO"]),"unexpected Cargo operation")
                    observed[pid]={"name":name,"image":b.pin(path),"scratch":actual["TEMP"],"cwd":proc.cwd()}
                except psutil.NoSuchProcess:
                    continue
        return {"observed":observed,"highwater":highwater},[]
    log,stdin=OUT/"stock.log",OUT/"stdin"
    c["guard_many"](guard,[log,stdin])
    report={"passed":False,"inputs":pins,"argv":cmd,"installed":False,"signed":False,"profile":"debug",
            "source":built["source_identity"],"actual_stage_allowance":allowance,"stock_three_copy_bytes":needed}
    try:
        with stdin.open("xb"):
            pass
        with stdin.open("rb") as inp,log.open("xb") as sink:
            job=owner.run_windows_owned_training(cmd,VIEW,sink,deadline-time.monotonic(),env,inp,6*GIB,
                                                 before_resume=before,after_resume=monitor,absolute_deadline=deadline)
        report["job"]=job
        require(job["exit_code"]==0 and not job["timed_out"] and not job["owned_job_forced_termination"] and
                job["owned_descendants_settled"] and not job["owned_job_final_member_pids"] and
                job["owned_job_final_active_processes"]==0,"stock packaging or settlement failed")
        require(any(p["name"]=="iscc.exe" for p in observed.values()),"genuine Inno compiler not observed")
        receipt,=PACKAGES.glob("*/receipt.json")
        results=b.read(receipt)
        installer=Path(results["installer"]).resolve(strict=True)
        require(installer.is_relative_to(receipt.parent) and results["result"]=="built" and results["signed"] is False
                and b.pin(installer)["sha256"]==results["installer_sha256"],"stock unsigned installer identity differs")
        for entry in [*pins,*built["files"]]:
            b.pinned(entry)
        report.update(passed=True,installer=b.pin(installer),stock=b.pin(receipt))
    except BaseException as exc:
        report["error"]=repr(exc)
        raise
    finally:
        report.update(highwater=highwater,native=observed,log=b.pin(log))
        c["write_new"](guard,OUT/"execution.json",report)


if __name__=="__main__":
    launch(int(sys.argv[1]))
