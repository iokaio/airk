# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
connection=json.loads(Path("artifacts/v190/neo-connection.local.json").read_text())
command=["ssh",*connection["ssh_options"],"-i",connection["identity_path"],"-p",str(connection["port"]),connection["user"]+"@"+connection["host"],"sh","-s"]
remote="""set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
import importlib.util,json,subprocess,threading
from pathlib import Path
spec=importlib.util.spec_from_file_location('core',Path('scripts/provision-static-onnx-runtime.py'))
core=importlib.util.module_from_spec(spec);spec.loader.exec_module(core)
root=Path('artifacts/advanced-split/ort130-arm64/tools').resolve();program=root/'cmake/CMake.app/Contents/bin/cmake';app=program.parent.parent.parent
for argv in [['/usr/bin/file',str(program)],['/usr/bin/xattr','-l',str(app)],['/usr/bin/codesign','--verify','--deep','--strict','--verbose=2',str(app)]]:
 try:
  r=subprocess.run(argv,capture_output=True,timeout=10);print('INSPECT',argv[0],r.returncode,repr(r.stdout[:4096]),repr(r.stderr[:4096]))
 except subprocess.TimeoutExpired:print('INSPECT_TIMEOUT',argv[0])
log=core.probe_log(root,'cmake-sample');original=core.subprocess.Popen;ready=threading.Event();captured={}
def spawn(argv,*args,**kwargs):
 child=original(argv,*args,**kwargs)
 if str(argv[0])==str(program):captured['pid']=child.pid;ready.set()
 return child
def sample():
 if not ready.wait(5):return
 try:
  samplepath=log.parent/'sample.txt';r=subprocess.run(['/usr/bin/sample',str(captured['pid']),'1','1','-file',str(samplepath)],capture_output=True,timeout=5)
  captured.update(sample_return=r.returncode,sample=samplepath.read_text(errors='replace')[:24000] if samplepath.exists() else '',stderr=r.stderr.decode(errors='replace')[:4096])
 except Exception as error:captured['error']=str(error)
core.subprocess.Popen=spawn;thread=threading.Thread(target=sample);thread.start()
result=core.run_owned([program,'--version'],root,log,15);thread.join(6)
print(json.dumps(result,indent=2));print('ACTUAL_LOG',repr(log.read_bytes()));print('SAMPLE',json.dumps(captured))
PY
"""
r=subprocess.run(command,input=remote.encode(),capture_output=True,timeout=45)
Path('artifacts/advanced-split/receipts/delivery-04-neo-cmake-startup-sample.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode(errors='replace'));print(r.stderr.decode(errors='replace'));raise SystemExit(r.returncode)
