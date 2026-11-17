# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,importlib.util,json,re,subprocess,time
ROOT=Path(__file__).resolve().parents[2];NODE=ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe';OUT=ROOT/'artifacts/advanced-split'
names=('packages/ui/src/helpMarkdown.ts','packages/ui/src/helpSchema.ts','packages/ui/src/pilotContract.ts','packages/vscode/src/adminUiContract.ts')
def sources():return {n:hashlib.sha256((ROOT/n).read_bytes()).hexdigest() for n in names}
s=importlib.util.spec_from_file_location('packager',ROOT/'scripts/package-extension.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
def outputs():return {p.relative_to(ROOT).as_posix():p.read_bytes() for p in [*(ROOT/'crates/airk-cli/admin-assets').iterdir(),*[ROOT/'packages/vscode/out'/n for n in (*m.BUNDLES,'uiPilotRuntime.js')]] if p.is_file()}
before=outputs();old=before['packages/vscode/out/uiPilotRuntime.js']
try:m.verify_pilot_runtime(old)
except ValueError:stale_refused=True
else:stale_refused=False
report={'scope':'Four exact EOF blank-line removals only; focused helper/contract semantics and producer source seals. Previous receipts retain original identities.','source_before':sources(),'stale_original_runtime_refused':stale_refused,'commands':[]}
commands=[('affected-semantics',[str(NODE),'--test','packages/ui/test/helpPresentation.test.mjs','packages/ui/test/adminUiContract.test.mjs','packages/ui/test/pilotNativeCases.test.mjs']),('admin-producer',[str(NODE),'packages/admin/build.mjs']),('editor-producer',[str(NODE),'packages/vscode/scripts/build-explorer.cjs']),('admin-exact-check',[str(NODE),'packages/admin/build.mjs','--check'])]
for name,argv in commands:
 start=time.perf_counter();r=subprocess.run(argv,cwd=ROOT,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=60);output=r.stdout.decode('utf-8','replace');log=OUT/f'p6-eof-reseal-{name}.log';log.write_bytes(r.stdout)
 selected=re.search(r'(?:# tests|ℹ tests) (\d+)',output);count=int(selected[1]) if selected else None
 passed=r.returncode==0 and (count is not None and count>0 if name=='affected-semantics' else True)
 report['commands'].append({'name':name,'argv':argv,'cwd':str(ROOT),'exit_code':r.returncode,'selected_count':count,'elapsed_seconds':time.perf_counter()-start,'passed':passed,'log':log.relative_to(ROOT).as_posix()});print(name,passed,count,flush=True)
 if not passed:break
after=outputs();report['source_after']=sources();report['output_before_sha256']={n:hashlib.sha256(b).hexdigest() for n,b in before.items()};report['output_after_sha256']={n:hashlib.sha256(b).hexdigest() for n,b in after.items()}
report['changed_outputs']=[n for n in set(before)|set(after) if before.get(n)!=after.get(n)]
pilot='packages/vscode/out/uiPilotRuntime.js';report['pilot_body_identical']=before[pilot].split(b'\n',2)[2]==after[pilot].split(b'\n',2)[2]
m.verify_pilot_runtime(after[pilot]);report['new_runtime_owner_verified']=True
report['passed']=len(report['commands'])==len(commands) and all(c['passed'] for c in report['commands']) and report['source_before']==report['source_after'] and stale_refused and report['changed_outputs']==[pilot] and report['pilot_body_identical']
(OUT/'p6-eof-reseal.json').write_text(json.dumps(report,indent=2)+'\n');print('Final:',report['passed']);raise SystemExit(0 if report['passed'] else 1)
