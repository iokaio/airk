# SPDX-License-Identifier: Apache-2.0
import os,pathlib,subprocess,json,hashlib,tarfile,time,resource,platform,importlib.util
run=pathlib.Path(__file__).resolve().parent
assert run.parent==pathlib.Path('/Users/tylerjensen/airk-builds') and run.name.startswith('p7-native-')
assert platform.system()=='Darwin' and platform.machine()=='arm64'
os.umask(0o077); resource.setrlimit(resource.RLIMIT_NOFILE,(4096,resource.getrlimit(resource.RLIMIT_NOFILE)[1]))
record=json.loads((run/'snapshot.json').read_text()); archive=run/'source.tar.gz'
assert hashlib.file_digest(archive.open('rb'),'sha256').hexdigest()==record['archive_sha256']
source=run/'source'; source.mkdir(mode=0o700)
with tarfile.open(archive) as t:t.extractall(source,filter='data')
def verify():
 actual={str(p.relative_to(source)):hashlib.file_digest(p.open('rb'),'sha256').hexdigest() for p in source.rglob('*') if p.is_file() and not p.is_relative_to(source/'.git')}
 assert actual==record['public_files'],'source manifest changed'
 return {'files':len(actual),'unchanged':True}
before=verify()
subprocess.run(['git','init','--quiet'],cwd=source,check=True)
subprocess.run(['git','add','--force','--all'],cwd=source,check=True)
spec=importlib.util.spec_from_file_location('builder',source/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
assert builder.source_identity()==record['native_source'],'native source input identity mismatch'
cache=pathlib.Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/target')
tools=pathlib.Path('/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools')
prepared=pathlib.Path('/Users/tylerjensen/airk-builds/p6f-focused-final-r2-20261104.Ijzrqk/prepared-tools')
private=run/'private-census';private.mkdir(mode=0o700)
env=dict(os.environ,CARGO_BUILD_JOBS='1',CARGO_NET_OFFLINE='true',MACOSX_DEPLOYMENT_TARGET='13.3',PYTHONUTF8='1',CARGO_TARGET_DIR=str(cache),AIRK_BUNDLED_MODEL_DIR=str(cache/'local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(cache/'local-onnx-runtime'),AIRK_TEST_MODEL_DIR=str(cache/'local-vector-model/minilm-l6-v2-s8-v1'),AIRK_PRIVATE_CENSUS_BASE=str(private))
env['PATH']=str(prepared/'bin')+':'+str(tools)+':'+str(pathlib.Path.home()/'.cargo/bin')+':'+env['PATH']
steps=[]
def step(name,cmd):
 (run/'phase.txt').write_text(name); start=time.monotonic()
 with (run/(name+'.log')).open('x') as f:result=subprocess.run(cmd,cwd=source,env=env,stdout=f,stderr=subprocess.STDOUT)
 row={'name':name,'command':cmd,'exit_code':result.returncode,'seconds':round(time.monotonic()-start,2)};steps.append(row);(run/'steps.json').write_text(json.dumps(steps,indent=2));print(json.dumps(row),flush=True)
 return result.returncode
step('fmt',['cargo','fmt','--all','--check'])
for package,filters in [('airk-core',['combined_reads','verification']),('airk-kernel',['combined_reads','workbench::combined','locations','verification']),('airk-mcp',['combined','verification']),('airk-cli',['combined_frontend'])]:
 for filter in filters:step(package+'-'+filter.replace('::','-'),['cargo','test','-p',package,'--lib','--no-default-features','--locked','--offline',filter,'--','--test-threads=1'])
step('clippy',['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings'])
# Build artifacts live outside source so the public snapshot remains exact.
code=step('native-build',['python3','scripts/build-service-runtime.py','--profile','release','--destination',str(run/'runtime')])
if code==0 and (run/'actual-approved.json').is_file():
 env['AIRK_TEST_RUNTIME']=str(run/'runtime');env['AIRK_TEST_BINARY']=str(run/'runtime/AIRK.app/Contents/MacOS/airk')
 step('actual-combined',['cargo','test','-p','airk-cli','--test','combined_reads','--locked','--offline','--','--test-threads=1','--nocapture'])
after=verify();report={'native_source':record['native_source'],'source_before':before,'source_after':after,'steps':steps,'status':'passed' if all(s['exit_code']==0 for s in steps) else 'failed','not_run':['install','editor','release signing/notarization','Linux qualification','full workspace tests']}
(run/'qualification.json').write_text(json.dumps(report,indent=2));(run/'phase.txt').write_text('complete');print(json.dumps(report),flush=True)
