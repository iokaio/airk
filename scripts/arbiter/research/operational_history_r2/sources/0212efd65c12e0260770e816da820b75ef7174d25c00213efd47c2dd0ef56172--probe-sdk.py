# SPDX-License-Identifier: Apache-2.0
import base64,json,shlex,subprocess
from pathlib import Path
config=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());assert config['host']=='192.168.68.61'
code='''
import json,subprocess
from pathlib import Path
sdk=Path('/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include')
patterns=('PROC_PGRP_ONLY','proc_listpids','proc_pidinfo','PROC_PIDTBSDINFO','struct proc_bsdinfo','pbi_pid','pbi_ppid','pbi_pgid','pbi_status','SZOMB','SSTOP','SIDL','SRUN','SSLEEP')
result={'sdk':str(sdk),'headers':{}}
for name in ['libproc.h','sys/proc_info.h','sys/proc.h']:
 lines=(sdk/name).read_text().splitlines(); selected=set()
 for i,line in enumerate(lines):
  if any(x in line for x in patterns):selected.update(range(max(0,i-2),min(len(lines),i+45 if 'struct proc_bsdinfo' in line else i+3)))
 result['headers'][name]=[{'line':i+1,'text':lines[i]} for i in sorted(selected)]
p=subprocess.run(['/bin/ps','-axo','pid,pgid,uid,state,command'],capture_output=True,text=True,timeout=10)
result['campaign_processes']=[line for line in p.stdout.splitlines() if '/Users/tylerjensen/airk-builds/one-airk-static-ort-1814-1794087059' in line]
result['ps_exit']=p.returncode
print(json.dumps(result,indent=2))
'''
python='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13'
command=shlex.quote(python)+' -c '+shlex.quote('import base64;exec(base64.b64decode('+repr(base64.b64encode(code.encode()).decode())+'))')
p=subprocess.run(['ssh','-i',config['identity_path'],*config['ssh_options'],config['user']+'@'+config['host'],command],capture_output=True,timeout=30)
out=Path('artifacts/local/one-airk-1814/neo-static-ort/supplement-1794092444')
(out/'sdk-process-diagnostic.json').write_bytes(p.stdout);(out/'sdk-process-diagnostic.stderr').write_bytes(p.stderr)
print(p.stdout.decode());print(p.stderr.decode());raise SystemExit(p.returncode)
