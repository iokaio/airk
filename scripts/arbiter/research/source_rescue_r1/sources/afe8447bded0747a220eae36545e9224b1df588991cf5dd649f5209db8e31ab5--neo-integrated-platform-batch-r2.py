# SPDX-License-Identifier: Apache-2.0
# Prepared only. Root must review exact packet before executing; this preparation never runs Cargo.
from pathlib import Path
import os,sys,json,time,hashlib,stat,subprocess,importlib.util,resource,shutil
from datetime import datetime,timezone
C={'base': '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1', 'source': '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/source', 'out': '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/platform-integrated-dda749cb-r1', 'target': '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/source/target', 'commit': 'dda749cbdf77b3e2106003ff10a683ec3aec3202', 'raw_c7': {'sha256': 'b2ec68bd5157e3f72ba0f1ca9a7d695070b4dcd9b73308cdbdd222059a9454b6', 'files': 1928}, 'tree': 'dcc3a2c4afd876e58b8272d440e143fc96366dc0', 'manifest': '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/transfer/bridge-dda749cb-r1/source-bridge-manifest.json', 'manifest_sha256': '515a03c9236d262f95c19bfa39c5c139e08395669f2abdb465362560e4205d94', 'static_environment': '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/static-environment-r1.json', 'static_sha256': 'e811e5facb90d2f919dd18ae78326ca05e1497e59e004c562a6e36616897d9fa', 'python': '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13', 'node': '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/node-v24.15.0-darwin-arm64/bin/node', 'commands': [{'name': 'clippy', 'argv': ['/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13', '-B', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/source/scripts/dev/receipt_runner.py', '--groups', 'native,harness,fixtures', '--tier', 'T3', '--out', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/platform-integrated-dda749cb-r1/receipts', '--target-dir', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/source/target', '--fixture-base', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/platform-integrated-dda749cb-r1/fixtures/clippy', '--node', '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/node-v24.15.0-darwin-arm64/bin/node', '--static-environment', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/static-environment-r1.json', 'neo-dda749cb-clippy-r1', 'cargo', 'clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--timings', '--', '-D', 'warnings']}, {'name': 'workspace-tests', 'argv': ['/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13', '-B', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/source/scripts/dev/receipt_runner.py', '--groups', 'native,harness,fixtures', '--tier', 'T3', '--out', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/platform-integrated-dda749cb-r1/receipts', '--target-dir', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/source/target', '--fixture-base', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/platform-integrated-dda749cb-r1/fixtures/workspace-tests', '--node', '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/node-v24.15.0-darwin-arm64/bin/node', '--static-environment', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/static-environment-r1.json', 'neo-dda749cb-workspace-tests-r1', 'cargo', 'test', '--workspace', '--all-features', '--locked', '--timings', '--tests', '--', '--test-threads=2']}, {'name': 'doctests', 'argv': ['/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13', '-B', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/source/scripts/dev/receipt_runner.py', '--groups', 'native,harness,fixtures', '--tier', 'T3', '--out', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/platform-integrated-dda749cb-r1/receipts', '--target-dir', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/source/target', '--fixture-base', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/platform-integrated-dda749cb-r1/fixtures/doctests', '--node', '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/node-v24.15.0-darwin-arm64/bin/node', '--static-environment', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/static-environment-r1.json', 'neo-dda749cb-doctests-r1', 'cargo', 'test', '--doc', '--workspace', '--all-features', '--locked', '--timings', '--', '--test-threads=2']}], 'cleanup': ['cargo', 'clean', '--manifest-path', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/source/Cargo.toml', '--target-dir', '/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/source/target']}

def actor():
 assert os.getuid()==501==os.geteuid()==os.stat('/dev/console').st_uid and os.getuid()!=0
 return {'uid':os.getuid(),'console_uid':os.stat('/dev/console').st_uid}
def sha(q):
 with Path(q).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def load(name,path):
 s=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
def fresh(q,value):
 actor()
 with q.open('x') as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def git(*args):return subprocess.check_output(['git','--no-optional-locks',*args],cwd=C['source'])
def preflight():
 actor();source=Path(C['source']);base=Path(C['base'])
 for q in (base,source,base/'evidence'):
  assert q.lstat().st_uid==501 and q.resolve()==q and q.is_dir()
 assert git('rev-parse','HEAD').decode().strip()==C['commit']
 assert git('rev-parse','HEAD^{tree}').decode().strip()==C['tree']
 assert not git('status','--porcelain=v1','--untracked-files=all')
 assert sha(C['manifest'])==C['manifest_sha256']
 manifest=json.loads(Path(C['manifest']).read_text())
 for row in manifest['files']:
  q=source/row['path'];s=q.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and sha(q)==row['sha256']
 assert load('actual_native_identity',source/'scripts/build-service-runtime.py').source_identity()==C['raw_c7']
 assert sha(C['static_environment'])==C['static_sha256']
 return source,base

def main():
 source,base=preflight();out=Path(C['out']);target=Path(C['target'])
 assert not out.exists() and not target.exists();os.umask(0o077);out.mkdir(mode=0o700);(out/'logs').mkdir();(out/'receipts').mkdir();(out/'fixtures').mkdir()
 soft,hard=resource.getrlimit(resource.RLIMIT_NOFILE);assert hard==resource.RLIM_INFINITY or hard>=4096
 resource.setrlimit(resource.RLIMIT_NOFILE,(max(soft,4096),hard))
 owner=load('actual_darwin_owner',source/'scripts/provision-static-onnx-runtime.py')
 env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',PYTHONNOUSERSITE='1',CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0',CARGO_TARGET_DIR=str(target),AIRK_BUNDLED_MODEL_DIR=str(base/'inputs/bundled-model-v3'),AIRK_TEST_MODEL_DIR=str(base/'inputs/bundled-model-v3/minilm-l6-v2-s8-v3'),RUST_TEST_THREADS='2')
 env['PATH']=str(Path(C['python']).parent)+':'+str(Path(C['node']).parent)+':'+str(Path.home()/'.cargo/bin')+':/usr/bin:/bin:/usr/sbin:/sbin'
 start=time.monotonic();records=[];cleanup=None;all_settled=True;error=None;passed=False
 initial={'schema':'airk.neo-integrated-platform-start/1','utc':datetime.now(timezone.utc).isoformat(),'actor':actor(),'config':C,'status':'started','passed':False,'descriptor_before':[soft,hard],'descriptor_after':list(resource.getrlimit(resource.RLIMIT_NOFILE)),'owner_script_sha256':sha(source/'scripts/provision-static-onnx-runtime.py'),'receipt_runner_sha256':sha(source/'scripts/dev/receipt_runner.py'),'original_dirty_status_sha256':hashlib.sha256(subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','-z','--untracked-files=normal'],cwd=Path.home()/'code/airk')).hexdigest()}
 fresh(out/'batch-start.json',initial)
 try:
  for command in C['commands']:
   actor();name=command['name'];fixture=out/'fixtures'/name;fixture.mkdir(mode=0o700)
   fresh(out/(name+'-outer-start.json'),{'utc':datetime.now(timezone.utc).isoformat(),'actor':actor(),'argv':command['argv'],'deadline_seconds':10800,'status':'started','passed':False})
   all_settled=False  # Unknown until THIS launch returns its actual ownership witness.
   result=owner.run_owned(command['argv'],source,out/'logs'/(name+'.log'),10800,env)
   natural=result.get('owned_descendants_settled') is True and not result.get('owned_group_signaled',False)
   ok=owner.successful(result) and natural
   record={'phase':name,'result':result,'natural_unix_settlement':natural,'passed':ok};records.append(record);fresh(out/(name+'-outer-result.json'),record)
   all_settled=result.get('owned_descendants_settled') is True
   print(json.dumps({'phase':name,'passed':ok,'result':result}),flush=True)
   if not ok:raise RuntimeError('failed '+name+'; remaining phases stopped, no replay')
  passed=True
 except BaseException as failure:
  error={'type':type(failure).__name__,'message':str(failure)};passed=False
 finally:
  actor()
  # Runner receipts/start records/logs and owned command timing copies are
  # already outside target. Retain any remaining timing bytes before clean.
  timing=target/'cargo-timings'
  if timing.is_dir():shutil.copytree(timing,out/'retained-cargo-timings',symlinks=False)
  if all_settled:
   assert target==source/'target' and target.resolve()==target and target.parent.resolve()==source.resolve()
   if target.exists():assert target.is_dir() and not target.is_symlink() and target.lstat().st_uid==501
   cleanup=owner.run_owned(C['cleanup'],source,out/'logs/cargo-clean.log',600,env)
   clean_ok=owner.successful(cleanup) and cleanup.get('owned_descendants_settled') is True and not cleanup.get('owned_group_signaled',False)
   fresh(out/'cargo-clean-result.json',{'result':cleanup,'passed':clean_ok,'target':str(target),'target_exists_after':target.exists()})
   passed=passed and clean_ok
  else:
   error={'type':'UnsettledOwnedGroup','message':'Target cleanup withheld; actual owned descendants unproven.','primary_error':error};passed=False
  current=load('after_native_identity',source/'scripts/build-service-runtime.py').source_identity()
  dirty=hashlib.sha256(subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','-z','--untracked-files=normal'],cwd=Path.home()/'code/airk')).hexdigest()
  unchanged=current==C['raw_c7'] and not git('status','--porcelain=v1','--untracked-files=all') and dirty==initial['original_dirty_status_sha256']
  passed=passed and unchanged
  fresh(out/'batch-result.json',{'schema':'airk.neo-integrated-platform-result/1','utc':datetime.now(timezone.utc).isoformat(),'actor':actor(),'passed':passed,'elapsed_seconds':time.monotonic()-start,'phases':records,'cleanup':cleanup,'error':error,'inputs_unchanged':bool(unchanged),'raw_c7_after':current,'original_dirty_status_sha256':dirty,'scope':'Actual Darwin compiled component gates only. No larger-model fit, signing, installed journey or Windows observer qualification.'})
 if not passed:raise SystemExit(1)

if __name__=='__main__':main()
