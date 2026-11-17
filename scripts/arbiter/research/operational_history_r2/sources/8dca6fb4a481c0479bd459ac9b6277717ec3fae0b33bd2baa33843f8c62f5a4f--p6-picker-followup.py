# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,re,subprocess,time
ROOT=Path(__file__).resolve().parents[2];NODE=ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe';OUT=ROOT/'artifacts/advanced-split'
paths=[p for directory in ('packages/admin/src','packages/admin/test','packages/ui/src','packages/ui/test','packages/vscode/src/explorerWebview') for p in (ROOT/directory).rglob('*') if p.is_file()]
def hashes():return {p.relative_to(ROOT).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(paths)}
commands=[('admin-types',[str(NODE),'packages/admin/node_modules/typescript/bin/tsc','-p','packages/admin/tsconfig.json']),('editor-webview-types',[str(NODE),'packages/vscode/node_modules/typescript/bin/tsc','-p','packages/vscode/tsconfig.webview.json']),
 ('affected-callbacks',[str(NODE),'--test',*[f'packages/admin/test/{name}.test.mjs' for name in ('r04Outcomes','r19Race','r20Lifecycle','helpSemantics')]]),
 ('shared-closure-navigation',[str(NODE),'--test','packages/ui/test/managementBundle.test.mjs','packages/ui/test/managementNavigation.test.mjs']),
 ('editor-boundaries',[str(NODE),'--test','packages/vscode/out/test/explorerBoundaries.test.js']),
 ('picker-actual-dom',[str(NODE),'--test','packages/ui/test/outerManagement.dom.test.mjs','packages/admin/test/r22Picker.dom.test.mjs'])]
report={'scope':'Affected picker lifetime/session correction only; prior436-source and10-DOM receipts retain original identities. No Cargo/native/installed transport claim.','before_sha256':hashes(),'commands':[]}
for name,argv in commands:
 start=time.perf_counter();r=subprocess.run(argv,cwd=ROOT,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=60);output=r.stdout.decode('utf-8','replace');log=OUT/f'p6-picker-followup-{name}.log';log.write_bytes(r.stdout)
 count=re.search(r'(?:# tests|ℹ tests) (\d+)',output);selected=int(count[1]) if count else None
 passed=r.returncode==0 and (selected is None if name.endswith('types') else selected is not None and selected>0)
 report['commands'].append({'name':name,'argv':argv,'cwd':str(ROOT),'exit_code':r.returncode,'selected_count':selected,'elapsed_seconds':time.perf_counter()-start,'passed':passed,'log':log.relative_to(ROOT).as_posix()});print(name,passed,selected,flush=True)
 if not passed:break
report['after_sha256']=hashes();report['passed']=len(report['commands'])==len(commands) and all(c['passed'] for c in report['commands']) and report['before_sha256']==report['after_sha256']
(OUT/'p6-picker-followup.json').write_text(json.dumps(report,indent=2)+'\n');raise SystemExit(0 if report['passed'] else 1)
