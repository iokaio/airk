# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
connection=json.loads(Path("artifacts/v190/neo-connection.local.json").read_text())
command=["ssh",*connection["ssh_options"],"-i",connection["identity_path"],"-p",str(connection["port"]),connection["user"]+"@"+connection["host"],"sh","-s"]
remote="""set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 - <<'PY'
from pathlib import Path
import importlib.util,json,subprocess,threading,os
spec=importlib.util.spec_from_file_location('core',Path('scripts/provision-static-onnx-runtime.py'));core=importlib.util.module_from_spec(spec);spec.loader.exec_module(core)
root=Path('artifacts/advanced-split/ort130-arm64/tools').resolve();binary=root/'cmake/CMake.app/Contents/bin/cmake';log=core.probe_log(root,'cmake-minimal-env');original=core.subprocess.Popen;ready=threading.Event();details={}
def spawn(argv,*args,**kwargs):
 child=original(argv,*args,**kwargs)
 if argv[0]=='/bin/sh':details['pid']=child.pid;ready.set()
 return child
def inspect():
 if not ready.wait(3):return
 for argv in [['/bin/ps','-p',str(details['pid']),'-o','pid=,stat=,wchan=,pcpu=,time=,rss=,command='],['/usr/sbin/lsof','-p',str(details['pid'])]]:
  try:
   r=subprocess.run(argv,capture_output=True,timeout=3);details[argv[0]]={'exit':r.returncode,'stdout':r.stdout.decode(errors='replace')[:12000],'stderr':r.stderr.decode(errors='replace')[:1000]}
  except subprocess.TimeoutExpired:details[argv[0]]='timeout'
core.subprocess.Popen=spawn;thread=threading.Thread(target=inspect);thread.start()
environment={'HOME':os.environ['HOME'],'PATH':'/usr/bin:/bin:/usr/sbin:/sbin','TMPDIR':os.environ.get('TMPDIR','/tmp'),'LANG':'en_US.UTF-8'}
result=core.run_owned(['/bin/sh','-c','printf BOOTSTRAP_READY; exec "$@"','owned-probe',str(binary),'--version'],root,log,15,environment);thread.join(4);core.write_json(log.parent/'result.json',result);print(json.dumps(result));print('LOG',repr(log.read_bytes()));print('DETAILS',json.dumps(details));print('LOADER_ENV_KEYS',[key for key in os.environ if key.startswith(('DYLD_','Malloc','CMAKE_'))])
PY
"""
r=subprocess.run(command,input=remote.encode(),capture_output=True,timeout=45)
Path('artifacts/advanced-split/receipts/delivery-04-neo-cmake-state-diagnosis.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode(errors='replace'));print(r.stderr.decode(errors='replace'));raise SystemExit(r.returncode)
