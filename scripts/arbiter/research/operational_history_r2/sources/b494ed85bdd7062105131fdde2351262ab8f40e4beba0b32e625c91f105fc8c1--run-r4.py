# SPDX-License-Identifier: Apache-2.0
import os,pathlib,subprocess,json,hashlib,time,resource,platform,sys,tomllib
sys.dont_write_bytecode=True
run=pathlib.Path(__file__).resolve().parent;source=run/'source';record=json.loads((run/'snapshot-r2.json').read_text());known=set(record['known_generated'])
assert run.parent==pathlib.Path('/Users/tylerjensen/airk-builds') and platform.machine()=='arm64'
os.umask(0o077);resource.setrlimit(resource.RLIMIT_NOFILE,(4096,resource.getrlimit(resource.RLIMIT_NOFILE)[1]))
def verify():
 actual={str(p.relative_to(source)):hashlib.file_digest(p.open('rb'),'sha256').hexdigest() for p in source.rglob('*') if p.is_file() and not p.is_relative_to(source/'.git') and str(p.relative_to(source)) not in known}
 assert actual==record['public_files'],'source manifest changed'
 return {'files':len(actual),'unchanged':True,'identified_generated_exclusions':sorted(known)}
before=verify()
cache=pathlib.Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/target')
tools=pathlib.Path('/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools')
prepared=pathlib.Path('/Users/tylerjensen/airk-builds/p6f-focused-final-r2-20261104.Ijzrqk/prepared-tools')
private=run/'private-census-r4';private.mkdir(mode=0o700)
env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',CARGO_BUILD_JOBS='1',CARGO_NET_OFFLINE='true',MACOSX_DEPLOYMENT_TARGET='13.3',PYTHONUTF8='1',CARGO_TARGET_DIR=str(run/'target-r2'),AIRK_BUNDLED_MODEL_DIR=str(cache/'local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(cache/'local-onnx-runtime'),AIRK_TEST_MODEL_DIR=str(cache/'local-vector-model/minilm-l6-v2-s8-v1'),AIRK_PRIVATE_CENSUS_BASE=str(private))
env['PATH']=str(prepared/'bin')+':'+str(tools)+':'+str(pathlib.Path.home()/'.cargo/bin')+':'+env['PATH']
steps=[]
def step(name,cmd):
 (run/'phase-r4.txt').write_text(name); start=time.monotonic()
 with (run/('r4-'+name+'.log')).open('x') as f:result=subprocess.run(cmd,cwd=source,env=env,stdout=f,stderr=subprocess.STDOUT)
 row={'name':name,'command':cmd,'exit_code':result.returncode,'seconds':round(time.monotonic()-start,2)};steps.append(row);(run/'steps-r4.json').write_text(json.dumps(steps,indent=2));print(json.dumps(row),flush=True)
 return result.returncode
target=run/'target-r2'
assert target.resolve()==run/'target-r2' and target.stat().st_uid==os.getuid() and not target.is_symlink() and (target/'CACHEDIR.TAG').is_file()
packages=[tomllib.loads(p.read_text())['package']['name'] for p in (source/'crates').glob('*/Cargo.toml')]
clean=['cargo','clean','--release','--offline','--target-dir',str(target)]
for package in packages:clean+=['-p',package]
assert step('clean-owned-release-workspace-packages',clean)==0
code=step('native-build',['python3','scripts/build-service-runtime.py','--profile','release','--destination',str(run/'runtime-r4')])
after=verify();report={'scope':'fresh canonical paired release staging only; scoped97 gates retained fromR3; actual/clippy pending Unix test overlay','native_source':record['native_source'],'source_before':before,'source_after':after,'steps':steps,'status':'passed' if code==0 else 'failed'}
(run/'staging-r4.json').write_text(json.dumps(report,indent=2));(run/'phase-r4.txt').write_text('complete');print(json.dumps(report),flush=True)
