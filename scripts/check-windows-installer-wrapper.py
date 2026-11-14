# SPDX-License-Identifier: Apache-2.0
"""Compile and execute actual Inno events with an inert payload and isolated roots.
No native AIRK state, PATH, icons, uninstall registration or network is touched.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import uuid
import time
import re

ROOT = Path(__file__).resolve().parent.parent
RUNS = []
SOURCE = ROOT / "installers/windows/airk-service.iss"
CSC = Path("C:/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe")
ISCC = Path.home() / "AppData/Local/Programs/Inno Setup 6/ISCC.exe"

STUB = r'''
using System;
using System.IO;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Web.Script.Serialization;
class Stub {
 [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
 [DllImport("ntdll.dll")] static extern int NtQueryInformationProcess(IntPtr p,int c,IntPtr b,int n,out int r);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern IntPtr CreateMutex(IntPtr a,bool o,string n);
 [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(int a,bool i,int p);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool DuplicateHandle(IntPtr a,IntPtr b,IntPtr c,out IntPtr d,int e,bool f,int g);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 static string Lease() {
  IntPtr data=Marshal.AllocHGlobal(IntPtr.Size*6);int length;
  if(NtQueryInformationProcess(GetCurrentProcess(),0,data,IntPtr.Size*6,out length)!=0) throw new Exception("inert ancestry unavailable");
  int parent=Marshal.ReadIntPtr(data,IntPtr.Size*5).ToInt32();Marshal.FreeHGlobal(data);
  IntPtr target=OpenProcess(0x40,false,parent),mutex=CreateMutex(IntPtr.Zero,false,"Local\\MUTEX_NAME"),result;
  if(!DuplicateHandle(GetCurrentProcess(),mutex,target,out result,0,false,2)) throw new Exception("inert mutex duplication failed");
  CloseHandle(target);CloseHandle(mutex);return result.ToInt64().ToString();
 }
 static string AuthorityFolder(Dictionary<string,object> value,string fallback) {
  string key=(string)value["operation_id"];
  foreach(string candidate in Directory.GetFiles(FIXTURE_ROOT,"*.json",SearchOption.AllDirectories)) {
   if(Path.GetFileName(Path.GetDirectoryName(candidate))=="recovery-projection") continue;
   try {
    var other=new JavaScriptSerializer().Deserialize<Dictionary<string,object>>(File.ReadAllText(candidate));
    if(other!=null && other.ContainsKey("operation_id") && (string)other["operation_id"]==key) return Path.GetDirectoryName(candidate);
   } catch { }
  }
  return fallback;
 }
 static int Main(string[] args) {
  if (args.Length < 4 || args[0] != "__airk_role") return 90;
  string op=args[3], folder=Path.GetDirectoryName(op);
  if (Array.IndexOf(args,"--recovery-field")>=0) {
   string field=args[Array.IndexOf(args,"--recovery-field")+1];
   var value=new JavaScriptSerializer().Deserialize<Dictionary<string,object>>(File.ReadAllText(op));
   string authority=AuthorityFolder(value,folder);
   string projected;
   if(field=="wrapper-lock-handle") projected=Lease();
   else if(field=="completion-required") projected=File.Exists(Path.Combine(authority,"native-completed"))?"false":"true";
   else if(field=="placement-required") projected=(File.Exists(Path.Combine(authority,"native-completed")) || File.Exists(Path.Combine(authority,"native-placed")))?"false":"true";
   else if(field=="operation-id") projected=(string)value["operation_id"];
   else if(field=="requested-by") projected=(string)value["requested_by"];
   else if(field=="inventory-sha256") projected=(string)value["payload_inventory_sha256"];
   else if(field=="trust") projected=(string)((Dictionary<string,object>)value["options"])["trust"];
   else if(field=="kind") projected=(string)value["kind"];
   else projected=(string)((Dictionary<string,object>)value["locations"])[field];
   File.WriteAllText(Path.Combine(folder,"recovery-"+field+".txt"),projected,new UTF8Encoding(false));
   Console.Write(projected); return 0;
  }
  File.AppendAllText(Path.Combine(folder,"phase-events.txt"),String.Join(" ",args)+"\n");
  File.Copy(op, Path.Combine(folder,"observed-operation.json"), true);
  string phase=Array.IndexOf(args,"--seal-inno")>=0?"seal":Array.IndexOf(args,"--prepare")>=0?"prepare":Array.IndexOf(args,"--plan")>=0?"plan":"complete";
  File.Copy(op, Path.Combine(folder,"observed-"+phase+".json"), true);
  if (Array.IndexOf(args,"--plan")>=0) {
   File.WriteAllText(Path.Combine(folder,"plan.json"),"{\"location_problems\":[],\"blockers\":[]}",new UTF8Encoding(false));
   File.WriteAllText(Path.Combine(folder,"plan.txt"),"Isolated inert fixture"); return 0;
  }
  if (Array.IndexOf(args,"--prepare")>=0) return 0;
  if (Array.IndexOf(args,"--seal-inno")>=0) return SEAL_EXIT;
  if (MARK_PLACED) File.WriteAllText(Path.Combine(folder,"native-placed"),"placed");
  File.WriteAllText(Path.Combine(folder,"error.txt"),"inert finalization refusal",new UTF8Encoding(false));
  if (FINAL_EXIT==0 || File.Exists(Path.Combine(folder,"resume-allowed"))) {
   File.WriteAllText(Path.Combine(folder,"native-completed"),"complete"); return 0;
  }
  return FINAL_EXIT;
 }
}
'''

def run(command, **kwargs):
    arguments=[str(x) for x in command]
    started=time.perf_counter()
    try:
        result=subprocess.run(arguments,stdin=subprocess.DEVNULL,capture_output=True,timeout=120,**kwargs)
    except subprocess.TimeoutExpired:
        RUNS.append({"command":arguments,"elapsed_seconds":time.perf_counter()-started,"timed_out":True})
        raise
    RUNS.append({"command":arguments,"elapsed_seconds":time.perf_counter()-started,"exit_code":result.returncode})
    return result


def replace_procedure(source, name, body):
    start = source.index("procedure " + name + ";")
    end = source.index("\nend;", start) + len("\nend;")
    return source[:start] + "procedure " + name + ";\nbegin\n" + body + "\nend;" + source[end:]


def scenario(root, name, native_exit, path_failure, source_path=None):
    source_path=source_path or SOURCE
    source_bytes=source_path.read_bytes()
    work = root / name
    work.mkdir()
    payload = work / "payload"
    for item in ("service", "bin", "notices"):
        (payload / item).mkdir(parents=True, exist_ok=True)
    code = work / "stub.cs"
    code.write_text(STUB.replace("FIXTURE_ROOT", json.dumps(str(work / "private"))).replace("MARK_PLACED", "true" if name=="resume-native-completion" else "false").replace("MUTEX_NAME", "AIRK.InertWrapper."+uuid.uuid4().hex).replace("FINAL_EXIT", str(native_exit)).replace("SEAL_EXIT", "8" if name=="seal-failure" else "0"), encoding="utf-8")
    result = run([CSC, "/nologo", "/r:System.Web.Extensions.dll", "/target:exe", "/out:" + str(payload / "service/airk.exe"), code])
    if result.returncode: raise RuntimeError(result.stdout.decode(errors="replace"))
    (payload / "bin/airkctl.exe").write_bytes((payload / "service/airk.exe").read_bytes())
    (payload / "notices/LICENSE").write_text("Inert test fixture", encoding="utf-8")
    (payload / "payload-inventory.json").write_text("{}", encoding="utf-8")
    source = source_bytes.decode("utf-8").replace("\r\n", "\n")
    source = source.replace("1A421FBD-1C33-47AD-9770-AE490F2737CF", str(uuid.uuid4()).upper())
    source = source.replace("[Setup]", "[Setup]\nUninstallable="+("yes" if name in ("uninstall-handoff","uninstall-resume") else "no")+"\nCreateUninstallRegKey=no")
    source = source[:source.index("[Icons]")] + source[source.index("[Code]"):]
    source = source.replace("{localappdata}", str(work / "private"))
    (work / "private").mkdir()
    source = replace_procedure(source, "AddOwnedUserPath", "  if not FileExists(ExtractFileDir(SetupOperation) + '\\resume-allowed') then RaiseException('inert PATH refusal');" if path_failure else "  Log('fixture: PATH untouched');")
    source = replace_procedure(source, "RemoveOwnedUserPath", "  if not FileExists(ExtractFileDir(ExpandConstant('{param:AIRKOPERATION|}')) + '\\wrapper-removal-allowed') then RaiseException('inert removal PATH refusal');" if name=="uninstall-resume" else "  Log('fixture: PATH untouched');")
    if name in ("uninstall-handoff","uninstall-resume"):
        destination=str(work).replace("'","''")
        probe=("procedure CurStepChanged(CurStep: TSetupStep);\nvar\n  PairExe, PairData, PairEvent: String;\nbegin\n"
               "  if (CurStep = ssPostInstall) or (CurStep = ssDone) then begin\n"
               "    PairExe := ExpandConstant('{uninstallexe}');\n"
               "    PairData := ChangeFileExt(PairExe, '.dat');\n"
               "    if CurStep = ssPostInstall then PairEvent := 'post' else PairEvent := 'done';\n"
               "    if not SaveStringToFile('"+destination+"\\pair-' + PairEvent + '.txt',\n"
               "      GetSHA256OfFile(PairExe) + #10 + GetSHA256OfFile(PairData) + #10, False) then\n"
               "      RaiseException('Cannot capture pair finalization proof.');\n"
               "  end;\n")
        source=source.replace("procedure CurStepChanged(CurStep: TSetupStep);\nbegin\n",probe)
    script = work / "fixture.iss"
    script.write_text(source, encoding="utf-8-sig")
    digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    command = [ISCC, "/Qp", "/DAirkVersion=1.8.35", "/DAirkPayload="+str(payload),
               "/DAirkOutput="+str(work), "/DAirkArtifact=fixture",
               "/DAirkInventorySHA="+digest(payload / "payload-inventory.json"),
               "/DAirkBinarySHA="+digest(payload / "service/airk.exe"),
               "/DAirkCtlBinarySHA="+digest(payload / "bin/airkctl.exe"), script]
    result = run(command)
    (work / "compile.log").write_bytes(result.stdout + result.stderr)
    if result.returncode: raise RuntimeError((work / "compile.log").read_text(errors="replace"))
    program = work / "program \u00e9 \u65e5\u672c\u8a9e \U0001f431"
    command = [work / "fixture.exe", "--silent", "/LOG="+str(work / "relay.log"), "/DIR="+str(program),
               "/DATA="+str(work / "data \u00e9"), "/CACHE="+str(work / "cache \u65e5\u672c\u8a9e"), "/LOG="+str(work / "setup.log")]
    result = run(command)
    expected = 11 if native_exit or path_failure or name=="seal-failure" else 0
    if result.returncode != expected:
        raise AssertionError(f"{name}: exit {result.returncode}, expected {expected}; {work / 'setup.log'}")
    requests = list((work / "private").rglob("operation-request.json"))
    if not requests: raise AssertionError("recovery requests missing")
    initial_request_bytes={p:p.read_bytes() for p in requests}
    for request in requests:
        raw = request.read_bytes()
        value = json.loads(raw)
        canonical = (json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n").encode()
        if raw != canonical: raise AssertionError("noncanonical UTF-8 operation")
    pairs=0
    for prepared in (work / "private").rglob("observed-prepare.json"):
        completed=prepared.with_name("observed-complete.json")
        if completed.exists():
            if prepared.read_bytes()!=completed.read_bytes(): raise AssertionError("operation changed between prepare and completion")
            pairs+=1
    if pairs!=1: raise AssertionError("missing original operation phase continuity")
    if name in ("resume-placement", "resume-finalization", "resume-native-completion"):
        operations=list((work / "private").rglob("observed-complete.json"))
        if len(operations)!=1: raise AssertionError("ambiguous original operation")
        original=operations[0].with_name("operation-request.json")
        retained=original.read_bytes()
        events=original.with_name("phase-events.txt").read_text()
        original.with_name("resume-allowed").write_text("resume original")
        resumed=run([work / "fixture.exe","--silent","/AIRKOPERATION="+str(original),"/LOG="+str(work / "resume.log")])
        if resumed.returncode!=0: raise AssertionError("original operation resume failed: "+str(resumed.returncode))
        if original.read_bytes()!=retained: raise AssertionError("resume changed original request")
        if {json.loads(p.read_bytes())["operation_id"] for p in (work / "private").rglob("operation-request.json")}!={json.loads(p.read_bytes())["operation_id"] for p in requests}: raise AssertionError("resume created a new operation key")
        if any(p.read_bytes()!=raw for p,raw in initial_request_bytes.items()): raise AssertionError("resume changed an existing original or plan request")
        if any(p.read_bytes()!=retained for p in (work / "private").rglob("operation-request.json") if p not in initial_request_bytes): raise AssertionError("new producer projection changed original operation bytes")
        after=original.with_name("phase-events.txt").read_text()
        added=after[len(events):]
        if name=="resume-finalization" and ("--prepare" in added or " --seal-inno" not in added or added.count("__airk_role")!=1):
            raise AssertionError("completed native operation was replayed instead of wrapper finalization")
        if name=="resume-native-completion" and ("--prepare" in added or "--seal-inno" not in added or added.count("__airk_role")!=2):
            raise AssertionError("placed candidate was overwritten or native completion was skipped")
        if name=="resume-placement" and ("--prepare" not in added or "--seal-inno" not in added):
            raise AssertionError("admitted original placement did not resume prepare/completion")
        native_exit=0
        path_failure=False
    seals=list((work / "private").rglob("observed-seal.json"))
    if native_exit or path_failure:
        if seals: raise AssertionError("seal ran before successful native and PATH completion")
    elif len(seals)!=1 or seals[0].read_bytes()!=seals[0].with_name("operation-request.json").read_bytes():
        raise AssertionError("seal did not receive the retained original operation bytes")
    if name in ("uninstall-handoff","uninstall-resume"):
        pairs=list(program.glob("unins*.exe"))
        if len(pairs)!=1:raise AssertionError("ambiguous finalized fixture pair")
        final=(digest(pairs[0])+"\n"+digest(pairs[0].with_suffix(".dat"))+"\n").encode("ascii")
        if (work / "pair-post.txt").read_bytes()!=final or (work / "pair-done.txt").read_bytes()!=final:
            raise AssertionError("uninstall EXE/DAT changed after wrapper finalization event")
        original=next((work / "private").rglob("observed-complete.json"))
        request=json.loads(original.read_bytes())
        request["kind"]="uninstall"
        request["operation_id"]=uuid.uuid4().hex
        request["options"]={"login_startup":False,"trust":"skip","start":False,"delete_indexes":"keep"}
        operation=work / "private/handoff-operation.json"
        raw=(json.dumps(request,ensure_ascii=False,separators=(",",":"))+"\n").encode("utf-8")
        operation.write_bytes(raw)
        uninstallers=list(program.glob("unins*.exe"))
        if len(uninstallers)!=1: raise AssertionError("ambiguous fixture uninstaller")
        removed=run([uninstallers[0],"/VERYSILENT","/SUPPRESSMSGBOXES","/NORESTART","/AIRKOPERATION="+str(operation),"/LOG="+str(work / "uninstall.log")])
        if name=="uninstall-resume":
            if removed.returncode==0 or not (program / "service/airk.exe").exists(): raise AssertionError("failed outer removal was not preserved")
            retained_images=list((work / "private").rglob("airk-setup.exe"))
            retained_image=next(path for path in retained_images if path.parent.name not in {original.parent.parent.name})
            operation.with_name("wrapper-removal-allowed").write_text("finish original outer removal")
            before=operation.with_name("phase-events.txt").read_bytes()
            (program / "service/airk.exe").unlink()
            removed=run([uninstallers[0],"/VERYSILENT","/SUPPRESSMSGBOXES","/NORESTART","/AIRKOPERATION="+str(operation),"/AIRKRECOVERYIMAGE="+str(retained_image),"/LOG="+str(work / "uninstall-resume.log")])
            if operation.with_name("phase-events.txt").read_bytes()!=before: raise AssertionError("successful original native removal was replayed")
        if removed.returncode!=0 or (program / "service/airk.exe").exists(): raise AssertionError("fixture uninstall did not remove its payload")
        if operation.with_name("observed-complete.json").read_bytes()!=raw: raise AssertionError("uninstaller recreated the handed-off operation")
    images = list((work / "private").rglob("airk-setup.exe"))
    if not images or any(digest(image) != digest(payload / "service/airk.exe") for image in images):
        raise AssertionError("verified recovery image missing or changed")
    if source_path.read_bytes()!=source_bytes: raise AssertionError("wrapper source changed during fixture execution")
    logs=[work / "setup.log",work / "setup.log.headless.log"]
    for log in logs:
        text=log.read_text(encoding="utf-8",errors="strict")
        if "Setup version: Inno Setup version 6.7.1" not in text: raise AssertionError("wrong Inno runtime version")
        for temporary in re.findall(r"Created temporary directory: (.+)",text):
            if Path(temporary.strip()).exists(): raise AssertionError("owned installer temporary directory survived completion")
    return {"name":name,"exit":result.returncode,"requests":len(requests),"source_sha256":hashlib.sha256(source_bytes).hexdigest(),
            "fixture_sha256":digest(script),"command":[str(x) for x in command]}


def emit_utf8_fixture(root):
    """Compile the actual serializer with fixed clock/id and inert synthetic roots."""
    source=SOURCE.read_text(encoding="utf-8")
    source=source.replace('  FinalizationFailed: Boolean;','  FinalizationFailed: Boolean;\n  EmittingUtf8Fixture: Boolean;')
    source=source.replace('  GetSystemTimeAsFileTime(Time);',"  if EmittingUtf8Fixture then begin Result := 1800000000000; Exit; end;\n  GetSystemTimeAsFileTime(Time);")
    source=source.replace('function NewOperationId: String;\nbegin',"function NewOperationId: String;\nbegin\n  if EmittingUtf8Fixture then begin Result := '"+'0e'*16+"'; Exit; end;")
    prefix='C:'+chr(92)+'Users'+chr(92)+'fixture-'+chr(0xe9)+'-'+''.join(map(chr,[0x65e5,0x672c,0x8a9e]))+'-'+chr(0x1f642)
    destination=str(root/'wrapper_utf8_operation.json').replace("'","''")
    procedure="procedure EmitUtf8OperationFixture;\nvar\n  Operation: String;\nbegin\n  EmittingUtf8Fixture := True;\n  try\n    Operation := WriteOperation('install', LocationsJson('"+prefix+"\\Programs\\AIRK', '"+prefix+"\\AIRK', '"+prefix+"\\AIRKCache'), '"+'5'*64+"', 'keep', False, False, False, False);\n    if not FileCopy(Operation, '"+destination+"', True) then RaiseException('Cannot retain UTF8 fixture');\n  finally\n    EmittingUtf8Fixture := False;\n  end;\nend;\n\n"
    source=source.replace('procedure InitializeWizard;\nbegin',procedure+'procedure InitializeWizard;\nbegin\n  EmitUtf8OperationFixture;')
    path=root/'utf8-source.iss';path.write_text(source,encoding='utf-8')
    result=scenario(root,'unicode-emission',0,False,path)
    raw=(root/'wrapper_utf8_operation.json').read_bytes()
    value=json.loads(raw)
    if value['locations']['program']!=prefix+'\\Programs\\AIRK':raise AssertionError('synthetic Unicode field changed')
    if raw!=(json.dumps(value,ensure_ascii=False,separators=(',',':'))+'\n').encode('utf-8'):raise AssertionError('noncanonical synthetic operation')
    result['case_sha256']=hashlib.sha256(raw).hexdigest()
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--scenario",action="append",choices=("native-failure","path-failure","seal-failure","unicode-success","uninstall-handoff","resume-placement","resume-finalization","uninstall-resume","resume-native-completion"))
    parser.add_argument("--emit-utf8",action="store_true",help="Also emit a stable inert canonical I4 operation from the compiled serializer")
    args=parser.parse_args()
    output=args.output.resolve()
    output.mkdir(parents=True,exist_ok=False)
    names=args.scenario or ["native-failure","path-failure","seal-failure","unicode-success","uninstall-handoff","resume-placement","resume-finalization","uninstall-resume","resume-native-completion"]
    if len(names)!=len(set(names)):parser.error("Duplicate scenario")
    selected=len(names)+int(args.emit_utf8)
    results=[];started=time.perf_counter();failure=None
    try:
        for name in names:results.append(scenario(output,name,7 if name in ("native-failure","resume-placement","resume-native-completion") else 0,name in ("path-failure","resume-finalization")))
        if args.emit_utf8:results.append(emit_utf8_fixture(output))
    except Exception as error:
        failure=str(error)
        raise
    finally:
        receipt={"scope":"compiled inert wrapper only","selected":selected,"passed":len(results),"failure":failure,
            "elapsed_seconds":time.perf_counter()-started,"harness_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "product_source_sha256":hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
            "compiler_sha256":hashlib.sha256(ISCC.read_bytes()).hexdigest(),"commands":RUNS,"results":results}
        (output / "receipt.json").write_text(json.dumps(receipt,indent=2)+"\n",encoding="utf-8")
    print(json.dumps(results,indent=2))

if __name__ == "__main__": main()
