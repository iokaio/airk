# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,shlex,subprocess
b=Path(__file__).resolve().parent;c=json.loads(Path("artifacts/v190/neo-connection.local.json").read_text(encoding="utf-8-sig"));a=json.loads((b/"active.json").read_text());assert c["host"]=="192.168.68.61"
ssh=["ssh","-i",c["identity_path"],*c["ssh_options"],"-p",str(c.get("port",22)),c["user"]+"@"+c["host"]]
s="from pathlib import Path\nimport json,time\nr=Path("+repr(a["remote"])+")\n"+"""p=r/'evidence2/receipt.json'
x=json.loads(p.read_text()) if p.exists() else {'result':'preparing'}
phase=(r/'phase2.txt').read_text() if (r/'phase2.txt').exists() else 'preparing'
log=r/'evidence2'/(phase+'.log')
o={'phase':phase,'result':x['result'],'commands':[{k:v for k,v in row.items() if k in ['stage','exit_code','elapsed_seconds','process_group_empty_after_exit']} for row in x.get('commands',[])],'tail':log.read_text(errors='replace').splitlines()[-5:] if log.exists() else (r/'owner2.log').read_text(errors='replace').splitlines()[-5:]}
if x.get('commands') and 'elapsed_seconds' not in x['commands'][-1]:o['current_stage_elapsed_seconds']=round(time.time()-x['commands'][-1]['started_unix'],3)
if 'error' in x:o['error']=x['error']
if 'elapsed_seconds' in x:o['elapsed_seconds']=x['elapsed_seconds']
print(json.dumps(o,indent=2))
"""
p=subprocess.run([*ssh,shlex.quote(c['existing_tools']+'/python3.13')+' -'],input=s,text=True,capture_output=True,timeout=30)
if p.returncode:print(p.stderr);raise SystemExit(p.returncode)
(b/'last-progress.json').write_text(p.stdout)
print(p.stdout)
