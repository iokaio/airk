# SPDX-License-Identifier: Apache-2.0
"""Pinned Python TRAIN-only public source download; no validation/test rows or model."""
import hashlib,json,time,urllib.request,urllib.parse
from pathlib import Path
ROOT=Path(__file__).resolve().parent
meta=json.loads((ROOT/'hf-dataset-metadata.json').read_bytes());tree=json.loads((ROOT/'hf-dataset-tree.json').read_bytes())
row=next(row for row in tree if row.get('path')=='python/train-00000-of-00001.parquet')
assert row['size']==521785752 and row['lfs']['oid']=='ad9e3a4ab10c2c1d8926d2b26ca2bfcc3aadda1477ba29a933391f93806b9fed'
url='https://huggingface.co/datasets/code-search-net/code_search_net/resolve/'+meta['sha']+'/'+row['path']
path=ROOT/'codesearchnet-python-TRAIN.parquet'
assert not path.exists()
started=time.monotonic();deadline=started+1800;count=0;sha=hashlib.sha256();failure=None;receipt={'source_url':url,'publisher_repo_revision':meta['sha'],'publisher_file':row,'selected_split':'python/TRAIN','other_split_text_downloaded':False,'model_execution':False}
try:
 with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'AIRK-local-source-rights-audit'}),timeout=60) as response,path.open('xb') as output:
  endpoint=urllib.parse.urlsplit(response.geturl());receipt['response_endpoint_without_query']=urllib.parse.urlunsplit((endpoint.scheme,endpoint.netloc,endpoint.path,'',''))
  receipt['response_headers']={key:value for key,value in response.headers.items() if key.lower() in ['content-length','etag','last-modified','x-repo-commit','x-linked-etag','content-type']}
  next_report=32<<20
  while chunk:=response.read(1<<20):
   if time.monotonic()>=deadline:raise TimeoutError('original source download deadline expired')
   count+=len(chunk)
   if count>row['size']:raise ValueError('publisher train file larger than frozen inventory')
   output.write(chunk);sha.update(chunk)
   if count>=next_report:print(json.dumps({'downloaded_bytes':count,'total_bytes':row['size']}),flush=True);next_report+=32<<20
  output.flush()
 if count!=row['size'] or sha.hexdigest()!=row['lfs']['oid']:raise ValueError('exact publisher TRAIN file hash/length mismatch')
 with path.open('rb') as data:
  assert data.read(4)==b'PAR1';data.seek(-4,2);assert data.read()==b'PAR1'
 receipt.update(complete=True,path=str(path),bytes=count,sha256=sha.hexdigest())
except BaseException as error:
 failure={'type':type(error).__name__,'message':str(error)};raise
finally:
 receipt.update(failure=failure,observed_bytes=count,observed_sha256=sha.hexdigest(),elapsed_seconds=time.monotonic()-started,automatic_retry=False)
 with (ROOT/'python-train-acquisition.json').open('x',encoding='utf-8') as out:json.dump(receipt,out,indent=2);out.write('\n')
print(json.dumps({'complete':True,'bytes':count,'sha256':sha.hexdigest(),'elapsed_seconds':receipt['elapsed_seconds']}),flush=True)