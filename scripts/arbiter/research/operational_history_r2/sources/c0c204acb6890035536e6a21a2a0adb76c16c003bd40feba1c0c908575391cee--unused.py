# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,os,pathlib,resource,subprocess,tarfile,time
run=pathlib.Path(__file__).resolve().parent;source=run/'source'
assert run.parent==pathlib.Path('/Users/tylerjensen/airk-builds') and run.name.startswith('p8-native-')
assert json.loads((run/'qualification-slash.json').read_text())['status']=='passed'
resource.setrlimit(resource.RLIMIT_NOFILE,(4096,resource.getrlimit(resource.RLIMIT_NOFILE)[1]))
record=json.loads((run/'snapshot.json').read_text())
for row in json.loads((run/'final-overlay.json').read_text())['files']:record['files'][row['path']]=row['after']
for row in json.loads((run/'slash-overlay.json').read_text())['files']:record['files'][row['path']]=row['after']
delta=json.loads((run/'unused-overlay.json').read_text())
assert hashlib.sha256((run/'unused-overlay.tar.gz').read_bytes()).hexdigest()==delta['archive_sha256']
for name,digest in record['files'].items():assert hashlib.sha256((source/name).read_bytes()).hexdigest()==digest,name
for row in delta['files']:
 assert row['path'].startswith('crates/') and record['files'][row['path']]==row['before']
with tarfile.open(run/'unused-overlay.tar.gz') as archive:
 assert {m.name for m in archive.getmembers()}=={r['path'] for r in delta['files']}
 for member in archive.getmembers():assert member.isfile() and (source/member.name).resolve().is_relative_to(source)
 archive.extractall(source,filter='data')
for row in delta['files']:record['files'][row['path']]=row['after']
def verify():
 for name,digest in record['files'].items():assert hashlib.sha256((source/name).read_bytes()).hexdigest()==digest,name
 return {'files':len(record['files']),'unchanged':True}
before=verify();spec=importlib.util.spec_from_file_location('builder',source/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder);identity=builder.source_identity()
cache=pathlib.Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/target')
env=dict(os.environ,CARGO_BUILD_JOBS='1',CARGO_NET_OFFLINE='true',MACOSX_DEPLOYMENT_TARGET='13.3',CARGO_TARGET_DIR=str(cache),AIRK_BUNDLED_MODEL_DIR=str(cache/'local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(cache/'local-onnx-runtime'),AIRK_TEST_MODEL_DIR=str(cache/'local-vector-model/minilm-l6-v2-s8-v1'),AIRK_PRIVATE_CENSUS_BASE=str(run/'private-census'))
env['PATH']='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools:'+str(pathlib.Path.home()/'.cargo/bin')+':'+env.get('PATH','')
steps=[]
for name,cmd in [('unused-core',['cargo','test','-p','airk-core','--lib','--locked','--offline','combined_reads','--','--test-threads=1']),('unused-transport',['cargo','test','-p','airk-cli','--test','combined_reads','--locked','--offline','--','--test-threads=1']),('unused-clippy',['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings'])]:
 (run/'phase.txt').write_text(name);start=time.monotonic()
 with (run/(name+'.log')).open('x') as log:result=subprocess.run(cmd,cwd=source,env=env,stdout=log,stderr=subprocess.STDOUT)
 steps.append({'name':name,'command':cmd,'exit_code':result.returncode,'seconds':round(time.monotonic()-start,2)});print(json.dumps(steps[-1]),flush=True)
report={'schema':'airk.p8.neo-r3/1','native_source':identity,'source_before':before,'source_after':verify(),'delta':delta,'steps':steps,'status':'passed' if all(s['exit_code']==0 for s in steps) else 'failed','retained_initial_suites':['core','workbench','mcp'],'not_run':['installed editor','release signing/notarization','full Mac workspace tests','Mac performance']}
(run/'qualification-r3.json').write_text(json.dumps(report,indent=2));(run/'phase.txt').write_text('r3-complete');print(json.dumps(report),flush=True)
