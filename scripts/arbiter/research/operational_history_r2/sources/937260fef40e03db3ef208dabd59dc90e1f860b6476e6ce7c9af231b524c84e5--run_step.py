# SPDX-License-Identifier: Apache-2.0
"""Execute one explicitly reviewed recipe step on a dedicated clean OS account.
The driver retains raw output. It never declares installed-product qualification.
"""
from pathlib import Path
import argparse,csv,ctypes,hashlib,json,os,platform,socket,subprocess,time,uuid
HERE=Path(__file__).resolve().parent

def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def known_folder(identifier):
    data=uuid.UUID(identifier).bytes_le
    class Guid(ctypes.Structure):_fields_=[("data",ctypes.c_ubyte*16)]
    guid=Guid();guid.data[:]=data;out=ctypes.c_wchar_p()
    result=ctypes.windll.shell32.SHGetKnownFolderPath(ctypes.byref(guid),0x4000,None,ctypes.byref(out))
    if result<0:raise ValueError("OS KnownFolder unavailable")
    try:return Path(out.value)
    finally:ctypes.windll.ole32.CoTaskMemFree(ctypes.cast(out,ctypes.c_void_p))
def native_registration_conflicts(system_directory=None):
    if os.name=="nt":
        import winreg
        conflicts=[]
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER,r"Software\Microsoft\Windows\CurrentVersion\Uninstall") as uninstall:
            for index in range(winreg.QueryInfoKey(uninstall)[0]):
                name=winreg.EnumKey(uninstall,index)
                with winreg.OpenKey(uninstall,name) as entry:
                    try:display=winreg.QueryValueEx(entry,"DisplayName")[0]
                    except FileNotFoundError:continue
                    if "airk" in str(display).casefold():conflicts.append("uninstall:"+name)
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER,"Environment") as environment:
            try:value=winreg.QueryValueEx(environment,"Path")[0]
            except FileNotFoundError:value=""
            if any("airk" in part.casefold() for part in value.split(";")):conflicts.append("user PATH AIRK entry")
        tasks=subprocess.check_output([str(Path(system_directory)/"schtasks.exe"),"/query","/fo","csv","/nh"],text=True,timeout=30)
        if "airksvc-" in tasks.casefold():conflicts.append("AIRK scheduled task")
        return conflicts
    import pwd
    home=Path(pwd.getpwuid(os.geteuid()).pw_dir)
    candidates=[home/".local/bin/airkctl"]
    for folder in [home/".config/systemd/user",home/"Library/LaunchAgents"]:
        if folder.exists():candidates.extend(p for p in folder.iterdir() if "airk" in p.name.casefold())
    return [str(p) for p in candidates if p.exists() or p.is_symlink()]
def native_identity():
    if os.name=="nt":
        buffer=ctypes.create_unicode_buffer(32768)
        if not ctypes.windll.kernel32.GetSystemDirectoryW(buffer,len(buffer)):raise ValueError("system folder unavailable")
        raw=subprocess.check_output([str(Path(buffer.value)/"whoami.exe"),"/user","/fo","csv","/nh"],text=True,timeout=10)
        identity=list(csv.reader(raw.splitlines()))[0][1]
        conflicts=native_registration_conflicts(buffer.value)
        home=known_folder("5E6C858F-0E22-4760-9AFE-EA3317B67173");local=known_folder("F1B32785-6FBA-4FCF-9D55-7B8E7F157091")
        roots=[local/"AIRK",local/"AIRK Service",local/"Programs/AIRK",home/".airk"]
    else:
        import pwd
        entry=pwd.getpwuid(os.geteuid());identity=str(entry.pw_uid);home=Path(entry.pw_dir)
        conflicts=native_registration_conflicts()
        roots=([home/"Library/Application Support/AIRK",home/"Library/Caches/AIRK",home/"Library/Logs/AIRK"] if platform.system()=="Darwin" else [home/".config/airk",home/".local/share/airk",home/".local/state/airk",home/".cache/airk",home/".local/lib/airk-service"])+[home/".airk"]
    return {"identity":identity,"hostname":socket.gethostname(),"home":str(home),"predecessor_roots":[str(p) for p in roots],"existing_predecessor_roots":[str(p) for p in roots if p.exists() or p.is_symlink()],"registration_conflicts":conflicts}
def validate_host(recipe,observed,first):
    if observed["identity"]!=recipe["dedicated_host"]["identity"] or observed["hostname"]!=recipe["dedicated_host"]["hostname"]:raise ValueError("exact dedicated OS host/account mismatch")
    if observed["identity"] in {"0","S-1-5-21-3678370870-3858642417-826968105-1002"}:raise ValueError("user installation account is protected")
    if first and (observed["existing_predecessor_roots"] or observed["registration_conflicts"]):raise ValueError("existing per-user AIRK state is preserved; five explicit roots do not isolate its pointer")
