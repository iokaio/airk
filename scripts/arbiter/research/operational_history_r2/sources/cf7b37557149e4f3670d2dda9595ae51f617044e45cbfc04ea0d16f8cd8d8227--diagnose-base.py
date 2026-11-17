# SPDX-License-Identifier: Apache-2.0
import os,pathlib,subprocess,time,hashlib,json,datetime
run=pathlib.Path(__file__).resolve().parent;source=run/'source';record=json.loads((run/'snapshot-ownership-final.json').read_text());known=set(record['known_generated'])
def verify():
 actual={str(p.relative_to(source)):hashlib.file_digest(p.open('rb'),'sha256').hexdigest() for p in source.rglob('*') if p.is_file() and not p.is_relative_to(source/'.git') and str(p.relative_to(source)) not in known};assert actual==record['public_files']
verify();private=run/'private-census-r8-base-diagnostic';private.mkdir(mode=0o700);logs=run/'r8-captured-fixture-logs';logs.mkdir(mode=0o700)
env=dict(os.environ,CARGO_BUILD_JOBS='1',CARGO_NET_OFFLINE='true',MACOSX_DEPLOYMENT_TARGET='13.3',CARGO_TARGET_DIR=str(run/'target-r2'),AIRK_PRIVATE_CENSUS_BASE=str(private),AIRK_CATALOG_CANDIDATE_BINARY=str(run/'runtime-ownership-final/AIRK.app/Contents/MacOS/airk'),AIRK_PROCESS_DIAGNOSTICS='1',PYTHONDONTWRITEBYTECODE='1',RUST_BACKTRACE='1')
env['PATH']='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools:'+str(pathlib.Path.home()/'.cargo/bin')+':'+env['PATH']
command=['cargo','test','-p','airk-cli','--test','combined_reads','--features','embedding-worker','--locked','--offline','actual_native_cli_and_mcp_preserve_shared_index_scope_and_complete_results','--','--exact','--nocapture','--test-threads=1']
opened={};start=datetime.datetime.now(datetime.timezone.utc).isoformat()
with (run/'r8-basecase.log').open('x') as output:
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
verify();report={'scope':'ONE exact basecase diagnostics only; no source/index state copied','native_source':record['native_source'],'started_utc':start,'finished_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exit_code':child.returncode,'command':command,'captured_log_files':[p.name for p in logs.iterdir()],'source_before_after_unchanged':True}
(run/'r8-diagnostic.json').write_text(json.dumps(report,indent=2));print(json.dumps(report),flush=True)
