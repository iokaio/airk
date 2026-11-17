# SPDX-License-Identifier: Apache-2.0
"""One isolated native ARM64 static ORT prerequisite campaign; never AIRK/Cargo."""
import hashlib,json,os,platform,resource,shlex,shutil,signal,subprocess,sys,tarfile,time,urllib.parse,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent
assert ROOT.name.startswith("one-airk-static-ort-1814-") and ROOT.parent==Path("/Users/tylerjensen/airk-builds")
assert ROOT.resolve(strict=True)==ROOT and ROOT.stat().st_uid==501 and os.getuid()==501 and platform.machine()=="arm64"
assert subprocess.check_output(["hostname"],text=True).strip()=="Tylers-MacBook-Neo.local"
E=ROOT/"evidence";E.mkdir(exist_ok=False);TOOLS=ROOT/"tools";TOOLS.mkdir(exist_ok=False)
R={"schema":1,"purpose":"native ARM64 static ORT prerequisite production only; not AIRK qualification","host":"192.168.68.61","hostname":"Tylers-MacBook-Neo.local","upstream_commit":"89746dc19a0a1ae59ebf4b16df9acab8f99f3925","ort_version":"1.22.1","architecture":"arm64","deployment_target":"13.3","parallel":2,"cargo_executed":False,"airk_source_checkout":False,"keychain_or_signing_operations":False,"commands":[],"downloads":[],"result":"running","free_bytes_before":shutil.disk_usage(ROOT).free}
started=time.monotonic();current_group=None
soft,hard=resource.getrlimit(resource.RLIMIT_NOFILE);R["nofile_before"]=[soft,hard];resource.setrlimit(resource.RLIMIT_NOFILE,(4096,hard));R["nofile_after"]=list(resource.getrlimit(resource.RLIMIT_NOFILE))
def save():
 (E/"receipt.json").write_text(json.dumps(R,indent=2)+"\n")
def digest(p,algorithm="sha256"):
 with p.open("rb") as f:return hashlib.file_digest(f,algorithm).hexdigest()
