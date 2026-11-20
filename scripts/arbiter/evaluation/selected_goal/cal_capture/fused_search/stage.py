# SPDX-License-Identifier: Apache-2.0
"""Seal the actual rebuilt Windows service against the original scientific inputs."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parent/"timing"))
from run import AUDIT_PATH, AUDIT_SHA, CAMPAIGN, ROOT, SELECTOR, link, linked, native, pin, require

OUTPUT=CAMPAIGN/"cal100-fused-candidate-launch-r1"


def stage(build,build_sha):
    require(OUTPUT.is_dir() and not any(OUTPUT.iterdir()) and build.name=="fused-service-image.json" and
            build.parent.parent==CAMPAIGN/"cal100-store-candidate-timing-r1","unexpected source or output")
    successful=linked(pin(build,build_sha))
    require(successful["exit_code"]==0 and successful["inputs_unchanged"] is True,"build changed or failed")
    log=build.with_suffix(".log")
    images=[]
    for line in log.read_bytes().splitlines():
        if not line.startswith(b"{"):
            continue
        row=json.loads(line)
        if row.get("reason")=="compiler-artifact" and row["target"]["name"]=="airk_service" and row.get("executable") and row["profile"].get("test"):
            images.append(Path(row["executable"]))
    require(len(images)==1,"expected one sealed native service libtest image")
    old=linked(pin(AUDIT_PATH,AUDIT_SHA))
    src=images[0]
    require(src.is_file() and src.is_relative_to(ROOT/"target") and src.suffix==".exe","unexpected built PE location")
    dst=OUTPUT/"airk-service-fused-search.exe"
    shutil.copyfile(src,dst)
    image=link(dst)
    require(image["sha256"]==link(src)["sha256"] and image["sha256"]!=old["inputs"]["service"]["sha256"],"service stage differs")
    env=os.environ.copy()
    env["PATH"]=str(Path(old["inputs"]["service"]["path"]).parent)+os.pathsep+env["PATH"]
    cmd=[str(dst),"--list","--exact",SELECTOR]
    out=subprocess.run(cmd,cwd=ROOT,env=env,stdin=subprocess.DEVNULL,capture_output=True,timeout=45,check=False)
    (OUTPUT/"selector.stdout").write_bytes(out.stdout)
    (OUTPUT/"selector.stderr").write_bytes(out.stderr)
    require(out.returncode==0 and out.stdout.decode().splitlines().count(SELECTOR+": test")==1,"native selector unavailable")
    old["inputs"]["service"]=image
    old["selector_check"]=cmd
    native.write_json(OUTPUT/"cal100-launch-audit.json",old)
    native.write_json(OUTPUT/"provenance.json",{"schema":"airk.private.cal-fused-service-source/1","historical":pin(AUDIT_PATH,AUDIT_SHA),"audit":link(OUTPUT/"cal100-launch-audit.json"),
                      "image":image,"compiler_image":link(src),"build":link(build),"compiler_log":link(log),
                      "selector":link(OUTPUT/"selector.stdout"),"cargo_scope":successful["command"],
                      "active_data_schema":"identical original 100 CAL requests","qualified":False})


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("build",type=Path)
    parser.add_argument("sha")
    args=parser.parse_args()
    stage(args.build,args.sha)
