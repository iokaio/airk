# SPDX-License-Identifier: Apache-2.0
import json,subprocess,shlex
from pathlib import Path
b=Path(__file__).resolve().parent;c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text(encoding='utf-8-sig'));r=json.loads((b/'active.json').read_text())['remote']
names=['gui-sign-phase.txt','gui-sign-owner.log','evidence/gui-sign-command.log','evidence/gui-sign-child-result.json','evidence/signed-gui-installed-runtime.log','evidence/signed-gui-service-clients.log','gui-sign-result.json']
code='from pathlib import Path;import json;r=Path('+repr(r)+');print(json.dumps({n:(r/n).read_text()[-14000:] for n in '+repr(names)+' if (r/n).exists()}))'
p=subprocess.run(['ssh','-i',c['identity_path'],*c['ssh_options'],c['user']+'@'+c['host'],shlex.quote(c['existing_tools']+'/python3.13')+' -'],input=code,text=True,capture_output=True,timeout=30)
print(p.stdout);print(p.stderr);raise SystemExit(p.returncode)
