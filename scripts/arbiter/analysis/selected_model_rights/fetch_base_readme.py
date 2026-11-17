# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import urllib.request,json,hashlib
out=Path('artifacts/advanced-split/models/selected-model-rights-r1');m=json.loads((out/'source-notice-manifest.json').read_bytes());row=next(r for r in m['upstream']if r['repository']=='Qwen/Qwen3-0.6B-Base');meta=json.loads(Path('artifacts/advanced-split/models/qwen3-reranker06-assets-r1/Qwen--Qwen3-0.6B-Base-metadata.json').read_bytes());entry=next(e for e in meta['siblings']if e['rfilename']=='README.md');url=f'https://huggingface.co/{row["repository"]}/resolve/{row["revision"]}/README.md';request=urllib.request.Request(url,headers={'User-Agent':'AIRK-rights-evidence/1'})
with urllib.request.urlopen(request,timeout=20) as response:
 raw=response.read(1024*1024+1);assert response.status==200
assert len(raw)==entry['size'];blob=hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest();assert blob==entry['blobId']
p=out/'notices/Qwen--Qwen3-0.6B-Base--README.md'
with p.open('xb')as stream:stream.write(raw)
row['files'].append({'name':'README.md','source_url':url,'git_blob_sha1':blob,'original':'Downloaded exact pinned official commit with ordinary TLS; original candidate acquisition intentionally contained only LICENSE','pin':{'path':str(p.resolve()),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}});row['not_locally_acquired']=[];(out/'source-notice-manifest.json').write_text(json.dumps(m,indent=2)+'\n',encoding='utf-8');print('Pinned full base README',len(raw),hashlib.sha256(raw).hexdigest())
