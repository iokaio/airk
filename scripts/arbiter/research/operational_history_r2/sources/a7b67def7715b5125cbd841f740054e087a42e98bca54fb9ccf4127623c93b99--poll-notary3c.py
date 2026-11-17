# SPDX-License-Identifier: Apache-2.0
import json,shlex,subprocess
from pathlib import Path
b=Path(__file__).resolve().parent;c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text(encoding='utf-8-sig'));r=json.loads((b/'notary-active.json').read_text())['remote']
code='from pathlib import Path;import json;r=Path('+repr(r)+');'+'''
phase=(r/'notary3c-phase.txt').read_text();report={'phase':phase}
steps=r/'notary3c-steps.json'
if steps.exists():report['steps']=[{k:v[k] for k in ['name','exit_code','elapsed_seconds']} for v in json.loads(steps.read_text())]
log=r/'evidence3c'/(phase+'.log')
if log.exists():report['current_log_tail']=log.read_text(errors='replace')[-2200:]
result=r/'notary3c-result.json'
if result.exists():report['result']={k:v for k,v in json.loads(result.read_text()).items() if k in ['status','error','target_absent','accepted_notarization_ids','original_runtime_unchanged','gui_cleanup']}
print(json.dumps(report,indent=2))
'''
p=subprocess.run(['ssh','-i',c['identity_path'],*c['ssh_options'],c['user']+'@'+c['host'],shlex.quote(c['existing_tools']+'/python3.13')+' -'],input=code,text=True,capture_output=True,timeout=30)
print(p.stdout);print(p.stderr);raise SystemExit(p.returncode)
