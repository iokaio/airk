# SPDX-License-Identifier: Apache-2.0
import os,pathlib,subprocess,json,hashlib,tarfile,time,resource,platform,importlib.util,sys,shutil,tomllib
sys.dont_write_bytecode=True
run=pathlib.Path(__file__).resolve().parent
assert run.parent==pathlib.Path('/Users/tylerjensen/airk-builds') and run.name.startswith('p7-native-')
assert platform.system()=='Darwin' and platform.machine()=='arm64'
os.umask(0o077); resource.setrlimit(resource.RLIMIT_NOFILE,(4096,resource.getrlimit(resource.RLIMIT_NOFILE)[1]))
record=json.loads((run/'snapshot.json').read_text());source=run/'source'
assert hashlib.file_digest((run/'source.tar.gz').open('rb'),'sha256').hexdigest()==record['archive_sha256']
known_generated={'scripts/__pycache__/build-service-runtime.cpython-313.pyc','scripts/__pycache__/int8-policy.cpython-313.pyc'}
def verify():
 actual={str(p.relative_to(source)):hashlib.file_digest(p.open('rb'),'sha256').hexdigest() for p in source.rglob('*') if p.is_file() and not p.is_relative_to(source/'.git') and str(p.relative_to(source)) not in known_generated}
 assert actual==record['public_files'],'source manifest changed'
 return {'files':len(actual),'unchanged':True,'identified_generated_exclusions':sorted(known_generated)}
delta=json.loads((run/'overlay.json').read_text())
if hashlib.file_digest((source/delta['path']).open('rb'),'sha256').hexdigest()==delta['after_sha256']:
 record['public_files'][delta['path']]=delta['after_sha256']
verify()
delta=json.loads((run/'overlay.json').read_text());body=(run/'combined_reads.final.rs').read_bytes()
assert hashlib.sha256(body).hexdigest()==delta['after_sha256']
assert record['public_files'][delta['path']] in (delta['before_sha256'],delta['after_sha256'])
(source/delta['path']).write_bytes(body);record['public_files'][delta['path']]=delta['after_sha256']
spec=importlib.util.spec_from_file_location('builder',source/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
record['native_source']=builder.source_identity()
(run/'snapshot-r2.json').write_text(json.dumps({'native_source':record['native_source'],'public_files':record['public_files'],'delta':delta,'known_generated':sorted(known_generated)},indent=2))
before=verify()
cache=pathlib.Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/target')
tools=pathlib.Path('/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools')
prepared=pathlib.Path('/Users/tylerjensen/airk-builds/p6f-focused-final-r2-20261104.Ijzrqk/prepared-tools')
private=run/'private-census-r2';private.mkdir(mode=0o700)
env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',CARGO_BUILD_JOBS='1',CARGO_NET_OFFLINE='true',MACOSX_DEPLOYMENT_TARGET='13.3',PYTHONUTF8='1',CARGO_TARGET_DIR=str(run/'target-r2'),AIRK_BUNDLED_MODEL_DIR=str(cache/'local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(cache/'local-onnx-runtime'),AIRK_TEST_MODEL_DIR=str(cache/'local-vector-model/minilm-l6-v2-s8-v1'),AIRK_PRIVATE_CENSUS_BASE=str(private))
env['PATH']=str(prepared/'bin')+':'+str(tools)+':'+str(pathlib.Path.home()/'.cargo/bin')+':'+env['PATH']
steps=[]
def step(name,cmd):
 (run/'phase-r2.txt').write_text(name); start=time.monotonic()
 with (run/('r2-'+name+'.log')).open('x') as f:result=subprocess.run(cmd,cwd=source,env=env,stdout=f,stderr=subprocess.STDOUT)
 row={'name':name,'command':cmd,'exit_code':result.returncode,'seconds':round(time.monotonic()-start,2)};steps.append(row);(run/'steps-r2.json').write_text(json.dumps(steps,indent=2));print(json.dumps(row),flush=True)
 return result.returncode
target=run/'target-r2'
assert not target.exists() and target.parent.resolve()==run and not run.is_symlink()
subprocess.run(['/bin/cp','-cR',str(cache),str(target)],check=True)
assert target.resolve()==run/'target-r2' and target.stat().st_uid==os.getuid() and not target.is_symlink()
packages=[tomllib.loads(p.read_text())['package']['name'] for p in (source/'crates').glob('*/Cargo.toml')]
clean=['cargo','clean','--offline','--target-dir',str(target)]
for package in packages:clean+=['-p',package]
assert step('clean-owned-workspace-packages',clean)==0
step('fmt',['cargo','fmt','--all','--check'])
for package,filters in [('airk-core',['combined_reads','verification']),('airk-kernel',['combined_reads','workbench::combined','locations','verification']),('airk-mcp',['combined','verification']),('airk-cli',['combined_frontend'])]:
 for filter in filters:step(package+'-'+filter.replace('::','-'),['cargo','test','-p',package,'--lib','--no-default-features','--locked','--offline',filter,'--','--test-threads=1'])
step('clippy',['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings'])
# Build artifacts live outside source so the public snapshot remains exact.
code=step('native-build',['python3','scripts/build-service-runtime.py','--profile','release','--destination',str(run/'runtime-r2')])
if code==0:
 env['AIRK_CATALOG_CANDIDATE_BINARY']=str(run/'runtime-r2/AIRK.app/Contents/MacOS/airk')
 step('actual-combined',['cargo','test','-p','airk-cli','--test','combined_reads','--features','embedding-worker','--locked','--offline','--','--test-threads=1','--nocapture'])
after=verify();report={'native_source':record['native_source'],'source_before':before,'source_after':after,'steps':steps,'status':'passed' if all(s['exit_code']==0 for s in steps) else 'failed','not_run':['install','editor','release signing/notarization','Linux qualification','full workspace tests']}
(run/'qualification-r2.json').write_text(json.dumps(report,indent=2));(run/'phase-r2.txt').write_text('complete');print(json.dumps(report),flush=True)