def run(name,argv,timeout=600,cwd=ROOT,env=None):
 global current_group
 (ROOT/"phase.txt").write_text(name)
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
 R["toolchain"]={"clang":subprocess.check_output(["/usr/bin/clang","--version"],text=True),"sdk":subprocess.check_output(["/usr/bin/xcrun","--sdk","macosx","--show-sdk-path"],text=True).strip()};save()
 package=ROOT/"public-prerequisite-inputs.tar";assert digest(package)=="ce2c9044a5f5a77199ff3a347cc679244c30034ff9ec9bb9d8f149c8789e7247"
 with tarfile.open(package) as archive:archive.extractall(ROOT/"inputs",filter="data")
 deps=json.loads((ROOT/"inputs/cpu-dependencies.json").read_text());assert digest(ROOT/"inputs/cpu-dependencies.json")=="067284e8a09958a10cf0f59f506054d6c2115ea1cecd496bff5ab53a25ab4787"
 cmake_archive=fetch("CMake3.31.6","https://github.com/Kitware/CMake/releases/download/v3.31.6/cmake-3.31.6-macos-universal.tar.gz",TOOLS/"cmake.tar.gz",78374480,sha256="330b9514f5112e5ed4fb08b8b05803b776fd9b539a6ae12927d14dcc0ee2ba8d")
 with tarfile.open(cmake_archive) as archive:archive.extractall(TOOLS,filter="data")
 CMAKE=TOOLS/"cmake-3.31.6-macos-universal/CMake.app/Contents/bin/cmake";assert CMAKE.is_file()
 env=os.environ.copy();env.update(MACOSX_DEPLOYMENT_TARGET="13.3",CC="/usr/bin/clang",CXX="/usr/bin/clang++",PATH=str(CMAKE.parent)+":"+env.get("PATH","/usr/bin:/bin"))
 run("cmake-version",[CMAKE,"--version"],60,env=env)
 ninja_source=ROOT/"ninja-source"
 run("ninja-clone",["/usr/bin/git","-c","core.autocrlf=false","clone","--depth","1","--branch","v1.12.1","https://github.com/ninja-build/ninja.git",ninja_source],600,env=env)
 assert subprocess.check_output(["/usr/bin/git","-C",str(ninja_source),"rev-parse","HEAD"],text=True).strip()=="2daa09ba270b0a43e1929d29b073348aa985dfaa"
 run("ninja-configure",[CMAKE,"-S",ninja_source,"-B",ROOT/"ninja-build","-G","Unix Makefiles","-DCMAKE_BUILD_TYPE=Release","-DCMAKE_OSX_ARCHITECTURES=arm64","-DCMAKE_OSX_DEPLOYMENT_TARGET=13.3","-DBUILD_TESTING=OFF"],600,env=env)
 run("ninja-build",[CMAKE,"--build",ROOT/"ninja-build","--parallel","2"],600,env=env)
 NINJA=ROOT/"ninja-build/ninja";assert NINJA.is_file();run("ninja-version",[NINJA,"--version"],60,env=env)
 source=ROOT/"onnxruntime-source"
 run("ort-source-clone",["/usr/bin/git","-c","core.autocrlf=false","clone","--depth","1","--branch","v1.22.1","https://github.com/microsoft/onnxruntime.git",source],1200,env=env)
 assert subprocess.check_output(["/usr/bin/git","-C",str(source),"rev-parse","HEAD"],text=True).strip()==R["upstream_commit"]
 assert (source/"VERSION_NUMBER").read_text().strip()=="1.22.1"
 metadata=json.loads((ROOT/"inputs/upstream-metadata.json").read_text())
 for item in metadata["files"]:assert digest(source/item["path"])==item["sha256"],item["path"]
 R["source_tree"]=subprocess.check_output(["/usr/bin/git","-C",str(source),"rev-parse","HEAD^{tree}"],text=True).strip()
 R["source_metadata_sha256"]=digest(ROOT/"inputs/upstream-metadata.json");R["dependency_manifest_sha256"]=digest(ROOT/"inputs/cpu-dependencies.json")
 for item in deps["dependencies"]:
  original=ROOT/"inputs/dependency-mirror"/item["mirror_relative_path"];assert original.stat().st_size==item["bytes"] and digest(original)==item["sha256"] and digest(original,"sha1")==item["upstream_sha1"]
  target=source/"mirror"/item["mirror_relative_path"];target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(original,target)
 protoc_line=next(line for line in (source/"cmake/deps.txt").read_text().splitlines() if line.startswith("protoc_mac_universal;"));_,url,sha1=protoc_line.split(";");assert sha1=="23710c3d1c2036d8d65a6a22234372fa2d7af9ef"
 parsed=urllib.parse.urlsplit(url);protoc=source/"mirror"/parsed.netloc/parsed.path.lstrip("/");protoc.parent.mkdir(parents=True,exist_ok=True);fetch("protoc21.12osx-universal",url,protoc,32*1024*1024,sha1=sha1)
 build=ROOT/"b/Release";assert not build.exists()
 configure=[CMAKE,"-S",source/"cmake","-B",build,"-G","Ninja","-DCMAKE_BUILD_TYPE=Release","-DCMAKE_MAKE_PROGRAM="+str(NINJA),"-DCMAKE_OSX_ARCHITECTURES=arm64","-DCMAKE_OSX_DEPLOYMENT_TARGET=13.3","-DCMAKE_OSX_SYSROOT="+R["toolchain"]["sdk"],"-DFETCHCONTENT_TRY_FIND_PACKAGE_MODE=NEVER","-DPatch_EXECUTABLE=/usr/bin/patch","-Donnxruntime_BUILD_SHARED_LIB=OFF","-Donnxruntime_BUILD_UNIT_TESTS=OFF","-Donnxruntime_BUILD_BENCHMARKS=OFF","-Donnxruntime_BUILD_FOR_NATIVE_MACHINE=OFF","-Donnxruntime_MINIMAL_BUILD=OFF","-Donnxruntime_DISABLE_CONTRIB_OPS=OFF","-Donnxruntime_DISABLE_ML_OPS=OFF","-Donnxruntime_ENABLE_PYTHON=OFF","-Donnxruntime_USE_CUDA=OFF","-Donnxruntime_USE_DML=OFF","-Donnxruntime_USE_COREML=OFF","-Donnxruntime_USE_KLEIDIAI=OFF","-Donnxruntime_USE_VCPKG=OFF","-Donnxruntime_USE_TELEMETRY=OFF","-Donnxruntime_ENABLE_LTO=OFF"]
 R["configure"]=list(map(str,configure));R["tools"]={"cmake":str(CMAKE),"ninja":str(NINJA),"ninja_source_commit":"2daa09ba270b0a43e1929d29b073348aa985dfaa","cmake_sha256":digest(CMAKE),"ninja_sha256":digest(NINJA)};save()
 run("ort-configure",configure,600,env=env)
 run("ort-build",[CMAKE,"--build",build,"--config","Release","--parallel","2"],10800,env=env)
 libraries=[{"path":str(p.relative_to(build)),"bytes":p.stat().st_size,"sha256":digest(p)} for p in sorted(build.rglob("*.a"))]
 assert any(x["path"]=="libonnxruntime_session.a" for x in libraries),"CPU staticcomponent archive missing"
 R["libraries"]=libraries;R["result"]="static_libraries_built_not_AIRK_model_or_link_qualified"
 R["tracked_source_diff"]=run("ort-tracked-source-unchanged",["/usr/bin/git","-C",source,"diff","--exit-code","HEAD","--"],60,env=env)
except Exception as error:
 R["result"]="failed";R["error"]=str(error);print("FAILED: "+str(error),flush=True)
finally:
 R["elapsed_seconds"]=round(time.monotonic()-started,3);R["free_bytes_after"]=shutil.disk_usage(ROOT).free;R["owned_command_group_active"]=current_group;R["cargo_cleanup"]="not_applicable_no_Cargo_used";R["retention"]="source,tools,staticlibraries,andcompleteevidence retained in exact owned root; no deletion";save();(ROOT/"phase.txt").write_text("complete");print(json.dumps({k:R[k] for k in ["result","elapsed_seconds","error"] if k in R}),flush=True)
raise SystemExit(0 if R["result"].startswith("static_libraries_built") else 1)
