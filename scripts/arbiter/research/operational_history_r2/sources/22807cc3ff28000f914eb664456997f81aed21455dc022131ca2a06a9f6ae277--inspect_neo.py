# SPDX-License-Identifier: Apache-2.0
"""Read-only Neo inventory; connection secrets never enter printed output."""
from pathlib import Path
import json,subprocess,time,hashlib
ROOT=Path(__file__).resolve().parents[3];packet=ROOT/"artifacts/v190/neo-connection.local.json";d=json.loads(packet.read_bytes())
remote=r'''import os,json,subprocess,pwd,hashlib,shutil
from pathlib import Path
home=Path(pwd.getpwuid(os.geteuid()).pw_dir);repo=home/'code/airk'
def command(argv,cwd=None):
 try:
  x=subprocess.run(argv,cwd=str(cwd) if cwd else None,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=30)
  return {'exit':x.returncode,'stdout':x.stdout[:16000],'stderr':x.stderr[:2000]}
 except Exception as e:return {'error':type(e).__name__}
report={'schema':'airk.private-neo-readonly-preflight/1','read_only':True,'uid':os.geteuid(),'user':pwd.getpwuid(os.geteuid()).pw_name,'home':str(home),'platform':command(['/usr/bin/uname','-sm']),'os_version':command(['/usr/bin/sw_vers']),'disk':command(['/bin/df','-Pk',str(repo)]),'memory':command(['/usr/sbin/sysctl','-n','hw.memsize']),'cpu':command(['/usr/sbin/sysctl','-n','hw.physicalcpu']),'tools':{name:shutil.which(name) for name in ['git','codesign','xcrun','security','node','npm','cargo','rustup','python3','ssh']}}
if repo.is_dir():
 report['checkout']={'head':command(['/usr/bin/git','rev-parse','HEAD'],repo),'branch':command(['/usr/bin/git','branch','--show-current'],repo),'status':command(['/usr/bin/git','status','--short'],repo)}
 tool=home/'.cargo/bin/rustup'
 if tool.is_file():report['toolchains']=command([str(tool),'toolchain','list'])
 path=repo/'rust-toolchain.toml'
 if path.is_file():report['toolchain_file_sha256']=hashlib.sha256(path.read_bytes()).hexdigest()
paths=[home/'Library/Application Support/AIRK/install-pointer/current.json',home/'Library/Application Support/AIRK/install-pointer/active.json',home/'Library/Application Support/AIRK Service',home/'Applications/AIRK.app']
report['installation_paths']={str(path):{'exists':path.exists(),'symlink':path.is_symlink()} for path in paths}
process=command(['/bin/ps','-axo','pid=,comm=']);report['airk_processes']=[line for line in process.get('stdout','').splitlines() if any(part in line.casefold() for part in ['/airk','airkctl'])];report['process_observation_exit']=process.get('exit')
launch=command(['/bin/launchctl','list']);report['AIRK_launch_jobs']=[line for line in launch.get('stdout','').splitlines() if 'airk' in line.casefold()];report['launch_jobs_exit']=launch.get('exit')
print(json.dumps(report))
'''
argv=['ssh','-p',str(d['port']),'-i',str(d['identity_path']),*d['ssh_options'],d['user']+'@'+d['host'],'/usr/bin/python3','-'];started=time.monotonic()
x=subprocess.run(argv,input=remote,capture_output=True,text=True,timeout=60)
r={'schema':'airk.private-neo-readonly-command/1','ssh_exit':x.returncode,'elapsed_seconds':time.monotonic()-started,'connection_file_sha256':hashlib.sha256(packet.read_bytes()).hexdigest(),'credentials_printed':False}
if x.returncode==0:r['observation']=json.loads(x.stdout)
else:r['error']='SSH refused; raw stderr retained privately';r['stderr_sha256']=hashlib.sha256(x.stderr.encode()).hexdigest()
out=Path(__file__).with_name('neo-readonly-normal-owner.json');assert not out.exists();out.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))
if x.returncode:raise SystemExit(x.returncode)
