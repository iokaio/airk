# SPDX-License-Identifier: Apache-2.0
"""Bounded public metadata audit, no model scoring; preserve publisher splits."""
import concurrent.futures,hashlib,html,io,json,re,tarfile,time,urllib.request
from pathlib import Path
root=Path(__file__).parent/'qasper-rights-audit-v03'
papers={}
archives=[('qasper-train-dev-v0.3.tgz','a28fdf966db827bcee3d873107d6b6669864fb7ca8fbf73a192f5e39191bdb5a', [('qasper-train-v0.3.json','train'),('qasper-dev-v0.3.json','validation')]),
 ('qasper-test-and-evaluator-v0.3.tgz','72a52a41193e2838b8074f80ac074b94f956b84886c36a61c58a7df4171bdd72',[('qasper-test-v0.3.json','test')])]
for filename,sha,members in archives:
 raw=(root/filename).read_bytes();assert hashlib.sha256(raw).hexdigest()==sha
 with tarfile.open(fileobj=io.BytesIO(raw),mode='r:gz') as archive:
  for member,split in members:
   for identifier,paper in json.loads(archive.extractfile(member).read()).items():
    assert identifier not in papers
    papers[identifier]=(split,paper)
assert len(papers)==1585
started=time.monotonic()
def inspect(identifier):
 split,paper=papers[identifier];path=root/f'arxiv-{identifier}.html'
 item={'paper_id':identifier,'publisher_split':split,'eligible':False}
 if not re.fullmatch(r'[0-9]{4}\.[0-9]{4,5}',identifier):return {**item,'reason':'unsupported-id'}
 try:
  if path.exists():data=path.read_bytes()
  else:
   with urllib.request.urlopen(urllib.request.Request('https://arxiv.org/abs/'+identifier,headers={'User-Agent':'AIRK-private-QASPER-license-audit'}),timeout=30) as response:data=response.read(262145)
   if len(data)>262144:raise ValueError('metadata bound exceeded')
   with path.open('xb') as output:output.write(data)
  text=html.unescape(data.decode())
  def meta(key):
   match=re.search(r'<meta name="'+key+r'" content="([\s\S]*?)"\s*/?>',text)
   return match.group(1) if match else ''
  norm=lambda value:' '.join(value.split()).strip()
  versions=sorted(set(re.findall(r'\[v(\d+)\]',text)))
  licenses=sorted(set(re.findall(r'https?://(?:creativecommons\.org/licenses/[^\s"<>]+|arxiv\.org/licenses/[^\s"<>]+)',text)))
  item.update(metadata_sha256=hashlib.sha256(data).hexdigest(),versions=versions,license_urls=licenses,
   title_matches=norm(meta('citation_title'))==norm(paper['title']),abstract_matches=norm(meta('citation_abstract'))==norm(paper['abstract']))
  item['eligible']=versions==['1'] and item['title_matches'] and item['abstract_matches'] and any('creativecommons.org/licenses/by/4.0' in license for license in licenses)
  if item['eligible']:
   item.update(title=paper['title'],authors=re.findall(r'<meta name="citation_author" content="([^"]+)"',text),
    source_id='arxiv:'+identifier+'v1',source_text_sha256=hashlib.sha256(json.dumps(paper['full_text'],sort_keys=True,ensure_ascii=False,separators=(',',':')).encode()).hexdigest(),
    question_ids=[q['question_id'] for q in paper['qas']],human_annotation_ids=[a['annotation_id'] for q in paper['qas'] for a in q['answers']],
    questions=len(paper['qas']),human_alternatives=sum(len(q['answers']) for q in paper['qas']))
 except Exception as error:item['error']=str(error)
 return item
records=[]
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
 for index,record in enumerate(pool.map(inspect,sorted(papers))):
  records.append(record)
  if index%100==0:print(json.dumps({'observed':index+1,'eligible':sum(r['eligible'] for r in records),'errors':sum('error' in r for r in records)}),flush=True)
report={'schema':'airk.private-qasper-rights-closure/1','scope':'Rights/schema only, no model evaluation. Original publisher train/validation/test splits immutable. Groundtruth consists only original human annotations; no new adjudication claimed.',
 'decision_authority':'Primary delegates adoption of unambiguous single-version CC BY4 exact-title/abstract sources under previously reviewed criteria; materialized closure requires primary review before training.',
 'records':records,'elapsed_seconds':time.monotonic()-started,'runner_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
destination=root/f'bulk-rights-{time.time_ns()}.json';destination.write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'output':str(destination),'eligible_papers':sum(r['eligible'] for r in records),'eligible_questions':sum(r.get('questions',0) for r in records if r['eligible']),'errors':sum('error' in r for r in records),'elapsed_seconds':report['elapsed_seconds']}),flush=True)
