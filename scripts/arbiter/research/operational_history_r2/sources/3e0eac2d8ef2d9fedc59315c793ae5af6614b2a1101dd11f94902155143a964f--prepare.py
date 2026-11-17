# SPDX-License-Identifier: Apache-2.0
"""Prepare current public install commands; never invokes a product or installer."""
from pathlib import Path,PureWindowsPath,PurePosixPath
import argparse, hashlib, json

TARGETS={"windows":"x86_64-pc-windows-msvc","linux":"x86_64-unknown-linux-gnu","mac-arm64":"aarch64-apple-darwin","mac-intel":"x86_64-apple-darwin"}
PROTECTED_IDENTITIES={"S-1-5-21-3678370870-3858642417-826968105-1002"}

def recipe(platform,mode,payload,root,identity,hostname):
    if platform not in TARGETS or mode not in {"portable","installer"}:raise ValueError("supported target/mode required")
    if not identity or identity in PROTECTED_IDENTITIES or identity=="0":raise ValueError("dedicated clean OS identity required; the user account is protected")
    if not hostname:raise ValueError("exact dedicated hostname required")
    path_class=PureWindowsPath if platform=="windows" else PurePosixPath
    payload,root=path_class(payload),path_class(root)
    if not payload.is_absolute() or not root.is_absolute():raise ValueError("absolute payload and fresh evidence root required")
    if payload==root or root in payload.parents or payload in root.parents:raise ValueError("payload and evidence roots must be disjoint")
    windows=platform=="windows";mac=platform.startswith("mac-")
    program=root/"program";data=root/"data-location";cache=root/"cache-location"
    if mode=="portable":
        program=payload/"runtime"
        ctl=program/("AIRK.app/Contents/MacOS/airkctl" if mac else "airkctl.exe" if windows else "airkctl")
        install=[str(ctl),"service","setup","--portable","--program",str(program),"--data",str(root/"indexes"),"--state",str(root/"state"),"--cache",str(root/"cache"),"--logs",str(root/"logs")]
    elif windows:
        ctl=program/"bin/airkctl.exe"
        install=[str(payload),"--silent","/DIR="+str(program),"/DATA="+str(data),"/CACHE="+str(cache),"/LOG="+str(root/"installer.log")]
    else:
        ctl=program/("AIRK.app/Contents/MacOS/airkctl" if mac else "bin/airkctl")
        install=[str(payload/("Install AIRK.command" if mac else "install.sh")),"--silent","--program-dir",str(program),"--data-dir",str(root/"indexes"),"--state-dir",str(root/"state"),"--cache-dir",str(root/"cache"),"--logs-dir",str(root/"logs")]
    commands=[{"id":"install","argv":install,"may_mutate":True}]
    if mode=="portable":commands.append({"id":"start","argv":[str(ctl),"service","start"],"may_mutate":True})
    commands.extend({"id":name,"argv":[str(ctl),"service",name],"may_mutate":False} for name in ["setup-receipt","status","health"])
    commands.extend([{"id":"stop","argv":[str(ctl),"service","stop"],"may_mutate":True},
        {"id":"remove","argv":[str(ctl),"service","uninstall","--yes","--output","json"],"may_mutate":True}])
    return {"schema":"airk.private-current-public-install-recipe/1","preparation_only":True,"platform":platform,"target":TARGETS[platform],"mode":mode,
      "dedicated_host":{"identity":identity,"hostname":hostname},"payload":str(payload),"evidence_root":str(root),"program":str(program),"commands":commands,
      "execution_release_required":True,"qualified":False,"uninstall_exit_zero_is_completion":False,
      "required_preconditions":["OS-native identity and hostname exactly match; inherited HOME/LOCALAPPDATA never establish isolation", "No AIRK fixed install pointer/active setup custody, predecessor roots, registration, PATH entry or service exists for this OS user", "No source workspace or unrelated user service is assigned to the test OS identity", "Complete verified signed/notarized payload and exact bytes are frozen; fresh evidence roots are private, canonical and disjoint", "Expected platform manager and interactive consent capabilities are observed, not inferred"],
      "process_contract":{"stdin":"DEVNULL","shell":False,"timeout_seconds":600,"on_timeout":"retain PID/logs and UNKNOWN; do not kill admitted native installer or automatically retry"},
      "cleanup_contract":["Public service stop must prove exact owned process exit before removal", "Uninstall launches an asynchronous owner: retain full native operation, receipt, recovery_program and process identity", "Observe exact original native terminal removal receipt and wrapper PATH/registration cleanup; pending/unknown is not success", "Preserve keep-data bytes plus source sentinel and unrelated registrations; do not recursively delete roots to manufacture cleanup", "On any failure stop the batch and retain logs/receipts/all fixture roots; no new install/replay"],
      "acceptance_still_required":["actual closed-stdin/no-TTY execution on each target", "offline semantic indexing/search and browser Admin", "interactive install and consent separately", "interruption/original-operation resume and keep-data reinstall", "default locations/login policy separate from explicit-root run"]}

def seal(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument("--platform",choices=TARGETS,required=True);parser.add_argument("--mode",choices=["portable","installer"],required=True)
    parser.add_argument("--payload",required=True);parser.add_argument("--evidence-root",required=True);parser.add_argument("--identity",required=True);parser.add_argument("--hostname",required=True);parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args();result=recipe(args.platform,args.mode,args.payload,args.evidence_root,args.identity,args.hostname)
    if args.output.exists():raise ValueError("fresh preparation output required")
    args.output.write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8");print(json.dumps({"path":str(args.output),"sha256":seal(args.output),"execution_started":False}))
if __name__=="__main__":main()
