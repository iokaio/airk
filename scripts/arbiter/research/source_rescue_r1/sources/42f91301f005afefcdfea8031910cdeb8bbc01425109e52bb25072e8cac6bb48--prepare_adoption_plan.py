# SPDX-License-Identifier: Apache-2.0
"""Pin only actual successful driver outputs after Root approves and runs phase one."""
import argparse,hashlib,json
from pathlib import Path

def pin(path):
    if path.is_symlink() or not path.is_file():raise ValueError("Missing/redirected pin: "+str(path))
    return {"path":str(path.resolve(strict=True)),"sha256":hashlib.sha256(path.read_bytes()).hexdigest(),"bytes":path.stat().st_size}

def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument("--local-run",type=Path,required=True);ap.add_argument("--driver-receipt",type=Path,required=True);args=ap.parse_args()
    here=Path(__file__).resolve().parent;template=here/"adoption/plan.template.json";plan=json.loads(template.read_bytes());candidate=Path(plan["candidate_root"]);run=args.local_run.resolve(strict=True)
    if run.parent != candidate/"artifacts/local-service-installer":raise ValueError("Expected the single fresh candidate-local run")
    driver=json.loads(args.driver_receipt.read_bytes());local=json.loads((run/"receipt.json").read_bytes())
    for key in ("passed","inputs_unchanged","runner_unchanged","runtime_unchanged","command_inputs_unchanged"):
        if driver.get(key) is not True:raise ValueError("Driver evidence refused: "+key)
    if driver.get("evidence_errors") != [] or driver.get("exit_code") != 0 or driver.get("runner_exit_code") != 0:raise ValueError("Driver evidence is incomplete")
    command=driver["command"]
    if Path(command[2]).resolve()!=candidate/"scripts/build-local-service-installer.py":raise ValueError("Driver command owner differs")
    for flag,want in (("--runtime-dir",plan["source_runtime"]),("--admin-inventory",plan["pins"]["admin_inventory"]["path"])):
        if Path(command[command.index(flag)+1]).resolve()!=Path(want):raise ValueError("Driver consumed another input")
    if local.get("result")!="built" or local.get("signed") is not True or local.get("dirty") is not False or local.get("commit")!=plan["commit"]:raise ValueError("No successful clean-source signed installer")
    plan["local_run"]=str(run);plan["pins"]["local_receipt"]=pin(run/"receipt.json");plan["pins"]["local_driver_receipt"]=pin(args.driver_receipt)
    for name in ("airk","airkctl","installer","uninstaller"):plan["pins"]["signing_"+name]=pin(run/("signing-"+name+".json"))
    for value in plan["pins"].values():
        actual=pin(Path(value["path"]))
        if actual!=value:raise ValueError("Input changed before adoption: "+value["path"])
    destination=here/"adoption/plan.json";raw=(json.dumps(plan,indent=2)+"\n").encode("utf8")
    with destination.open("xb") as f:f.write(raw)
    print(json.dumps({"plan":str(destination),"sha256":hashlib.sha256(raw).hexdigest(),"local_run":str(run)}))

if __name__=="__main__":main()
