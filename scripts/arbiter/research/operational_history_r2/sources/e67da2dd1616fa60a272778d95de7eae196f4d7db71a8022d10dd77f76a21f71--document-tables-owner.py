# SPDX-License-Identifier: Apache-2.0
"""One bounded unchanged-helper invocation; preserve all results and owned state."""
import hashlib,json,os,signal,subprocess,time
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source3c6e81c';evidence=run/'evidence3c';python=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools/python3.13'
assert run==Path.home()/'airk-builds/explorer-notary-1812-ee5ad77-20261107'
commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip();assert commit=='3c6e81cf5911751aac8dbe6767ea77abab11d206'
names=['scripts/check-document-tables.py','scripts/check-service-clients.py'];identity={}
for n in names:
 data=(source/n).read_bytes();assert data==subprocess.check_output(['git','show','HEAD:'+n],cwd=source);identity[n]=hashlib.sha256(data).hexdigest()
original=source/'artifacts/neo/signed-notarized-3c/runtime/airk-build.json';assert json.loads(original.read_text())['source']=={'sha256':'51536a705cf9cbabe4947cacddf43a8cb29b0af8bad4e23ca7f293aa6dd347fa','files':978}
started=time.monotonic();env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1');timed=False;hard=False
with (evidence/'document-tables-observer.log').open('x') as log:
 child=subprocess.Popen([str(python),str(run/'document-tables-observer.py')],cwd=source,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
 try:code=child.wait(timeout=720)
 except subprocess.TimeoutExpired:
  timed=True;child.send_signal(signal.SIGINT) # Allow the original ExitStack/Fixture.close to run.
  try:code=child.wait(timeout=190)
  except subprocess.TimeoutExpired:
   hard=True;os.killpg(child.pid,signal.SIGTERM)
   try:code=child.wait(timeout=5)
   except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);code=child.wait(timeout=5)
report={'schema':'airk.document-table-single-run-owner/1','commit':commit,'original_helper_sha256':identity,'exit_code':code,'observation_timeout_seconds':720,'original_cleanup_grace_seconds':190,'timed_out':timed,'hard_owned_group_cleanup':hard,'elapsed_seconds':round(time.monotonic()-started,3),'rust_build_or_warmup':False,'target_absent':not (source/'target').exists() and not (run/'target').exists()}
(evidence/'document-tables-owner-result.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
