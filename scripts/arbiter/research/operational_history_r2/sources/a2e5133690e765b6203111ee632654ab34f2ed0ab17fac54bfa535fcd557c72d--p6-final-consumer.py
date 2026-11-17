# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,importlib.util,json,subprocess,tempfile,time
ROOT=Path(__file__).resolve().parents[2];OUT=ROOT/'artifacts/advanced-split';NODE=ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe'
s=importlib.util.spec_from_file_location('packager',ROOT/'scripts/package-extension.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
report={'scope':'Exact compiled frontend bundle consumer and bootstrap owner gates; no positive final VSIX/native publication claim.','checks':[]}
pilot=(ROOT/'packages/vscode/out/uiPilotRuntime.js').read_bytes();m.verify_pilot_runtime(pilot)
report['checks'].append({'name':'actual-pilot-source-seal','passed':True,'sha256':hashlib.sha256(pilot).hexdigest()})
with tempfile.TemporaryDirectory(prefix='airk-p6-bundles-') as d:
    m.stage_bundles(ROOT/'packages/vscode/out',Path(d),'10.29.8')
    report['checks'].append({'name':'actual-six-frontend-bundles','passed':True,'sha256':{name:hashlib.sha256((ROOT/'packages/vscode/out'/name).read_bytes()).hexdigest() for name in m.BUNDLES}})
with tempfile.TemporaryDirectory(prefix='airk-p6-notices-') as d:
    m.stage_model_notices(Path(d))
    report['checks'].append({'name':'actual-five-public-model-notices','passed':True,'sha256':{name:hashlib.sha256((Path(d)/name).read_bytes()).hexdigest() for name in m.MODEL_NOTICE_FILES}})
for flag,name,expected in [('--verify-stdin','provenance',0),('--verify-release-stdin','release',1)]:
    path=ROOT/f'packages/vscode/out/airkctl/bootstrap/{name}.js';data=path.read_bytes();argv=[str(NODE),str(ROOT/'packages/vscode/scripts/build-bootstrap.cjs'),flag];t=time.perf_counter()
    r=subprocess.run(argv,cwd=ROOT,input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
    report['checks'].append({'name':name,'argv':argv,'elapsed_seconds':time.perf_counter()-t,'sha256':hashlib.sha256(data).hexdigest(),'exit_code':r.returncode,'stdout':r.stdout.decode(),'stderr':r.stderr.decode(),'passed':r.returncode==0 if expected==0 else r.returncode!=0,'expected':'Verified exact source bundle' if expected==0 else 'Refusal: actual qualified four-profile native publication capture absent'})
report['passed']=all(c['passed'] for c in report['checks']);(OUT/'p6-final-consumer.json').write_text(json.dumps(report,indent=2)+'\n');print([(c['name'],c['passed']) for c in report['checks']]);raise SystemExit(0 if report['passed'] else 1)
