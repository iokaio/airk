# SPDX-License-Identifier: Apache-2.0
"""Create future R4 owner pins only after reviewed usage delta and external compile proof exist."""
from pathlib import Path
import hashlib,json
B=Path(__file__).resolve().parent;W=B/'p10-worktree'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
read=lambda p:json.loads(Path(p).read_text(encoding='utf-8'))
OUT=B/'p14-package-editor-continuation-r4-prepared.json'
assert not OUT.exists()
old=read(B/'p14-package-editor-continuation-r3-prepared.json');proposal=B/'p14-usage-editor-pick-proposed-r1/manifest.json'
assert sha(proposal)=='5aaecc17fe9b1fd0d2469687103e5b5f24cb068fd775f3f3438b0debf6898cf2'
m=read(proposal);changes={str((W/row['path']).resolve()):row for row in m['files']}
proof=read(B/'p14-installed-editor-compile-r3.json')
assert proof['status']=='passed' and proof['actual_exit_code']==0 and proof['error'] is None
changed_output=str((W/'packages/vscode/out/test/installedEditor.js').resolve())
assert changed_output in {str(Path(p).resolve()) for p in proof['emission']['emitted']}
pins={}
for p,value in old['pins'].items():
 if str(Path(p).resolve()) in changes:
  row=changes[str(Path(p).resolve())];assert value==row['base_sha256'] and sha(p)==row['proposed_sha256'];value=row['proposed_sha256']
 elif str(Path(p).resolve())==changed_output:value=sha(p)
 else:assert sha(p)==value,p
 pins[p]=value
proof=read(B/'p14-installed-editor-compile-r3.json')
assert proof['status']=='passed' and proof['production_source_unchanged'] is True and proof['production_out_unchanged'] is True
assert proof['vsix_sha256']=='49170b51765048e438dfd9516a35e4884d7667efe5ad2652d02303caf22f4c90'
for p in [proposal,B/'p14-usage-editor-pick-proposed-r1/editor-usage.patch',B/'p14-usage-editor-pick-proposed-r1/evidence-review-r1.json',B/'p14-editor-three-completed-adoption-r1.json',B/'p14-editor-cleanup-native-installed-cUDfG8-r1.json',B/'p14-installed-editor-compile-r3.json',B/'p14-installed-editor-compile-r3-before.json',B/'p14-installed-editor-compile-proof-r3.py',B/'p14-package-editor-continuation-r3.json',B/'p14-usage-editor-pick-live-adoption-r1.json',B/'p14-editor-ui-node-tests-r3.receipt.json',B/'p14-editor-ui-node-tests-r3.log',B/'p14-installed-editor-compile-r3-emit.json',B/'p14-installed-editor-compile-r3.log']:
 pins[str(p.resolve())]=sha(p)
for row in m['files']:pins[str((W/row['path']).resolve())]=row['proposed_sha256']
owner=B/'p14-package-editor-continuation-r4.py'
result={'owner_sha256':sha(owner),'pins':pins,'scope':'Only fresh usage+untrusted, then fresh managed legacy fixture and sixth scenario; exact first3 scenario evidence adopted; no builds or packaging.'}
with OUT.open('x',encoding='utf-8') as f:json.dump(result,f,indent=2);f.write('\n')
print(json.dumps({'prepared':str(OUT),'sha256':sha(OUT),'owner_sha256':sha(owner),'pin_count':len(pins)}))
