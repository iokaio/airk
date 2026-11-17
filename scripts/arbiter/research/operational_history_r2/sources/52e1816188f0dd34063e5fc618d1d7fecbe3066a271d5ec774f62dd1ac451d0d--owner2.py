# SPDX-License-Identifier: Apache-2.0
"""One isolated native ARM64 static ORT prerequisite campaign; never AIRK/Cargo."""
import hashlib,json,os,platform,resource,shlex,shutil,signal,subprocess,sys,tarfile,time,urllib.parse,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent
assert ROOT.name.startswith("one-airk-static-ort-1814-") and ROOT.parent==Path("/Users/tylerjensen/airk-builds")
assert ROOT.resolve(strict=True)==ROOT and ROOT.stat().st_uid==501 and os.getuid()==501 and platform.machine()=="arm64"
assert subprocess.check_output(["hostname"],text=True).strip()=="Tylers-MacBook-Neo.local"
E=ROOT/"evidence2";E.mkdir(exist_ok=False);TOOLS=ROOT/"tools";assert TOOLS.is_dir()
R={"schema":1,"purpose":"native ARM64 static ORT prerequisite production only; not AIRK qualification","host":"192.168.68.61","hostname":"Tylers-MacBook-Neo.local","upstream_commit":"89746dc19a0a1ae59ebf4b16df9acab8f99f3925","ort_version":"1.22.1","architecture":"arm64","deployment_target":"13.3","parallel":2,"cargo_executed":False,"airk_source_checkout":False,"keychain_or_signing_operations":False,"commands":[],"downloads":[],"result":"running","free_bytes_before":shutil.disk_usage(ROOT).free}
started=time.monotonic();current_group=None
soft,hard=resource.getrlimit(resource.RLIMIT_NOFILE);R["nofile_before"]=[soft,hard];resource.setrlimit(resource.RLIMIT_NOFILE,(4096,hard));R["nofile_after"]=list(resource.getrlimit(resource.RLIMIT_NOFILE))
def save():
 (E/"receipt.json").write_text(json.dumps(R,indent=2)+"\n")
def digest(p,algorithm="sha256"):
 with p.open("rb") as f:return hashlib.file_digest(f,algorithm).hexdigest()
def run(name,argv,timeout=600,cwd=ROOT,env=None):
 global current_group
 (ROOT/"phase2.txt").write_text(name)
 row={"stage":name,"argv":[str(a) for a in argv],"cwd":str(cwd),"timeout_seconds":timeout,"started_unix":time.time(),"log":str(E/(name+".log"))};R["commands"].append(row);save();tick=time.monotonic()
 print("Starting "+name,flush=True)
 with (E/(name+".log")).open("wb") as out:
  proc=subprocess.Popen(row["argv"],cwd=cwd,env=env,stdout=out,stderr=subprocess.STDOUT,start_new_session=True)
  current_group=proc.pid;row["owned_process_group"]=proc.pid;save()
  try:code=proc.wait(timeout=timeout)
  except subprocess.TimeoutExpired:
   row["timed_out"]=True
   assert os.getpgid(proc.pid)==proc.pid
   os.killpg(proc.pid,signal.SIGTERM)
   try:proc.wait(timeout=10)
   except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=10)
   code=proc.returncode
  until=time.monotonic()+2
  while True:
   try:os.killpg(proc.pid,0);empty=False
   except ProcessLookupError:empty=True
   if empty or time.monotonic()>=until:break
   time.sleep(.05)
  row["process_group_empty_after_exit"]=empty
  if not empty:raise RuntimeError(name+" owned descendant group still observed; preserve evidence and stop")
  current_group=None
 row.update(exit_code=code,elapsed_seconds=round(time.monotonic()-tick,3));save()
 if code:raise RuntimeError(name+" failed; stop without retry; inspect "+row["log"])
 return (E/(name+".log")).read_text(errors="replace")
def fetch(name,url,target,maximum,sha256=None,sha1=None):
 assert url.startswith("https://") and not target.exists()
 tick=time.monotonic();request=urllib.request.Request(url+('?airk='+str(int(time.time()))),headers={"User-Agent":"AIRK-static-ORT-prerequisite/1"})
 with urllib.request.urlopen(request,timeout=120) as response,target.open("xb") as out:
  final=urllib.parse.urlsplit(response.url);assert final.scheme=="https" and final.hostname in {"github.com","codeload.github.com","release-assets.githubusercontent.com","objects.githubusercontent.com"}
  count=0
  while block:=response.read(1024*1024):
   count+=len(block)
   if count>maximum:raise RuntimeError(name+" exceeds bound")
   out.write(block)
  out.flush();os.fsync(out.fileno())
 actual=digest(target)
 if sha256:assert actual==sha256,name+" SHA256 differs"
 if sha1:assert digest(target,"sha1")==sha1,name+" upstream SHA1 differs"
 R["downloads"].append({"name":name,"url":url,"bytes":count,"sha256":actual,"upstream_sha256":sha256,"upstream_sha1":sha1,"elapsed_seconds":round(time.monotonic()-tick,3)});save()
 return target
