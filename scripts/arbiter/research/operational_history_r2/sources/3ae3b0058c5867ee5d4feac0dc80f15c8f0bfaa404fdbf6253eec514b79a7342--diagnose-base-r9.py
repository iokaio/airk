# SPDX-License-Identifier: Apache-2.0
import os,pathlib,subprocess,time,hashlib,json,datetime,tarfile,sys,importlib.util
sys.dont_write_bytecode=True
run=pathlib.Path(__file__).resolve().parent;source=run/'source';record=json.loads((run/'snapshot-ownership-final.json').read_text());known=set(record['known_generated'])
def verify():
 actual={str(p.relative_to(source)):hashlib.file_digest(p.open('rb'),'sha256').hexdigest() for p in source.rglob('*') if p.is_file() and not p.is_relative_to(source/'.git') and str(p.relative_to(source)) not in known};assert actual==record['public_files']
verify();delta=json.loads((run/'diagnostic-overlay.json').read_text());archive=run/'diagnostic-overlay.tar.gz'
assert hashlib.file_digest(archive.open('rb'),'sha256').hexdigest()==delta['archive_sha256']
for row in delta['changes']:assert record['public_files'][row['path']]==row['before_sha256']
with tarfile.open(archive) as t:
 assert set(t.getnames())=={r['path'] for r in delta['changes']};t.extractall(source,filter='data')
for row in delta['changes']:
 assert hashlib.file_digest((source/row['path']).open('rb'),'sha256').hexdigest()==row['after_sha256'];os.utime(source/row['path'],None);record['public_files'][row['path']]=row['after_sha256']
spec=importlib.util.spec_from_file_location('builder',source/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder);record['native_source']=builder.source_identity()
(run/'snapshot-r9.json').write_text(json.dumps(record,indent=2));verify();private=run/'private-census-r9-base-diagnostic';private.mkdir(mode=0o700);logs=run/'r9-captured-fixture-logs';logs.mkdir(mode=0o700)
env=dict(os.environ,CARGO_BUILD_JOBS='1',CARGO_NET_OFFLINE='true',MACOSX_DEPLOYMENT_TARGET='13.3',CARGO_TARGET_DIR=str(run/'target-r2'),AIRK_PRIVATE_CENSUS_BASE=str(private),AIRK_BUNDLED_MODEL_DIR='/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/target/local-vector-model',AIRK_BUNDLED_RUNTIME_DIR='/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/target/local-onnx-runtime',AIRK_TEST_MODEL_DIR='/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/target/local-vector-model/minilm-l6-v2-s8-v1',AIRK_PROCESS_DIAGNOSTICS='1',PYTHONDONTWRITEBYTECODE='1',RUST_BACKTRACE='1')
env['PATH']='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools:'+str(pathlib.Path.home()/'.cargo/bin')+':'+env['PATH']
native_logs=run/'r9-native-worker-diagnostics';native_logs.mkdir(mode=0o700);env['AIRK_NATIVE_DIAGNOSTIC_DIR']=str(native_logs)
steps=[]
def step(name,args):
 (run/'phase-r9.txt').write_text(name)
 with (run/('r9-'+name+'.log')).open('x') as f:r=subprocess.run(args,cwd=source,env=env,stdout=f,stderr=subprocess.STDOUT)
 row={'name':name,'exit_code':r.returncode,'command':args};steps.append(row);print(json.dumps(row),flush=True);return r.returncode
step('fmt',['cargo','fmt','--all','--check'])
step('clippy',['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings'])
assert step('native-build',['python3','scripts/build-service-runtime.py','--profile','debug','--destination',str(run/'runtime-r9-diagnostic')])==0
verify();env['AIRK_CATALOG_CANDIDATE_BINARY']=str(run/'runtime-r9-diagnostic/AIRK.app/Contents/MacOS/airk')
(run/'phase-r9.txt').write_text('ONLY-basecase-diagnostics')
command=['cargo','test','-p','airk-cli','--test','combined_reads','--features','embedding-worker','--locked','--offline','actual_native_cli_and_mcp_preserve_shared_index_scope_and_complete_results','--','--exact','--nocapture','--test-threads=1']
opened={};start=datetime.datetime.now(datetime.timezone.utc).isoformat()
with (run/'r9-basecase.log').open('x') as output:
 child=subprocess.Popen(command,cwd=source,env=env,stdout=output,stderr=subprocess.STDOUT)
 while child.poll() is None:
  for p in private.glob('*/*.log'):
   if p.name not in ['service.log','mcp.log'] or str(p) in opened:continue
   try:opened[str(p)]=(p.open('rb'),logs/(p.parent.name+'-'+p.name))
   except FileNotFoundError:pass
  time.sleep(.01)
 for input,target in opened.values():
  with target.open('xb') as output:output.write(input.read())
  input.close()
verify();report={'scope':'ONE exact basecase diagnostics only; no source/index state copied','native_source':record['native_source'],'started_utc':start,'finished_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exit_code':child.returncode,'command':command,'captured_log_files':[p.name for p in logs.iterdir()],'source_before_after_unchanged':True,'steps':steps,'delta':delta,'native_diagnostic_files':[p.name for p in native_logs.iterdir()]}
(run/'r9-diagnostic.json').write_text(json.dumps(report,indent=2));print(json.dumps(report),flush=True)

(run/'phase-r9.txt').write_text('complete')
