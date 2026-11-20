# SPDX-License-Identifier: Apache-2.0
"""Build and run the standalone model-free read comparison under a checked D-only Cargo home."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import time

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parent/"timing"))
from run import CAMPAIGN, ROOT, link, linked, native, owned_module, pin, require

WORK=CAMPAIGN/"cal100-store-candidate-timing-r1"
ROSTER="a18055c2d1da54c3b5bcde22fc512a1ec8cf4ced93dc4ea40c7fc669ed2c2d4a"
EXAMPLE="observe_graph_candidate_reads"
TARGET=ROOT/"target"
TRIPLE="x86_64-pc-windows-msvc"


def file_sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream,"sha256").hexdigest()


def checked_clone():
    roster=linked(pin(WORK/"immutable-source-roster.json",ROSTER))
    root=Path(roster["clone"])
    require(root==WORK/"index" and len(roster["files"])==2286,"foreign snapshot")
    for row in roster["files"]:
        path=root/row["relative"]
        require(path.stat().st_size==row["bytes"] and file_sha(path)==row["sha256"].lower(),"copied source changed")
    return root/"collections-1.4/cal100"


def run_worker(out):
    n=runpy.run_path(str(ROOT/"scripts/arbiter/analysis/public_source_admission/aws_normal_service.py"))
    with n["capped"]({"write_json":native.write_json},out,4<<30):
        clone=checked_clone()
        cargo=Path(os.environ["CARGO"])
        require(Path(os.environ["CARGO_HOME"])==CAMPAIGN/"windows-offline-cargo-r1/home" and
                os.environ["CARGO_BUILD_JOBS"]=="1" and os.environ["CARGO_INCREMENTAL"]=="1","foreign Cargo environment")
        runner=ROOT/"scripts/dev/receipt_runner.py"
        for part in ("clippy","build"):
            args=[cargo,part,"--locked","--offline","--timings","--target",TRIPLE,"--no-default-features",
                  "-p","airk-store","--example",EXAMPLE]
            args.extend(["--","-D","warnings"] if part=="clippy" else ["--message-format=json-render-diagnostics"])
            command=[sys.executable,"-I","-B","-S",runner,"--groups","native","--out",out,"--target-dir",TARGET,part,"--",*args]
            result=subprocess.run(command,cwd=ROOT,timeout=600,check=False)
            require(result.returncode==0,part+" failed; original receipt retained")
        original=TARGET/TRIPLE/"debug/examples"/(EXAMPLE+".exe")
        exe=out/(EXAMPLE+".exe")
        shutil.copyfile(original,exe)
        require(file_sha(exe)==file_sha(original),"image staging differs")
        old=CAMPAIGN/"cal100-focus1024-timing-ab-r2"
        binding=linked(pin(old/"fixture/enable-context.bindings.json",
                          linked(pin(old.with_name(old.name+"-postclosure-analysis-r1.json"),
                                     "d2bb6cba0e02213b8040409435ab2e05c229bf0f4a4e2092320be54b00873ab5"))["native"]["bindings"]["sha256"]))["bindings"][0]
        pub=binding["publication"]
        argv=[exe,clone,binding["definition"]["backing"]["root"],pub["incarnation"],str(pub["generation"]),pub["revision"]]
        with (out/"observed.jsonl").open("xb") as log,(out/"diagnostic.stderr").open("xb") as err:
            start=time.perf_counter()
            result=subprocess.run(argv,stdout=log,stderr=err,timeout=120,check=False)
        native.write_json(out/"probe.json",{"command":[str(v) for v in argv],"image":link(exe),"seconds":time.perf_counter()-start,
                                         "exit_code":result.returncode,"log":link(out/"observed.jsonl")})
        require(result.returncode==0,"native model-free observation failed")
        rows=[json.loads(l) for l in (out/"observed.jsonl").read_bytes().splitlines()]
        require([(r["width"],r["trial"],r["one_binding"]) for r in rows]==
                [(w,i,f) for w in (64,400) for i,f in enumerate((False,True,True,False))],"incomplete native trials")
        for width in (64,400):
            trials=[r for r in rows if r["width"]==width]
            require(len({r["stream_sha256"] for r in trials})==1 and all(r["pointer_count"]>=width for r in trials),
                    "mismatched pointer sequences")


if __name__=="__main__":
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument("out",type=Path)
    ap.add_argument("--worker",action="store_true")
    a=ap.parse_args()
    require(a.out.parent==WORK and a.out.name.startswith("native-r") and a.out.resolve(strict=True)==a.out,"foreign output")
    if a.worker:
        run_worker(a.out)
    else:
        require(not any(a.out.iterdir()),"fresh native output required")
        result=owned_module().run_evaluation_fixture([sys.executable,"-I","-B","-S",Path(__file__).resolve(),a.out,"--worker"],
                                                     ROOT,a.out/"outer.log",900,os.environ)
        native.write_json(a.out/"owned.json",result)
        require(owned_module().successful(result),"native source or observation did not complete")
