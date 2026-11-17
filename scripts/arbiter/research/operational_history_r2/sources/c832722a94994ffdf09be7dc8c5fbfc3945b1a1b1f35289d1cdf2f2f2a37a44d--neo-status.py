# SPDX-License-Identifier: Apache-2.0
import json,shlex,subprocess,time
from pathlib import Path
b=Path('artifacts/readiness-1817');c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());remote='/Users/tylerjensen/airk-builds/readiness-1817-r1'
s='''import json
from pathlib import Path
p=Path('''+repr(remote)+'''); r={"phase":(p/'phase.txt').read_text() if (p/'phase.txt').exists() else 'preparing',"owner_tail":(p/'owner.log').read_text(errors='replace')[-1800:]};
for name in ['qualification.json','cleanup.json','dependency-fetch.json']:
 if (p/name).exists():
  v=json.loads((p/name).read_text()); r[name]={k:value for k,value in v.items() if k not in ['tests','files','groups']}
print(json.dumps(r))'''
r=subprocess.run(['ssh','-i',c['identity_path'],*c['ssh_options'],c['user']+'@'+c['host'],'/usr/bin/python3 -c '+shlex.quote(s)],text=True,capture_output=True,timeout=25)
if r.returncode: print(r.stderr);raise SystemExit(r.returncode)
v=json.loads(r.stdout);(b/'neo-status-current.json').write_text(json.dumps(v,indent=2)+'\n');print(json.dumps(v))
