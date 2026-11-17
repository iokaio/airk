# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,hashlib,json,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2]
NODE=ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe'
OUT=ROOT/'artifacts/advanced-split'
names=('packages/admin/src','packages/ui/src','packages/ui/generated','packages/vscode/src','packages/vscode/scripts')
paths=[p for name in names for p in (ROOT/name).rglob('*') if p.is_file()]
paths.append(ROOT/'crates/airk-mcp/src/help/content.json')
def hashes(items):return {p.relative_to(ROOT).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(items)}
report={'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_before':hashes(paths),'commands':[],'scope':'Frontend asset generation and exact rebuilding; not installed/native transport/final VSIX qualification.'}
commands=[('admin-build',[str(NODE),'packages/admin/build.mjs']),('bootstrap-owner-restore',[str(NODE),'packages/vscode/scripts/build-bootstrap.cjs']),('editor-bundles',[str(NODE),'packages/vscode/scripts/build-explorer.cjs']),('admin-exact-check',[str(NODE),'packages/admin/build.mjs','--check'])]
for name,command in commands:
    start=time.perf_counter();r=subprocess.run(command,cwd=ROOT,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=60)
    log=OUT/f'p6-final-assets-{name}.log';log.write_bytes(r.stdout)
    report['commands'].append({'name':name,'argv':command,'cwd':str(ROOT),'exit_code':r.returncode,'elapsed_seconds':time.perf_counter()-start,'log':log.relative_to(ROOT).as_posix()})
    print(name,r.returncode,flush=True)
    if r.returncode:break
report['source_after']=hashes(paths);report['changed_sources']=[p for p,h in report['source_before'].items() if report['source_after'].get(p)!=h]
outputs=[p for name in ('crates/airk-cli/admin-assets','packages/vscode/out') for p in (ROOT/name).rglob('*') if p.is_file() and p.suffix in {'.js','.css','.json','.html'}]
report['output_sha256']=hashes(outputs);report['finished_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
report['passed']=len(report['commands'])==len(commands) and all(c['exit_code']==0 for c in report['commands']) and not report['changed_sources']
(OUT/'p6-final-assets.json').write_text(json.dumps(report,indent=2)+'\n')
sys.exit(0 if report['passed'] else 1)
