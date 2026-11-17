# SPDX-License-Identifier: Apache-2.0
import pathlib,json,hashlib,subprocess,os,time,resource,importlib.util
run=pathlib.Path(__file__).resolve().parent; source=run/'source'; record=json.loads((run/'snapshot.json').read_text());delta=json.loads((run/'overlay.json').read_text())
assert (run/'qualification.json').is_file(),'initial gates/build must finish before test-only overlay'
assert all(hashlib.file_digest((source/n).open('rb'),'sha256').hexdigest()==h for n,h in record['public_files'].items())
test=source/delta['path'];assert hashlib.file_digest(test.open('rb'),'sha256').hexdigest()==delta['before_sha256']
body=(run/'combined_reads.final.rs').read_bytes();assert hashlib.sha256(body).hexdigest()==delta['after_sha256'];test.write_bytes(body)
record['public_files'][delta['path']]=delta['after_sha256']
spec=importlib.util.spec_from_file_location('builder',source/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder);native=builder.source_identity()
(run/'final-source-overlay.json').write_text(json.dumps({'native_source':native,'delta':delta,'public_files':record['public_files'],'only_test_file_changed':True},indent=2))
os.umask(0o077);resource.setrlimit(resource.RLIMIT_NOFILE,(4096,resource.getrlimit(resource.RLIMIT_NOFILE)[1]))
cache=pathlib.Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/target');tools='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools'
env=dict(os.environ,CARGO_BUILD_JOBS='1',CARGO_NET_OFFLINE='true',MACOSX_DEPLOYMENT_TARGET='13.3',PYTHONUTF8='1',CARGO_TARGET_DIR=str(cache),AIRK_BUNDLED_MODEL_DIR=str(cache/'local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(cache/'local-onnx-runtime'),AIRK_TEST_MODEL_DIR=str(cache/'local-vector-model/minilm-l6-v2-s8-v1'),AIRK_PRIVATE_CENSUS_BASE=str(run/'private-census'))
env['PATH']=tools+':'+str(pathlib.Path.home()/'.cargo/bin')+':'+env['PATH'];steps=[]
def step(name,cmd):
 (run/'actual-phase.txt').write_text(name);start=time.monotonic()
 with (run/(name+'.log')).open('x') as f:r=subprocess.run(cmd,cwd=source,env=env,stdout=f,stderr=subprocess.STDOUT)
 row={'name':name,'command':cmd,'exit_code':r.returncode,'seconds':round(time.monotonic()-start,2)};steps.append(row);print(json.dumps(row),flush=True);return r.returncode
code=step('native-final-build',['python3','scripts/build-service-runtime.py','--profile','release','--destination',str(run/'runtime-final')])
if code==0:
 env['AIRK_CATALOG_CANDIDATE_BINARY']=str(run/'runtime-final/AIRK.app/Contents/MacOS/airk')
 step('actual-final-combined',['cargo','test','-p','airk-cli','--test','combined_reads','--features','embedding-worker','--locked','--offline','--','--test-threads=1','--nocapture'])
assert all(hashlib.file_digest((source/n).open('rb'),'sha256').hexdigest()==h for n,h in record['public_files'].items())
assert builder.source_identity()==native
report={'native_source':native,'test_only_delta':delta,'steps':steps,'source_before_after_unchanged':True,'status':'passed' if code==0 and all(x['exit_code']==0 for x in steps) else 'failed','not_run':['install','editor','Developer ID signing/notarization','Linux qualification','full workspace tests']}
(run/'actual-qualification.json').write_text(json.dumps(report,indent=2));(run/'actual-phase.txt').write_text('complete');print(json.dumps(report),flush=True)
