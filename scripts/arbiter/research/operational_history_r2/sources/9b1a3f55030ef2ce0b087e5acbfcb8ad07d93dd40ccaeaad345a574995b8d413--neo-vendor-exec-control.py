# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
connection=json.loads(Path("artifacts/v190/neo-connection.local.json").read_text())
command=["ssh",*connection["ssh_options"],"-i",connection["identity_path"],"-p",str(connection["port"]),connection["user"]+"@"+connection["host"],"sh","-s"]
remote="""set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 - <<'PY'
from pathlib import Path
import importlib.util,json,subprocess,threading,time
spec=importlib.util.spec_from_file_location('core',Path('scripts/provision-static-onnx-runtime.py'));core=importlib.util.module_from_spec(spec);spec.loader.exec_module(core)
root=Path('artifacts/advanced-split/ort130-arm64/tools').resolve();binary=root/'cmake/CMake.app/Contents/bin/ctest';log=core.probe_log(root,'ctest-control');original=core.subprocess.Popen;ready=threading.Event();detail={}
def spawn(argv,*args,**kwargs):
 child=original(argv,*args,**kwargs)
 if str(argv[0])==str(binary):detail['pid']=child.pid;ready.set()
 return child
def inspect():
 if not ready.wait(3):return
 for argv in [['/bin/ps','-M','-p',str(detail['pid']),'-o','pid=,stat=,wchan=,pcpu=,time=,rss=,flags=,command=']]:
  r=subprocess.run(argv,capture_output=True,timeout=3);detail['threads']={'exit':r.returncode,'stdout':r.stdout.decode(errors='replace')[:10000],'stderr':r.stderr.decode(errors='replace')[:1000]}
core.subprocess.Popen=spawn;thread=threading.Thread(target=inspect);thread.start();result=core.run_owned([binary,'--version'],root,log,15);thread.join(4);core.write_json(log.parent/'result.json',result);print(json.dumps(result));print('LOG',repr(log.read_bytes()));print('DETAIL',json.dumps(detail))
r=subprocess.run(['/usr/bin/log','show','--last','5m','--style','compact','--info','--debug','--predicate','process == "amfid" OR process == "taskgated-helper"'],capture_output=True,timeout=15);print('AMFI',r.returncode,r.stdout.decode(errors='replace')[-24000:],r.stderr.decode(errors='replace')[:1000])
PY
"""
r=subprocess.run(command,input=remote.encode(),capture_output=True,timeout=45)
Path('artifacts/advanced-split/receipts/delivery-04-neo-vendor-exec-control.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode(errors='replace'));print(r.stderr.decode(errors='replace'));raise SystemExit(r.returncode)
