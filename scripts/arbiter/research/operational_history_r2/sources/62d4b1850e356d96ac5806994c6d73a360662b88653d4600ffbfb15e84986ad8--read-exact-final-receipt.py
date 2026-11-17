# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,sqlite3,stat,urllib.parse
from pathlib import Path
run=Path(__file__).resolve().parent;fixture=run/'source3c6e81c/artifacts/editor-qualification/native-installed-QKyq61/same';control=fixture/'native-service/control';db=control/'service-1.4.sqlite3'
assert run==Path.home()/'airk-builds/explorer-notary-1812-ee5ad77-20261107' and control.resolve(strict=True)==control and db.resolve(strict=True)==db
s=db.lstat();assert stat.S_ISREG(s.st_mode) and not stat.S_ISLNK(s.st_mode) and s.st_uid==os.getuid()==501 and s.st_nlink==1 and s.st_size<=64*1024**2
expected={'operation_id':'f82d207db564023f2e837ca98998ba67','caller':'1158e7e12c5e7362318e5e3c2e1f2f1a','key':{'namespace':'d9ec441f4221fa1153dcb21bbd7de417','epoch':0,'nonce':'5e5636d56fca4b4087010afbe51bce5f'},'request_digest':'fc0f30a578d555ddd16f5f1aae86327f14bc65c9a04ffd68d815ad0e81dd1cd4'}
def hashes():
 result={}
 for p in [db,Path(str(db)+'-wal')]:
  if p.is_file():
   with p.open('rb') as f:result[p.name]=hashlib.file_digest(f,'sha256').hexdigest()
 return result
before=hashes();uri='file:'+urllib.parse.quote(str(db),safe='/')+'?mode=ro'
connection=sqlite3.connect(uri,uri=True,timeout=1);connection.execute('PRAGMA query_only=ON');connection.execute('PRAGMA trusted_schema=OFF')
query='SELECT stage,revision,substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1 AND kind=?2 LIMIT 2'
try:rows=connection.execute(query,(expected['operation_id'],'catalog_management_fenced')).fetchall()
finally:connection.close()
after=hashes();assert before==after and len(rows)<=1
report={'schema':'airk.neo-exact-offline-catalog-receipt/1','database':str(db),'sqlite_uri_mode':'ro','query_only':True,'query':query,'parameters':[expected['operation_id'],'catalog_management_fenced'],'row_count':len(rows),'expected_identity':expected,'database_hashes_before':before,'database_hashes_after':after,'database_content_unchanged':before==after,'service_restart_or_apply_replay':False}
if rows:
 stage,revision,payload=rows[0];payload=payload.encode() if isinstance(payload,str) else bytes(payload);assert len(payload)<=16384;value=json.loads(payload)
 assert value['request']['caller']==expected['caller'] and value['request']['key']==expected['key']
 if stage=='metadata_applied':assert value['metadata']['request_digest']==expected['request_digest']
 else:assert value['stage']==stage
 report.update(stage=stage,revision=revision,payload=value,payload_bytes=len(payload))
else:report['interpretation']='Exact row absent; absence does not establish that no effects occurred.'
(run/'evidence3c/exact-offline-catalog-receipt.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='payload'},indent=2))
print(json.dumps({'metadata':report.get('payload',{}).get('metadata'),'proof':report.get('payload',{}).get('proof')},indent=2))
