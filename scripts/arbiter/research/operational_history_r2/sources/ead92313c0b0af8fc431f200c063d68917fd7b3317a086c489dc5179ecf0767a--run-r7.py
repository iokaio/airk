# SPDX-License-Identifier: Apache-2.0
import os,pathlib,subprocess,json,hashlib,time,resource,platform,sys,tomllib,tarfile,importlib.util,re
sys.dont_write_bytecode=True
run=pathlib.Path(__file__).resolve().parent;source=run/'source';record=json.loads((run/'snapshot-retry-final.json').read_text());known=set(record['known_generated'])
assert run.parent==pathlib.Path('/Users/tylerjensen/airk-builds') and platform.machine()=='arm64'
os.umask(0o077);resource.setrlimit(resource.RLIMIT_NOFILE,(4096,resource.getrlimit(resource.RLIMIT_NOFILE)[1]))
def verify():
 actual={str(p.relative_to(source)):hashlib.file_digest(p.open('rb'),'sha256').hexdigest() for p in source.rglob('*') if p.is_file() and not p.is_relative_to(source/'.git') and str(p.relative_to(source)) not in known}
 assert actual==record['public_files'],'source manifest changed'
 return {'files':len(actual),'unchanged':True,'identified_generated_exclusions':sorted(known)}
assert (run/'helper-preflight.json').is_file(),'frozen-helper preflight must complete before regression delta'
verify()
delta=json.loads((run/'regression-overlay.json').read_text());archive=run/'regression-overlay.tar.gz'
assert hashlib.file_digest(archive.open('rb'),'sha256').hexdigest()==delta['archive_sha256']
for row in delta['changes']:
 assert row['path'].startswith('crates/airk-cli/tests/')
 if row['before_sha256'] is None:assert row['path'] not in record['public_files'] and not (source/row['path']).exists()
 else:assert record['public_files'][row['path']]==row['before_sha256']
with tarfile.open(archive) as t:
 assert set(t.getnames())=={r['path'] for r in delta['changes']}
 t.extractall(source,filter='data')
for row in delta['changes']:
 assert hashlib.file_digest((source/row['path']).open('rb'),'sha256').hexdigest()==row['after_sha256']
 os.utime(source/row['path'],None) # exact new bytes must invalidate Cargo's mtime-based workspace fingerprints
 record['public_files'][row['path']]=row['after_sha256']
spec=importlib.util.spec_from_file_location('builder',source/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
record['native_source']=builder.source_identity();record['final_overlay']=delta
(run/'snapshot-ownership-final.json').write_text(json.dumps(record,indent=2));before=verify()
cache=pathlib.Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/target')
tools=pathlib.Path('/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools')
prepared=pathlib.Path('/Users/tylerjensen/airk-builds/p6f-focused-final-r2-20261104.Ijzrqk/prepared-tools')
private=run/'private-census-r7';private.mkdir(mode=0o700)
env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',CARGO_BUILD_JOBS='1',CARGO_NET_OFFLINE='true',MACOSX_DEPLOYMENT_TARGET='13.3',PYTHONUTF8='1',CARGO_TARGET_DIR=str(run/'target-r2'),AIRK_BUNDLED_MODEL_DIR=str(cache/'local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(cache/'local-onnx-runtime'),AIRK_TEST_MODEL_DIR=str(cache/'local-vector-model/minilm-l6-v2-s8-v1'),AIRK_PRIVATE_CENSUS_BASE=str(private))
env['PATH']=str(prepared/'bin')+':'+str(tools)+':'+str(pathlib.Path.home()/'.cargo/bin')+':'+env['PATH']
steps=[]
def step(name,cmd):
 (run/'phase-r7.txt').write_text(name); start=time.monotonic()
 with (run/('r7-'+name+'.log')).open('x') as f:result=subprocess.run(cmd,cwd=source,env=env,stdout=f,stderr=subprocess.STDOUT)
 row={'name':name,'command':cmd,'exit_code':result.returncode,'seconds':round(time.monotonic()-start,2)};steps.append(row);(run/'steps-r7.json').write_text(json.dumps(steps,indent=2));print(json.dumps(row),flush=True)
 return result.returncode
step('fmt',['cargo','fmt','--all','--check'])
step('clippy',['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings'])
for i,command in enumerate(delta['regression_commands']):
 assert command[:2]==['cargo','test']
 step('unix-read-retry-regressions-'+str(i+1),command)
code=step('native-build',['python3','scripts/build-service-runtime.py','--profile','debug','--destination',str(run/'runtime-ownership-final')])
if code==0:
 env['AIRK_CATALOG_CANDIDATE_BINARY']=str(run/'runtime-ownership-final/AIRK.app/Contents/MacOS/airk')
 actual_code=step('actual-combined',['cargo','test','-p','airk-cli','--test','combined_reads','--features','embedding-worker','--locked','--offline','--','--test-threads=1','--nocapture'])
 if actual_code==0:
  counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (run/'r7-actual-combined.log').read_text())
  assert counts and int(counts[-1][0])>=7 and int(counts[-1][1])==0,'actual native test catalog did not execute seven cases'
after=verify();assert builder.source_identity()==record['native_source']
report={'scope':'final Unix test ownership+regression correction canonical DEBUG correctness/platform ownership qualification only;97unaffected focusedtests retainedfromR3; no latency/performance/release qualification','native_source':record['native_source'],'source_before':before,'source_after':after,'delta':delta,'steps':steps,'status':'passed' if code==0 and all(x['exit_code']==0 for x in steps) else 'failed','not_run':['full workspace tests','install','editor','Developer ID signing/notarization','Linux qualification']}
(run/'qualification-ownership-final.json').write_text(json.dumps(report,indent=2));(run/'phase-r7.txt').write_text('complete');print(json.dumps(report),flush=True)
