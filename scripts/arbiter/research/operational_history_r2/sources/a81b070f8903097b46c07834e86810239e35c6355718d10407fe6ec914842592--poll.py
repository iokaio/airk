# SPDX-License-Identifier: Apache-2.0
import json,shlex,subprocess
from pathlib import Path
base=Path('artifacts/local/1.8.12-explorer-20261107/mac-neo');c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text(encoding='utf-8-sig'));a=json.loads((base/'active.json').read_text())
ssh=['ssh','-i',c['identity_path'],*c['ssh_options']]
if c.get('port'):ssh+=['-p',str(c['port'])]
script='from pathlib import Path\nimport json\nr=Path('+repr(a['remote'])+')\n'+'''
phase=(r/'phase.txt').read_text() if (r/'phase.txt').exists() else 'preparing'
o={'phase':phase,'steps':json.loads((r/'steps.json').read_text()) if (r/'steps.json').exists() else [],'tail':(r/'owner.log').read_text(errors='replace').splitlines()[-3:]}
current=r/'evidence'/(phase+'.log')
if current.exists():o['current_log_tail']=current.read_text(errors='replace').splitlines()[-6:]
if (r/'qualification.json').exists():o['qualification']=json.loads((r/'qualification.json').read_text())
o['steps']=[{k:x[k] for k in ['name','exit_code','elapsed_seconds']} for x in o['steps']]
if 'qualification' in o:o['qualification']={k:o['qualification'][k] for k in ['status','required_steps_passed','failures','cleanup']}
print(json.dumps(o,indent=2))
'''
p=subprocess.run([*ssh,c['user']+'@'+c['host'],shlex.quote(c['existing_tools']+'/python3.13')+' -'],input=script,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=30)
print(p.stdout);raise SystemExit(p.returncode)
