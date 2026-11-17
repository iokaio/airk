# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,re,subprocess,time
ROOT=Path(__file__).resolve().parents[2];NODE=ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe';OUT=ROOT/'artifacts/advanced-split'
paths=[ROOT/p for p in ('packages/admin/build.mjs','packages/vscode/scripts/build-explorer.cjs','packages/ui/test/codegenStrings.test.mjs','packages/ui/test/generatedHelp.dom.test.mjs','crates/airk-mcp/src/help/content.json','packages/ui/generated/help-manifest.json')]
def hashes():return {p.relative_to(ROOT).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
commands=[('string-byte-positive-negative',[str(NODE),'--test','packages/ui/test/codegenStrings.test.mjs']),
 ('admin-producer',[str(NODE),'packages/admin/build.mjs']),('editor-producer',[str(NODE),'packages/vscode/scripts/build-explorer.cjs']),
 ('admin-exact-check',[str(NODE),'packages/admin/build.mjs','--check']),
 ('actual-generated-host-dom',[str(NODE),'--test','packages/ui/test/generatedHelp.dom.test.mjs'])]
report={'scope':'Producer-only escaped-string correction: full native object/all-string UTF8 equality, original whitespace defect reproduction, actual generated modules in both browsers, asset seals. No native/installed transport claim.','before_sha256':hashes(),'commands':[]}
for name,argv in commands:
 start=time.perf_counter();r=subprocess.run(argv,cwd=ROOT,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=60);output=r.stdout.decode('utf-8','replace');log=OUT/f'p6-codegen-reseal-{name}.log';log.write_bytes(r.stdout)
 selected=re.search(r'(?:# tests|ℹ tests) (\d+)',output);count=int(selected[1]) if selected else None
 passed=r.returncode==0 and (count is not None and count>0 if name in ('string-byte-positive-negative','actual-generated-host-dom') else True)
 report['commands'].append({'name':name,'argv':argv,'cwd':str(ROOT),'exit_code':r.returncode,'selected_count':count,'elapsed_seconds':time.perf_counter()-start,'passed':passed,'log':log.relative_to(ROOT).as_posix()});print(name,passed,count,flush=True)
 if not passed:break
report['after_sha256']=hashes();report['passed']=len(report['commands'])==len(commands) and all(c['passed'] for c in report['commands']) and report['before_sha256']==report['after_sha256']
if report['passed']:
 files=list((ROOT/'crates/airk-cli/admin-assets').iterdir())+[ROOT/'packages/vscode/out'/name for name in ('explorerWebview.js','workbenchWebview.js','explorerAssistance.js','uiPilotRuntime.js','help-manifest.json')]
 report['output_sha256']={p.relative_to(ROOT).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in files if p.is_file()}
 js=[p for p in files if p.suffix=='.js'];report['physical_whitespace']={p.relative_to(ROOT).as_posix():{'trailing_lines':[i+1 for i,line in enumerate(p.read_bytes().splitlines()) if re.search(rb'[\t ]+$',line)],'extra_eof_blank':p.read_bytes().endswith(b'\n\n')} for p in js}
 report['passed']=not any(x['trailing_lines'] or x['extra_eof_blank'] for x in report['physical_whitespace'].values())
(OUT/'p6-codegen-reseal.json').write_text(json.dumps(report,indent=2)+'\n');print('Final:',report['passed']);raise SystemExit(0 if report['passed'] else 1)
