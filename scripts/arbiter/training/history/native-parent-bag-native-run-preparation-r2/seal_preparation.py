# SPDX-License-Identifier: Apache-2.0
"""Retain cheap private preparation checks; no Cargo, model call or ACL mutation."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys
import time

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]

def sha(path):
    with Path(path).open("rb") as stream:return hashlib.file_digest(stream,"sha256").hexdigest()

def run(argv):
    started=time.monotonic()
    actual=subprocess.run(argv,cwd=ROOT,capture_output=True,text=True)
    return {"argv":argv,"exit_code":actual.returncode,"elapsed_seconds":time.monotonic()-started,
            "stdout":actual.stdout,"stderr":actual.stderr}

def main():
    output=HERE/"preparation-review-r2.json"
    if output.exists():raise ValueError("fresh preparation receipt required")
    baseline=json.loads((HERE/"amendment-review.json").read_bytes())["public_source_before"]
    if any(sha(ROOT/name)!=pin for name,pin in baseline.items()):raise ValueError("public source baseline drift")
    checks=[run([sys.executable,"-B","-m","unittest","discover","-s",str(HERE),"-p","test_*.py"]),
            run([sys.executable,"-B",str(HERE/"validate_preparation.py")])]
    if any(row["exit_code"]!=0 for row in checks):raise ValueError("private preparation check failed")
    if "Ran 9 tests" not in checks[0]["stderr"]:raise ValueError("exact nonzero nine cheap checks required")
    files=[{"path":str(path.relative_to(ROOT)),"sha256":sha(path)}
           for path in sorted(HERE.iterdir()) if path.is_file() and path!=output]
    metadata=json.loads((HERE/"metadata-preflight-r1.json").read_bytes())
    actual={"schema":"airk.private-parent-bag-next-native-review/1","files":files,
            "public_source_baseline":baseline,"public_sources_unchanged":True,"checks":checks,
            "metadata":metadata,"PowerShell_bridge_parse":"passed separately via Parser.ParseFile; bridge not executed",
            "public_mount":False,"Cargo":False,"native_or_NN":False,"ACL_mutation":False,"FINAL_read":False,
            "pending":"Root review/thaw/mount, compile+actual Cargo JSON, typed emitter,24native numeric,4DEV64 and matched grade; later acceptance remains separate"}
    output.write_text(json.dumps(actual,indent=2)+"\n")
    print(json.dumps({"receipt":str(output),"sha256":sha(output),"files":len(files),
                      "cheap_tests_selected":9,"passed":True,"public_sources_unchanged":True}))

if __name__=="__main__":main()
