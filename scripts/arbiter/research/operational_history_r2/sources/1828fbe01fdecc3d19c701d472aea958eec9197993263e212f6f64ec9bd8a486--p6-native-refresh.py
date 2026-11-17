# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,re,subprocess,time
ROOT=Path(__file__).resolve().parents[2];NODE=ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe';OUT=ROOT/'artifacts/advanced-split'
paths=[ROOT/p for p in ('packages/contracts-gen/i3.schema.json','packages/contracts-gen/i3.ts','packages/vscode/src/generated/i3.d.ts','crates/airk-mcp/src/help/content.json','packages/ui/generated/help-manifest.json','fixtures/split/ui_pilot/native_reads.json','fixtures/split/ui_pilot/help_native.json','fixtures/split/ui_pilot/usage_native.json','fixtures/split/arbiter/public_answer_cases.json','packages/vscode/src/test/workbenchClientLifetime.test.ts')]
def hashes():return {p.relative_to(ROOT).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
commands=[('i3-contract-identity',['python','scripts/dev/contract_types.py','i3','--check']),
 ('admin-types',[str(NODE),'packages/admin/node_modules/typescript/bin/tsc','-p','packages/admin/tsconfig.json']),
 ('editor-host-types',[str(NODE),'packages/vscode/node_modules/typescript/bin/tsc','-p','packages/vscode']),
 ('editor-webview-types',[str(NODE),'packages/vscode/node_modules/typescript/bin/tsc','-p','packages/vscode/tsconfig.webview.json']),
 ('rust-native-pilot-cases',[str(NODE),'--test','packages/ui/test/pilotNativeCases.test.mjs']),
 ('rust-public-answer-consumer',[str(NODE),'--test','--test-name-pattern=A7','packages/vscode/out/test/workbenchClientLifetime.test.js']),
 ('restore-bootstrap-owner',[str(NODE),'packages/vscode/scripts/build-bootstrap.cjs']),
 ('admin-exact-asset-check',[str(NODE),'packages/admin/build.mjs','--check'])]
report={'scope':'Only affected fresh native capture consumers, generated contract identity and typechecks. No native/installed transport claim; no runtime Help/content bytes changed. Earlier436+10+89 retain original identities.','before_sha256':hashes(),'commands':[]}
for name,argv in commands:
 start=time.perf_counter();r=subprocess.run(argv,cwd=ROOT,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=60);output=r.stdout.decode('utf-8','replace');log=OUT/f'p6-native-refresh-{name}.log';log.write_bytes(r.stdout)
 selected=re.search(r'(?:# tests|ℹ tests) (\d+)',output);count=int(selected[1]) if selected else None
 passed=r.returncode==0 and (count is not None and count>0 if name in ('rust-native-pilot-cases','rust-public-answer-consumer') else True)
 report['commands'].append({'name':name,'argv':argv,'cwd':str(ROOT),'exit_code':r.returncode,'selected_count':count,'elapsed_seconds':time.perf_counter()-start,'passed':passed,'log':log.relative_to(ROOT).as_posix()});print(name,passed,count,flush=True)
 if not passed:break
report['after_sha256']=hashes();report['passed']=len(report['commands'])==len(commands) and all(c['passed'] for c in report['commands']) and report['before_sha256']==report['after_sha256']
(OUT/'p6-native-refresh.json').write_text(json.dumps(report,indent=2)+'\n');raise SystemExit(0 if report['passed'] else 1)
