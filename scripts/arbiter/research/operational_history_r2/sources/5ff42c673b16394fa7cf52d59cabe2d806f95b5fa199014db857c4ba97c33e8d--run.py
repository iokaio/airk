# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,pathlib,platform,resource,subprocess,tarfile,time
run=pathlib.Path(__file__).resolve().parent
assert platform.system()=='Darwin' and platform.machine()=='arm64'
assert run.parent==pathlib.Path('/Users/tylerjensen/airk-builds') and run.name.startswith('p8-native-')
os.umask(0o077)
resource.setrlimit(resource.RLIMIT_NOFILE,(4096,resource.getrlimit(resource.RLIMIT_NOFILE)[1]))
record=json.loads((run/'snapshot.json').read_text())
assert hashlib.sha256((run/'source.tar.gz').read_bytes()).hexdigest()==record['archive_sha256']
source=run/'source';source.mkdir(mode=0o700,exist_ok=True);assert not any(source.iterdir())
with tarfile.open(run/'source.tar.gz') as archive:
 for member in archive.getmembers():
  assert member.isfile() and not pathlib.Path(member.name).is_absolute()
  assert (source/member.name).resolve().is_relative_to(source)
 archive.extractall(source,filter='data')
def verify():
 for name,digest in record['files'].items():
  assert hashlib.sha256((source/name).read_bytes()).hexdigest()==digest,name
 return {'files':len(record['files']),'unchanged':True}
before=verify()
subprocess.run(['git','init','--quiet'],cwd=source,check=True)
subprocess.run(['git','add','--all'],cwd=source,check=True)
cache=pathlib.Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/target')
private=run/'private-census';private.mkdir(mode=0o700)
env=dict(os.environ,CARGO_BUILD_JOBS='1',CARGO_NET_OFFLINE='true',MACOSX_DEPLOYMENT_TARGET='13.3',PYTHONUTF8='1',CARGO_TARGET_DIR=str(cache),AIRK_BUNDLED_MODEL_DIR=str(cache/'local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(cache/'local-onnx-runtime'),AIRK_TEST_MODEL_DIR=str(cache/'local-vector-model/minilm-l6-v2-s8-v1'),AIRK_PRIVATE_CENSUS_BASE=str(private))
env['PATH']='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools:'+str(pathlib.Path.home()/'.cargo/bin')+':'+env.get('PATH','')
steps=[]
for name,command in [
 ('fmt',['cargo','fmt','--all','--check']),
 ('core',['cargo','test','-p','airk-core','--lib','--locked','--offline','combined_reads','--','--test-threads=1']),
 ('workbench',['cargo','test','-p','airk-kernel','--lib','--locked','--offline','workbench::combined','--','--test-threads=1']),
 ('mcp',['cargo','test','-p','airk-mcp','--lib','--locked','--offline','combined','--','--test-threads=1']),
 ('transport',['cargo','test','-p','airk-cli','--test','combined_reads','--locked','--offline','recommendations_keep','--','--test-threads=1']),
 ('clippy',['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings']),
]:
 (run/'phase.txt').write_text(name);start=time.monotonic()
 with (run/(name+'.log')).open('x') as output:
  result=subprocess.run(command,cwd=source,env=env,stdout=output,stderr=subprocess.STDOUT)
 row={'name':name,'command':command,'exit_code':result.returncode,'seconds':round(time.monotonic()-start,2)}
 steps.append(row);(run/'steps.json').write_text(json.dumps(steps,indent=2));print(json.dumps(row),flush=True)
report={'schema':'airk.p8.neo/1','platform':platform.platform(),'archive_sha256':record['archive_sha256'],'source_before':before,'source_after':verify(),'steps':steps,'status':'passed' if all(row['exit_code']==0 for row in steps) else 'failed','not_run':['installed editor','release signing/notarization','full workspace Mac tests','Mac performance']}
(run/'qualification.json').write_text(json.dumps(report,indent=2));(run/'phase.txt').write_text('complete');print(json.dumps(report),flush=True)
