# SPDX-License-Identifier: Apache-2.0

import datetime, hashlib, importlib.util, json, os, pwd, stat, sys, time
from pathlib import Path
assert os.getuid()==os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=="tyler", "Linux effective human identity mismatch"
os.umask(0o077)
base=Path("/home/tyler/airk-builds/arbiter-final-linux-r1")
source=base/"source"
for p in [base,source,base/"tmp",base/"evidence",base/"cargo",base/"rustup",base/"private-census"]:
    m=p.lstat()
    assert stat.S_ISDIR(m.st_mode) and m.st_uid==1000 and m.st_gid==1000 and stat.S_IMODE(m.st_mode)==0o700, "output root policy mismatch "+str(p)
freeze_path=Path("/mnt/d/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z/linux-userspace-provisioning-plan-r1/root-linux-cargo-command-freeze-r1.json")
raw=freeze_path.read_bytes()
assert hashlib.sha256(raw).hexdigest()=="5d6442d42cad45da149d0959b1d2bf3108f1f8d1e49292809ac6ca1f644d34f8","frozen command inputs changed"
freeze=json.loads(raw)
for row in freeze["source_owner_script_pins"]:
    assert hashlib.sha256(Path(row["path"]).read_bytes()).hexdigest()==row["sha256"],"source owner changed"
env=dict(os.environ)
env.update(freeze["root_only_cargo_future_environment"])
env.pop("GIT_PAGER",None)
assert not any(k.startswith("GIT_") for k in env),"unexpected inherited Git override"
assert env["CARGO_INCREMENTAL"]=="0" and env["CARGO_BUILD_JOBS"]=="2"
model=base/"inputs/bundled-model/minilm-l6-v2-u8-v3"
assert model.is_dir() and model.stat().st_uid==1000
env["AIRK_TEST_MODEL_DIR"]=str(model)
out=base/"evidence/root-platform-gates-r1"
assert not out.exists(),"original run exists; no overwrite or unchanged retry"
out.mkdir()
spec=importlib.util.spec_from_file_location("owned_static_runtime",source/"scripts/provision-static-onnx-runtime.py")
owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
started=time.monotonic()
report={"schema":"airk.private.linux-native-platform-batch/1","started_utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"human":{"uid":os.getuid(),"euid":os.geteuid(),"user":pwd.getpwuid(os.geteuid()).pw_name},"source_base":freeze["source_base_commit"],"source_overlay":freeze["source_overlay_bridge"],"source_c7":freeze["overlay_raw_c7"],"freeze_sha256":hashlib.sha256(raw).hexdigest(),"target_directory":env["CARGO_TARGET_DIR"],"incremental":env["CARGO_INCREMENTAL"],"jobs":env["CARGO_BUILD_JOBS"],"native_model_dir":str(model),"phases":[],"passed":False,"scope":"format then whole-workspace all-target all-feature Clippy; no model/platform/runtime qualification inferred"}
def retain():
    (out/"execution.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
retain()
try:
    cargo=str(base/"rustup/toolchains/1.98.0-x86_64-unknown-linux-gnu/bin/cargo")
    commands=[
        ("format",[cargo,"fmt","--all","--","--check"],120),
        ("whole-clippy",[cargo,"clippy","--workspace","--all-targets","--all-features","--locked","--timings","--","-D","warnings"],3600)
    ]
    for name,argv,limit in commands:
        command=["/usr/bin/python3","-I","-B",str(source/"scripts/dev/receipt_runner.py"),"--groups","native,harness,fixtures,frontend","--tier","T3","--out",str(out/"commands"),"--target-dir",str(base/"target"),"--node",str(base/"node/node-v24.15.0-linux-x64/bin/node"),"--static-environment",freeze["static_environment_file"]["path"],"linux-"+name+"-r1","--",*argv]
        entry={"name":name,"argv":command,"timeout_seconds":limit,"started_utc":datetime.datetime.now(datetime.timezone.utc).isoformat()}
        report["phases"].append(entry);retain()
        observed=time.monotonic()
        result=owner.run_owned(command,source,out/(name+"-owned.log"),limit,env)
        entry.update({"owned":result,"elapsed_seconds":time.monotonic()-observed})
        receipt=out/"commands"/("linux-"+name+"-r1.json")
        entry["runner_receipt"]={"path":str(receipt),"sha256":hashlib.sha256(receipt.read_bytes()).hexdigest()} if receipt.exists() else None
        actual=json.loads(receipt.read_bytes()) if receipt.exists() else {}
        entry["passed"]=(result.get("exit_code")==0 and result.get("owned_descendants_settled") is True and result.get("timed_out") is False and result.get("log_oversized") is False and result.get("cleanup_errors")==[] and actual.get("passed") is True)
        retain()
        if not entry["passed"]: raise RuntimeError("Actual "+name+" failed; original diagnostics retained; no following native phase")
    report["passed"]=True
except BaseException as error:
    report["failure"]={"type":type(error).__name__,"message":str(error)}
finally:
    report["elapsed_seconds"]=time.monotonic()-started
    report["ended_utc"]=datetime.datetime.now(datetime.timezone.utc).isoformat()
    retain()
print(json.dumps({"output":str(out),"passed":report["passed"],"failure":report.get("failure")}))
sys.exit(0 if report["passed"] else 1)
