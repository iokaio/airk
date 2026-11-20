# SPDX-License-Identifier: Apache-2.0
"""Compare the same actual notice file under the long stock-stage and shorter original paths."""
import hashlib
import os
from pathlib import Path
import runpy
import sys
import time

HERE=Path(__file__).resolve().parent
p=runpy.run_path(str(HERE/"windows_stock_cpu_installer.py"))
product,ROOT,E=(p[k] for k in ("product","ROOT","E"))
require=product["require"]
OUT=E/"windows-iscc-long-path-controls-r1"


def main():
    c,b,parent,owner,guard=runpy.run_path(str(HERE/"windows_cpu_ort.py"))["runtime"]()
    native=p["ISCC"]
    require(b.pin(native)["sha256"]=="eb6f4410c8db367a5f74127e8025ad2ccacc0afabbe783959d237df3050f97fb","wrong compiler")
    run=E/"iv/artifacts/local-service-installer/20261120-055958"
    rel=Path("static-onnx-runtime/pytorch_cpuinfo/LICENSE-46faf5e3ee30a27a9083a5346e48af8b1cc4bda5dc4de2bbfd4679fcf6b87f89.txt")
    sources=[run/"layout/notices/dependencies"/rel,product["OUT"]/"runtime/notices"/rel]
    require([len(str(x)) for x in sources]==[261,225] and sources[0].read_bytes()==sources[1].read_bytes(),
            "original physical paths or content differ")
    c["mkdir"](guard,OUT)
    _,cores=product["quiet_cpus"](None)
    rows=[]
    for label,file in zip(("long","near"),sources,strict=True):
        folder=OUT/label
        c["mkdir"](guard,folder)
        inp=folder/"stdin"
        log=folder/"compiler.log"
        c["guard_many"](guard,[inp,log])
        inp.touch(exist_ok=False)
        env=os.environ|{k:str(folder) for k in ("TMP","TEMP","TMPDIR","APPDATA","LOCALAPPDATA")}
        deadline=time.monotonic()+60
        prior=product["suspended_owner"](c,b,parent,owner,b.pin(native),deadline,cores,1<<30)
        args=[str(native),f"/DAirkOutput={folder}",f"/DAirkSource={file}",str(HERE/"windows_iscc_path.iss")]
        with inp.open("rb") as sink,log.open("xb") as output:
            job=owner.run_windows_owned_training(args,ROOT,output,deadline-time.monotonic(),env,sink,1<<30,
                                                  before_resume=prior,absolute_deadline=deadline)
        require(job["owned_descendants_settled"] and job["owned_job_final_active_processes"]==0 and
                not job["owned_job_forced_termination"] and not job["timed_out"],"compiler control did not close")
        files=[path for path in folder.rglob("*") if path.is_file()]
        c["guard_many"](guard,files,True)
        rows.append({"label":label,"argv":args,"path_length":len(str(file)),"original":b.pin(file),
                     "job":job,"log":b.pin(log),"output":[b.pin(x) for x in folder.glob("*.exe")]})
    c["write_new"](guard,OUT/"receipt.json",{"compiler":b.pin(native),"source":b.pin(HERE/"windows_iscc_path.iss"),
        "owner":b.pin(Path(__file__).resolve()),"rows":rows,"short_only_compiles":
        rows[0]["job"]["exit_code"]!=0 and rows[1]["job"]["exit_code"]==0})


if __name__=="__main__":
    main()
