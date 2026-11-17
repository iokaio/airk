# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,subprocess,tempfile,time
ROOT=Path(__file__).resolve().parents[2];NODE=ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe'
source=ROOT/'packages/admin/src/folderPicker.tsx';test=ROOT/'packages/ui/test/outerManagement.dom.test.mjs'
before=hashlib.sha256(source.read_bytes()).hexdigest()
text=test.read_text()
plugin="""plugins:[{name:'pre-guard-picker-replay',setup(plugin){plugin.onLoad({filter:/folderPicker\\.tsx$/},async args=>({contents:(await readFile(args.path,'utf8')).replaceAll('if (!accepted()) return;',''),loader:'tsx',resolveDir:fileURLToPath(new URL('../../admin/src/',import.meta.url))}));}},sharedAdminStylePlugin(),{"""
text=text.replace('plugins:[sharedAdminStylePlugin(),{',plugin)
assert 'pre-guard-picker-replay' in text
with tempfile.NamedTemporaryFile(mode='w',suffix='.picker-replay.tmp.mjs',prefix='outerManagement-',dir=test.parent,delete=False) as temporary:
    temporary.write(text);name=Path(temporary.name)
try:
    argv=[str(NODE),'--test',str(name)];start=time.perf_counter();r=subprocess.run(argv,cwd=ROOT,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=30)
finally:name.unlink()
output=r.stdout.decode('utf-8','replace')
report={'scope':'Negative replay removes only three post-await guard statements in an in-memory esbuild load; live production bytes unchanged.','argv':argv,'elapsed_seconds':time.perf_counter()-start,'exit_code':r.returncode,'source_before_sha256':before,'source_after_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'output':output}
report['passed']=r.returncode!=0 and 'closed old initialization cannot browse its initial path or parent under the replacement session' in output and report['source_before_sha256']==report['source_after_sha256']
(ROOT/'artifacts/advanced-split/p6-picker-negative-replay.json').write_text(json.dumps(report,indent=2)+'\n');print('Expected original-chain failure observed:',report['passed']);raise SystemExit(0 if report['passed'] else 1)
