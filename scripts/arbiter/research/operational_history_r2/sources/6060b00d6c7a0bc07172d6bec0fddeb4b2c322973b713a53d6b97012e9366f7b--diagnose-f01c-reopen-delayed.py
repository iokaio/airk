# SPDX-License-Identifier: Apache-2.0
# Private diagnostic on original fixture only. Raw native errors never enter public reports.
import importlib.util,json,tempfile,time
from pathlib import Path
spec=importlib.util.spec_from_file_location('bench',Path('scripts/benchmark-real-corpora.py'));B=importlib.util.module_from_spec(spec);spec.loader.exec_module(B)
meta=json.loads(Path('artifacts/optimizations/f01c-original-smoke.json').read_text())
location=json.loads(Path('artifacts/optimizations/f01c-reopen-diagnostic-location.json').read_text())
owner=Path(location['owner'])
raw={'kind':'original_fixture_private_diagnostic','observations':[]}
f=B.clients().Fixture(Path(meta['runtime']),owner,job_timeout=180)
def capture(stage,ready=False):
 with B.Session(f,'probe') as session:
  for ordinal,(name,args) in enumerate([('status',{}),('search',{'query':'configuration','mode':'vector','limit':1,'max_bytes':4096,'response_mode':'full'}),('search',{'query':'configuration','mode':'hybrid','limit':1,'max_bytes':4096,'response_mode':'full'}),('search',{'query':'configuration','mode':'vector','limit':1,'max_bytes':4096,'response_mode':'full'}),('search',{'query':'configuration','mode':'hybrid','limit':1,'max_bytes':4096,'response_mode':'full'}),('diagnostics',{'response_mode':'full'})]):
   started=time.perf_counter();value=session.request('tools/call',{'name':name,'arguments':args})
   raw['observations'].append({'stage':stage,'ordinal':ordinal,'name':name,'mode':args.get('mode'),'elapsed_ms':round((time.perf_counter()-started)*1000,3),'envelope':value})
primary=None
try:
 f.start();data=owner/'data';f.register('probe',Path(meta['airk']),data)
 capture('retained_restart_immediate')
 time.sleep(1)
 capture('retained_restart_independent_session_after_1s')

except BaseException as error:
 primary=error;raw['failure_code']=B.failure_code(error)
finally:
 raw['cleanup_failed']=B.close_owned_fixture(f,primary)
 (owner/'private-delayed-raw.json').write_text(json.dumps(raw,indent=2)+'\n')
 Path('artifacts/optimizations/f01c-reopen-delayed-diagnostic-location.json').write_text(json.dumps({'owner':str(owner),'private_report':str(owner/'private-delayed-raw.json')},indent=2)+'\n')
summary=[]
for row in raw['observations']:
 env=row['envelope'];text=env.get('content',[{}])[0].get('text','')
 try:value=json.loads(text)
 except ValueError:value={}
 summary.append({'stage':row['stage'],'ordinal':row['ordinal'],'name':row['name'],'mode':row['mode'],'elapsed_ms':row['elapsed_ms'],'is_error':env.get('isError',False),'index_state':value.get('index_state'),'hits':len(value['hits']) if isinstance(value.get('hits'),list) else None,'error_text_digest':B.hashlib.sha256(text.encode()).hexdigest() if env.get('isError') else None})
print(json.dumps({'kind':raw['kind'],'failure_code':raw.get('failure_code'),'cleanup_failed':raw['cleanup_failed'],'cancellation':raw.get('cancellation'),'observations':summary},indent=2))
