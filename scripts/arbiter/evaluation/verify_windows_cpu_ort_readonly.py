# SPDX-License-Identifier: Apache-2.0
"""Execute stock CPU --verify read-only and identify every opened input, rejecting prior private tool-probe directories."""
import argparse
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import runpy
import sys

ROOT = Path(__file__).resolve().parents[3]
E = ROOT/"artifacts/v190/final-plan/exec-20261116T052942Z"
Q = E/"q"
TARGET = "x86_64-pc-windows-msvc"
STOCK = ROOT/"scripts/provision-static-onnx-runtime.py"


def run():
    source = json.loads((Q/"build-receipt.json").read_bytes())
    excluded = []
    for name in ("cmake","ninja"):
        item = Path(source["toolchain"][name]["probe"]["log"])
        if (item.name != "output.log" or item.parent.parent != Q/"logs" or
                not item.parent.name.startswith("probe-"+name+"-") or item.resolve(strict=True) != item):
            raise ValueError("Unexpected historic tool-probe location")
        excluded.append(item.parent)
    opened = set()
    def audit(event,args):
        if event == "open":
            path, mode, flags = args
            if flags & (os.O_CREAT|os.O_WRONLY|os.O_RDWR|os.O_TRUNC|os.O_APPEND):
                raise ValueError("Stock verification requested mutation")
        elif event in ("os.listdir","os.scandir"):
            path = args[0]
        elif event in ("os.mkdir","os.rmdir","os.rename","os.remove","os.chmod","os.link","os.symlink",
                       "subprocess.Popen","os.system","os.startfile"):
            raise ValueError("Stock read-only verifier requested "+event)
        else:
            return
        if not isinstance(path,(str,bytes,os.PathLike)):
            return
        p=Path(os.fsdecode(path)).absolute()
        if any(p==folder or p.is_relative_to(folder) for folder in excluded):
            raise ValueError("Stock verification depends on original private probe evidence: "+str(p))
        if event=="open":
            opened.add(str(p))
    sys.addaudithook(audit)
    stock=runpy.run_path(str(STOCK))
    raw=io.StringIO()
    with contextlib.redirect_stdout(raw):
        if stock["main"](["--target",TARGET,"--destination",str(Q),"--verify"]) != 0:
            raise ValueError("Stock --verify refused")
    roster=sorted(opened)
    return {"result":json.loads(raw.getvalue()),"unique_read_paths":len(roster),
            "read_set_sha256":hashlib.sha256(json.dumps(roster,separators=(",",":")).encode()).hexdigest(),
            "excluded_historic_directories":[str(p) for p in excluded],"opened_excluded":0,"write_requests":0}


if __name__=="__main__":
    argparse.ArgumentParser(description=__doc__).parse_args()
    print(json.dumps(run(),sort_keys=True))
