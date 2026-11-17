# SPDX-License-Identifier: Apache-2.0
"""Read small public HF blob inventories only. Never fetch model or tokenizer bytes."""
import hashlib,json,time,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent
sources=[('Qwen/Qwen3-Reranker-0.6B','e61197ed45024b0ed8a2d74b80b4d909f1255473'),('Qwen/Qwen3-0.6B-Base','da87bfb608c14b7cf20ba1ce41287e8de496c0cd'),('Qwen/Qwen3-1.7B','70d244cc86ccca08cf5af4e1e306ecf908b1ad5e'),('Qwen/Qwen3-4B','1cfa9a7208912126459214e8b04321603b3df60c'),('onnx-community/Qwen3-Reranker-0.6B-ONNX','main'),('onnx-community/Qwen3-1.7B-ONNX','main'),('onnx-community/Qwen3-4B-ONNX','main')]
rows=[]
for repo,revision in sources:
 url=f'https://huggingface.co/api/models/{repo}/revision/{revision}?blobs=true'
 request=urllib.request.Request(url,headers={'User-Agent':'AIRK-readonly-distribution-metadata'})
 with urllib.request.urlopen(request,timeout=40) as response:
  assert response.status==200
  raw=response.read((2<<20)+1)
 assert len(raw)<=2<<20
 metadata=json.loads(raw);assert metadata['id']==repo and len(metadata['sha'])==40
 path=ROOT/(repo.replace('/','--')+'--metadata.json')
 with path.open('xb') as stream:stream.write(raw)
 rows.append({'repository':repo,'requested_revision':revision,'actual_revision':metadata['sha'],'metadata_url':url,'path':str(path.resolve()),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'files':len(metadata['siblings'])})
record={'schema':'airk.private-public-model-origin-inventories/1','sources':rows,'model_or_tokenizer_payload_downloaded':False,'uploaded':False,'qualification_granted':False}
with (ROOT/'inventories.json').open('x',encoding='utf-8') as stream:json.dump(record,stream,indent=2);stream.write('\n')
print(json.dumps(record))
