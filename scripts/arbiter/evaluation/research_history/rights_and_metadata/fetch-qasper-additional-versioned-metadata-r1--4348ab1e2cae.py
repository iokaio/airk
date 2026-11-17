# SPDX-License-Identifier: Apache-2.0
"""Closed official arXiv version-license/title/abstract metadata only; no paper text/dataset download."""
from pathlib import Path
import json,hashlib,time,datetime,urllib.request,urllib.parse
base=Path(__file__).resolve().parent;bulk=base/'qasper-rights-audit-v03/bulk-rights-1794625872552210100.json';raw=bulk.read_bytes();assert hashlib.sha256(raw).hexdigest()=='4a2a0788296acc11e508674a47a66be3c58d1b61294e4872aa5383565b579a2b';rows=json.loads(raw)['records'];single=[];multi=[r for r in rows if any('/by-sa/4.0' in x for x in r['license_urls'])];assert len(multi)==21;extra=[r for r in rows if r['paper_id'] in ('1707.07212','2004.01970','2004.02334')];assert len(extra)==3 and all(r['publisher_split']=='test' for r in extra);multi+=extra
multi.sort(key=lambda r:({'test':0,'validation':1,'train':2}[r['publisher_split']],r['paper_id']));targets=[(p,'1','test') for p in single]+[(r['paper_id'],v,r['publisher_split']) for r in multi for v in r['versions']];assert len(targets)<=80 and len(set((p,v) for p,v,_ in targets))==len(targets)
out=base/'qasper-additional-versioned-metadata-r1';out.mkdir();start=time.monotonic();record={'metadata_only':True,'no_corpus_or_paper_text_adoption':True,'source_bulk_sha256':hashlib.sha256(raw).hexdigest(),'runner_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'declared_target_versions':targets,'started_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'per_response_byte_cap':262144,'total_response_byte_cap':20*1024*1024,'campaign_seconds_cap':600,'requests':[],'unattempted':[]};total=0
save=lambda:(out/'metadata-collection.json').write_text(json.dumps(record,indent=2)+'\n')
for index,(pid,version,split) in enumerate(targets):
 if time.monotonic()-start>=600 or total>=20*1024*1024:record['unattempted']=targets[index:];break
 url='https://arxiv.org/abs/'+pid+'v'+version;item={'paper_id':pid,'version':version,'publisher_split':split,'url':url,'observed_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()};before=time.monotonic()
 try:
  request=urllib.request.Request(url,headers={'User-Agent':'AIRK-private-QASPER-version-license-metadata-audit','Accept-Encoding':'identity'})
  with urllib.request.urlopen(request,timeout=10) as response:
   final=response.geturl();parts=urllib.parse.urlsplit(final);assert parts.scheme=='https' and parts.netloc=='arxiv.org' and parts.path=='/abs/'+pid+'v'+version
   assert response.headers.get('Content-Type','').lower().startswith('text/html');data=response.read(262145);assert len(data)<=262144
  destination=out/(pid+'v'+version+'.html');destination.write_bytes(data);total+=len(data);item.update(file=str(destination),bytes=len(data),sha256=hashlib.sha256(data).hexdigest(),final_url=final)
 except Exception as error:
  item['error']=str(error)
  if getattr(error,'code',None)==429 or '10013' in str(error):record['unattempted']=targets[index+1:];record['requests'].append(item);save();break
 item['elapsed_seconds']=time.monotonic()-before;record['requests'].append(item);save();print(json.dumps({'paper_id':pid,'version':version,'bytes':item.get('bytes'),'error':item.get('error')}),flush=True)
record.update(finished_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),elapsed_seconds=time.monotonic()-start,total_metadata_bytes=total);save();print(json.dumps({'requests':len(record['requests']),'successful':sum('sha256'in r for r in record['requests']),'unattempted':len(record['unattempted']),'record':str(out/'metadata-collection.json')}));raise SystemExit(0 if not record['unattempted'] and all('sha256'in r for r in record['requests']) else 1)