try:
 save()
 prior_file=ROOT/"evidence/receipt.json";assert digest(prior_file)=="8cf93f568a5e643542a9f609a219c320247317dcee407a57329a974530bcff59"
 prior=json.loads(prior_file.read_text());assert prior["result"]=="failed" and prior["commands"][-1]["stage"]=="ort-configure"
 assert "PYTHON_EXECUTABLE-NOTFOUND" in (ROOT/"b/Release/CMakeCache.txt").read_text()
 R["reuses_verified_prerequisite_attempt"]={"receipt_sha256":digest(prior_file),"result":"failed_at_missing_PYTHON_EXECUTABLE_before_ORT_compilation","original_source_and_tools_retained":True}
 source=ROOT/"onnxruntime-source";assert subprocess.check_output(["/usr/bin/git","-C",str(source),"rev-parse","HEAD"],text=True).strip()==R["upstream_commit"]
 metadata=json.loads((ROOT/"inputs/upstream-metadata.json").read_text())
 for item in metadata["files"]:assert digest(source/item["path"])==item["sha256"],item["path"]
 deps=json.loads((ROOT/"inputs/cpu-dependencies.json").read_text());assert digest(ROOT/"inputs/cpu-dependencies.json")=="067284e8a09958a10cf0f59f506054d6c2115ea1cecd496bff5ab53a25ab4787"
 for item in deps["dependencies"]:
  archive=source/"mirror"/item["mirror_relative_path"];assert archive.stat().st_size==item["bytes"] and digest(archive)==item["sha256"] and digest(archive,"sha1")==item["upstream_sha1"]
 protoc=source/"mirror/github.com/protocolbuffers/protobuf/releases/download/v21.12/protoc-21.12-osx-universal_binary.zip";assert digest(protoc)=="1acab3d1f774d120d5a38bccb78fd379d35226b254dc258978bc45593478ca4c" and digest(protoc,"sha1")=="23710c3d1c2036d8d65a6a22234372fa2d7af9ef"
 CMAKE=Path(prior["tools"]["cmake"]);NINJA=Path(prior["tools"]["ninja"])
 assert digest(CMAKE)==prior["tools"]["cmake_sha256"] and digest(NINJA)==prior["tools"]["ninja_sha256"]
 env=os.environ.copy();env.update(MACOSX_DEPLOYMENT_TARGET="13.3",CC="/usr/bin/clang",CXX="/usr/bin/clang++",PATH=str(Path(sys.executable).parent)+":"+str(CMAKE.parent)+":"+env.get("PATH","/usr/bin:/bin"))
 python=Path(sys.executable).resolve(strict=True)
 R["python"]={"path":str(python),"sha256":digest(python),"version":sys.version}
 R["toolchain"]=prior["toolchain"];R["tools"]=prior["tools"];R["source_tree"]=prior["source_tree"];R["source_metadata_sha256"]=prior["source_metadata_sha256"];R["dependency_manifest_sha256"]=prior["dependency_manifest_sha256"]
 build=ROOT/"b2/Release";assert not build.exists()
 configure=prior["configure"].copy();configure[configure.index("-B")+1]=str(build)
 configure += ["-DPYTHON_EXECUTABLE="+str(python),"-DPython3_EXECUTABLE="+str(python)]
 R["configure"]=configure;save()
 run("python-version",[python,"--version"],60,env=env)
 run("ort-configure",configure,600,env=env)
 run("ort-build",[CMAKE,"--build",build,"--config","Release","--parallel","2"],10800,env=env)
 R["libraries"]=[{"path":str(p.relative_to(build)),"bytes":p.stat().st_size,"sha256":digest(p)} for p in sorted(build.rglob("*.a"))]
 assert any(x["path"]=="libonnxruntime_session.a" for x in R["libraries"]),"CPU staticcomponent archive missing"
 R["tracked_source_diff"]=run("ort-tracked-source-unchanged",["/usr/bin/git","-C",source,"diff","--exit-code","HEAD","--"],60,env=env)
 R["result"]="static_libraries_built_not_AIRK_model_or_link_qualified"
except Exception as error:
 R["result"]="failed";R["error"]=str(error);print("FAILED: "+str(error),flush=True)
finally:
 R["elapsed_seconds"]=round(time.monotonic()-started,3);R["free_bytes_after"]=shutil.disk_usage(ROOT).free;R["owned_command_group_active"]=current_group;R["cargo_cleanup"]="not_applicable_no_Cargo_used";R["retention"]="Bothattempts,source,tools,staticlibraries,andcompleteevidence retained; no deletion";save();(ROOT/"phase2.txt").write_text("complete");print(json.dumps({k:R[k] for k in ["result","elapsed_seconds","error"] if k in R}),flush=True)
raise SystemExit(0 if R["result"].startswith("static_libraries_built") else 1)
