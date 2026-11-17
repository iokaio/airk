# SPDX-License-Identifier: Apache-2.0
"""Root-only package/final gates around separately observed whole editor preflight."""
import argparse,hashlib,importlib.util,json,os,subprocess,sys,time
from pathlib import Path
B=Path(__file__).resolve().parent;W=B/'p10-worktree';R=B/'p14-runtime-final-r3';T=B/'p10-target'
EXPECTED={'sha256':'237f896933ad59a4075f9395fd53fcb92e376946faf1fc01036c139fc0533c0f','files':960}
CAT=B/'p14-current-windows-qualification-catalog-r1.json';CAT_SHA='56a5ff701de5e11f017f2e18457f4e79e5ff46ed4cacd8822bb9d262df11de37'
h=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
read=lambda p:json.loads(Path(p).read_text(encoding='utf-8'))
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('phase',choices=['package','final'])
 for n in ['producer','seal','editor']:
  p.add_argument('--'+n,type=Path,required=n!='editor');p.add_argument('--'+n+'-sha256',required=n!='editor')
 a=p.parse_args();record=B/('p14-current-windows-'+a.phase+'-r1.json');assert not record.exists()
 assert h(CAT)==CAT_SHA
 for path,sha in [(a.producer,a.producer_sha256),(a.seal,a.seal_sha256)]:assert h(path)==sha
 producer=read(a.producer);seal=read(a.seal)
 assert producer['exit_code']==producer['actual_exit_code']==0 and producer['validation_error'] is None
 assert producer['source_before']==producer['source_after']==EXPECTED and producer['source_unchanged'] is True
 producer_log=a.producer.with_name(a.producer.name.replace('.receipt.json','.log'));assert h(producer_log)==producer['log_sha256']
 builder=load('current_native_builder',W/'scripts/build-service-runtime.py');sealer=load('current_runtime_sealer',B/'p9-integrated-checks.py')
 baseline=read(B/'p14-installed-editor-compile-r4-before.json');compiled=read(B/'p14-installed-editor-compile-r4.json')
 assert compiled['status']=='passed' and compiled['native_source']==EXPECTED and compiled['production_source_unchanged'] is True and compiled['production_out_unchanged'] is True
 def product(root):return {p.relative_to(root).as_posix():h(p) for p in root.rglob('*') if p.is_file() and p.relative_to(root).parts[0]!='test'}
 def guard():
  assert builder.source_identity()==EXPECTED and sealer.seal_runtime(R,EXPECTED)==seal
  assert product(W/'packages/vscode/src')==baseline['production_sources'] and product(W/'packages/vscode/out')==baseline['production_outputs']
  assert h(a.producer)==a.producer_sha256 and h(a.seal)==a.seal_sha256 and h(CAT)==CAT_SHA
 guard()
 if a.phase=='final':
  assert a.editor and a.editor_sha256 and h(a.editor)==a.editor_sha256
  editor=read(a.editor);assert editor['status']=='passed' and editor.get('current_native_source',editor.get('native_source'))==EXPECTED
 else:assert not (W/'artifacts/p14-package-editor-r3').exists()
 env=os.environ.copy();env.update(PYTHONUTF8='1',AIRK_DEV_CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_PRIVATE_CENSUS_BASE=str(B/'p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02'))
 env['PATH']=str(B.parent/'tools/nextest')+os.pathsep+env['PATH']
 for key in ['AIRK_CATALOG_CANDIDATE_BINARY','AIRK_TEST_BINARY','AIRK_P10_RESOURCE_OUTPUT_DIR','AIRK_P11B_RESOURCE_OUTPUT_DIR']:env.pop(key,None)
 evidence=B/('p14-current-windows-'+a.phase+'-r1-evidence');evidence.mkdir(exist_ok=False)
 result={'status':'running','phase':a.phase,'native_source':EXPECTED,'producer_sha256':a.producer_sha256,'seal_sha256':a.seal_sha256,'editor_sha256':a.editor_sha256,'catalog_sha256':CAT_SHA,'steps':[]}
 def save():record.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
 save()
 try:
  for step in read(CAT)['commands'][a.phase]:
   guard();label='p14-current-'+step['name']+'-r1.log';log=B/label;assert not log.exists()
   started=time.time_ns();timings=T/'cargo-timings';before={p.name:h(p) for p in timings.glob('*.html')} if timings.exists() else {}
   code=subprocess.run([sys.executable,'-X','utf8',str(B/'p11-local-command.py'),label,*step['command']],cwd=W,env=env).returncode
   receipt=log.with_suffix('.receipt.json');v=read(receipt);row={'name':step['name'],'exit_code':code,'receipt':str(receipt),'receipt_sha256':h(receipt),'timings':[]}
   for timing in timings.glob('*.html'):
    if before.get(timing.name)!=h(timing) and timing.stat().st_mtime_ns>=started:
     copied=evidence/(step['name']+'-'+timing.name)
     with copied.open('xb') as f:f.write(timing.read_bytes())
     row['timings'].append({'path':str(copied),'sha256':h(copied)})
   if step['name']=='full-nextest':
    junit=W/'target/nextest/ci/junit.xml';assert junit.stat().st_mtime_ns>=started
    copied=evidence/'full-nextest.junit.xml'
    with copied.open('xb') as f:f.write(junit.read_bytes())
    row['junit']={'path':str(copied),'sha256':h(copied)}
   result['steps'].append(row);save()
   assert code==v['exit_code']==v['actual_exit_code']==0 and v['validation_error'] is None and v['source_before']==v['source_after']==EXPECTED
   assert h(log)==v['log_sha256'];guard()
  result['status']='passed';save()
 except BaseException as error:result['status']='failed';result['failure']=repr(error);save();raise
 print(json.dumps({'status':result['status'],'record':str(record),'sha256':h(record)}))
if __name__=='__main__':main()