def validate_steps(recipe,step_id,read_receipt):
    positions=[i for i,row in enumerate(recipe["commands"]) if row["id"]==step_id]
    if len(positions)!=1:raise ValueError("exact reviewed step required")
    index=positions[0]
    for previous in recipe["commands"][:index]:
        receipt=read_receipt(previous["id"])
        if receipt["exit_code"]!=0 or receipt["timed_out"] or receipt["recipe_sha256"]!=recipe["recipe_sha256"]:raise ValueError("earlier step unresolved or has foreign identity; no replay")
    return recipe["commands"][index]
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument("--recipe",type=Path,required=True);parser.add_argument("--recipe-sha256",required=True)
    parser.add_argument("--reviewed-artifacts",type=Path,required=True);parser.add_argument("--reviewed-artifacts-sha256",required=True);parser.add_argument("--step",required=True);parser.add_argument("--execute-reviewed",action="store_true")
    args=parser.parse_args()
    if sha(args.recipe)!=args.recipe_sha256 or sha(args.reviewed_artifacts)!=args.reviewed_artifacts_sha256:raise ValueError("reviewed packet changed")
    recipe=json.loads(args.recipe.read_bytes());recipe["recipe_sha256"]=args.recipe_sha256
    artifacts=json.loads(args.reviewed_artifacts.read_bytes())
    if artifacts.get("dedicated_host_reviewed") is not True or artifacts.get("signed_or_integrity_payload_verified") is not True or not artifacts.get("files"):raise ValueError("root-reviewed dedicated-host/artifact proof required")
    for name,digest in artifacts["files"].items():
        if sha(Path(name))!=digest:raise ValueError("frozen payload/source input changed")
    root=Path(recipe["evidence_root"])
    read=lambda name:json.loads((root/(name+".process.json")).read_bytes())
    step=validate_steps(recipe,args.step,read)
    for previous in recipe["commands"][:recipe["commands"].index(step)]:
        approval=artifacts.get("approved_prior_outcomes",{}).get(previous["id"],{})
        prior_path=root/(previous["id"]+".process.json")
        if approval.get("process_receipt_sha256")!=sha(prior_path) or approval.get("native_outcome_reviewed") is not True:raise ValueError("root review of exact original native outcome required before the next step")
    expected_platform={"windows":("Windows",{"amd64","x86_64"}),"linux":("Linux",{"x86_64","amd64"}),"mac-arm64":("Darwin",{"arm64","aarch64"}),"mac-intel":("Darwin",{"x86_64","amd64"})}[recipe["platform"]]
    if platform.system()!=expected_platform[0] or platform.machine().casefold() not in expected_platform[1]:raise ValueError("native target host does not match the recipe")
    observed=native_identity();validate_host(recipe,observed,args.step=="install")
    if not args.execute_reviewed:
        print(json.dumps({"preflight_passed":True,"step":step,"host":observed,"execution_started":False}));return
    if args.step=="install":
        if root.exists():raise ValueError("fresh private evidence root required")
        root.mkdir(mode=0o700)
    if not root.is_dir() or root.is_symlink():raise ValueError("original private evidence root required")
    receipt=root/(args.step+".process.json")
    if receipt.exists():raise ValueError("no overwrite or replay of a previous step")
    request={"step":step,"recipe_sha256":args.recipe_sha256,"reviewed_artifacts_sha256":args.reviewed_artifacts_sha256,"host":observed,"stdin_closed":True,"shell":False}
    (root/(args.step+".request.json")).write_text(json.dumps(request,indent=2)+"\n")
    started=time.monotonic();timed_out=False
    with (root/(args.step+".stdout.raw")).open("xb") as output,(root/(args.step+".stderr.raw")).open("xb") as error:
        process=subprocess.Popen(step["argv"],stdin=subprocess.DEVNULL,stdout=output,stderr=error,shell=False,creationflags=getattr(subprocess,"CREATE_NO_WINDOW",0))
        try:code=process.wait(timeout=recipe["process_contract"]["timeout_seconds"])
        except subprocess.TimeoutExpired:code=None;timed_out=True
    result={"recipe_sha256":args.recipe_sha256,"step":args.step,"pid":process.pid,"exit_code":code,"timed_out":timed_out,"elapsed_seconds":time.monotonic()-started,
      "process_exit_observed":code is not None,"forced_termination":False,"qualified":False,"native_removal_completion":False,
      "outcome":"UNKNOWN admitted work may still run; no kill or automatic replay" if timed_out else "process observation only; inspect exact native result/receipt"}
    receipt.write_text(json.dumps(result,indent=2)+"\n");print(json.dumps(result))
    if timed_out or code!=0:raise SystemExit(1)
if __name__=="__main__":main()
