# SPDX-License-Identifier: Apache-2.0
"""HEAD only known exact public primary/backup file URLs, no payload transfer."""
import hashlib,json,urllib.request,urllib.error,urllib.parse
from pathlib import Path
ROOT=Path(__file__).resolve().parent
class NoRedirect(urllib.request.HTTPRedirectHandler):
 def redirect_request(self,*args,**kwargs):return None
opener=urllib.request.build_opener(NoRedirect)
matrix=json.loads((ROOT/'asset-origin-matrix.raw-r1.json').read_bytes());unique={}
for row in matrix['assets']:
 for source in ([row['canonical_primary']] if row['canonical_primary'] else [])+row['verified_byte_identical_backups']:
  unique[source['url']]=source
results=[]
for url,source in sorted(unique.items()):
 request=urllib.request.Request(url,method='HEAD',headers={'User-Agent':'AIRK-readonly-origin-availability'})
 try: response=opener.open(request,timeout=30)
 except urllib.error.HTTPError as response:
  code=response.code;headers=response.headers
 else:code=response.status;headers=response.headers;response.close()
 results.append({'url':url,'status':code,'linked_etag':headers.get('X-Linked-Etag'),'etag':headers.get('ETag'),'content_length':headers.get('Content-Length'),'linked_size':headers.get('X-Linked-Size'),'redirect_host':urllib.parse.urlparse(headers.get('Location','')).hostname,'no_payload_transferred':True})
record={'schema':'airk.private-public-origin-head-observations/1','sources':results,'scope':'HEAD availability and immutable identity metadata only, not complete transfer verification or native downloader support. Signed redirect queries deliberately not retained.','uploaded':False}
with (ROOT/'source-head-observations.json').open('x',encoding='utf-8') as stream:json.dump(record,stream,indent=2);stream.write('\n')
print(json.dumps(record))
