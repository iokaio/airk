# SPDX-License-Identifier: Apache-2.0
import json,urllib.request,urllib.error,urllib.parse
from pathlib import Path
ROOT=Path(__file__).resolve().parent
class NoRedirect(urllib.request.HTTPRedirectHandler):
 def redirect_request(self,*args,**kwargs):return None
opener=urllib.request.build_opener(NoRedirect);rows=[]
urls=['https://huggingface.co/Qwen/Qwen3-1.7B/resolve/70d244cc86ccca08cf5af4e1e306ecf908b1ad5e/tokenizer.json','https://huggingface.co/Qwen/Qwen3-1.7B/resolve/70d244cc86ccca08cf5af4e1e306ecf908b1ad5e/LICENSE']
for url in urls:
 try:response=opener.open(urllib.request.Request(url,method='HEAD'),timeout=30)
 except urllib.error.HTTPError as error:code=error.code;headers=error.headers
 else:code=response.status;headers=response.headers;response.close()
 location=urllib.parse.urlparse(headers.get('Location',''));rows.append({'source':url,'status':code,'redirect_scheme':location.scheme,'redirect_host':location.hostname,'redirect_path':location.path,'query_redacted':bool(location.query),'no_payload_transferred':True})
with (ROOT/'hf-redirect-routes.json').open('x',encoding='utf-8') as file:json.dump(rows,file,indent=2);file.write('\n')
print(json.dumps(rows))
