# SPDX-License-Identifier: Apache-2.0
import concurrent.futures,hashlib,json,os,time,urllib.request
from pathlib import Path
ROOT=Path(__file__).parent
catalog_path=ROOT/'research-asset-pins.json'
catalog_bytes=catalog_path.read_bytes(); catalog=json.loads(catalog_bytes)
destination=ROOT/'candidate-assets'; destination.mkdir(exist_ok=True)
started=time.monotonic()
def fetch(item):
 model,asset=item; name=asset['name']; path=destination/model['id'].replace('/','--')/name
 path.parent.mkdir(parents=True,exist_ok=True)
 def verify():
  if path.stat().st_size!=asset['bytes']: raise ValueError('length differs:'+str(path))
  sha=hashlib.sha256(); blob=hashlib.sha1(); blob.update(('blob '+str(asset['bytes'])+'\0').encode())
  with path.open('rb') as f:
   while chunk:=f.read(4194304): sha.update(chunk);blob.update(chunk)
  if asset['sha256'] and sha.hexdigest()!=asset['sha256']: raise ValueError('sha256 differs:'+str(path))
  if not asset['sha256'] and blob.hexdigest()!=asset['git_blob']: raise ValueError('gitblob differs:'+str(path))
  return {'model':model['id'],'revision':model['revision'],'name':name,'bytes':asset['bytes'],'sha256':sha.hexdigest()}
 if path.exists(): return verify()
 partial=path.with_name(path.name+'.partial')
 if partial.exists(): raise ValueError('prior interrupted acquisition requires diagnosis:'+str(partial))
 url='https://huggingface.co/'+model['id']+'/resolve/'+model['revision']+'/'+name
 count=0
 with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'AIRK-authorized-private-model-qualification'}),timeout=120) as source, partial.open('xb') as sink:
  while chunk:=source.read(4194304):
   count+=len(chunk)
   if count>asset['bytes']: raise ValueError('download exceeded exact pinned bound')
   sink.write(chunk)
 if count!=asset['bytes']:raise ValueError('truncated pinnedasset')
 partial.rename(path)
 result=verify();print(json.dumps(result),flush=True);return result
items=[]
for model in catalog['models']:
 if model['id'].startswith('sentence-transformers/'):continue
 for asset in model['assets']:
  n=asset['name']
  if n.endswith('.py') or n.startswith('training/') or n.endswith('.yaml'):continue
  if n.endswith(('.safetensors','.pt','.json','.txt','.md','.model','.jinja')) or 'LICENSE' in n or 'MANIFEST' in n:items.append((model,asset))
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor: records=list(executor.map(fetch,items))
if catalog_path.read_bytes()!=catalog_bytes:raise ValueError('asset catalog drift')
report={'catalog_sha256':hashlib.sha256(catalog_bytes).hexdigest(),'source_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'records':records,'elapsed_seconds':time.monotonic()-started,'scope':'Private full pinned candidate assets; head.pt archival only, no unsafe pickle loading or remote model code execution.'}
(destination/('acquisition-'+str(time.time_ns())+'.json')).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'files':len(records),'bytes':sum(r['bytes'] for r in records),'elapsed_seconds':report['elapsed_seconds']}),flush=True)
