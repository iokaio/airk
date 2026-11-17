# SPDX-License-Identifier: Apache-2.0
import pathlib,json,sqlite3,hashlib
base=pathlib.Path(__file__).resolve().parent
receipt=json.loads((base/'windows-debug-pre14-adoption-result-r19.json').read_text(encoding='utf-8-sig'))
case=next(c for c in receipt['cases'] if c.get('preserved_owner'));fixture=pathlib.Path(case['preserved_owner'])
j=json.loads((fixture/'install/legacy-maintenance.json').read_text());before=json.loads((fixture/'install'/('legacy-maintenance-cycle-'+j['operation']+'-'+str(j['cycle'])+'.json')).read_text())
path=(fixture/'state/control/service-1.4.sqlite3').resolve(strict=True)
with sqlite3.connect(path.as_uri()+'?mode=ro',uri=True) as db:
 db.execute('PRAGMA query_only=ON')
 counts={name:db.execute('SELECT COUNT(*) FROM '+name).fetchone()[0] for name in ['service_jobs','service_requests','service_plans','service_lifecycle','service_background','service_setup_pending','service_watch','index_operation_receipts']}
 rows=[]
 for id,kind,stage,payload in db.execute('SELECT id,kind,stage,payload FROM index_operation_receipts ORDER BY id'):
  assert len(payload)<=65536
  rows.append({'id':id,'kind':kind,'stage':stage,'raw_sha256':hashlib.sha256(payload).hexdigest(),'payload':json.loads(payload)})
result={'schema':1,'fixture':str(fixture),'operation':j['operation'],'phase':j['phase'],'before':{k:before[k] for k in ['jobs','requests','unfinished','retained_intents']},'current_counts':counts,'current_operation_receipts':rows,'bound_plans':j['adoption_plans']}
assert len(json.dumps(result).encode())<=1024*1024
(base/'preserved-adoption-r19-continuation-delta.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'before':result['before'],'current_counts':counts,'receipts':[{'id':r['id'],'kind':r['kind'],'stage':r['stage']}for r in rows]}))
