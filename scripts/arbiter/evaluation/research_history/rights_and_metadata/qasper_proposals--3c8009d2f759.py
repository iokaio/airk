# SPDX-License-Identifier: Apache-2.0
import hashlib, html, io, json, re, tarfile
from pathlib import Path
root=Path(__file__).parent/'qasper-rights-audit-v03'
raw=(root/'qasper-train-dev-v0.3.tgz').read_bytes()
assert hashlib.sha256(raw).hexdigest()=='a28fdf966db827bcee3d873107d6b6669864fb7ca8fbf73a192f5e39191bdb5a'
papers={}
with tarfile.open(fileobj=io.BytesIO(raw),mode='r:gz') as archive:
 for member,split in [('qasper-train-v0.3.json','train'),('qasper-dev-v0.3.json','validation')]:
  for identifier,paper in json.loads(archive.extractfile(member).read()).items(): papers[identifier]=(split,paper)
rows=[]
for identifier in ['1912.01673','1912.08320','2003.11645','1909.03526','1911.12579']:
 metadata=(root/f'arxiv-{identifier}.html').read_bytes(); text=html.unescape(metadata.decode())
 def meta(key):
  match=re.search(r'<meta name="'+key+r'" content="([\s\S]*?)"\s*/?>',text)
  return match.group(1) if match else None
 split,paper=papers[identifier]
 norm=lambda value: ' '.join(value.split()).strip()
 versions=sorted(set(re.findall(r'\[v(\d+)\]',text)))
 item={'paper_id':identifier,'publisher_split':split,'title':paper['title'],
  'title_matches':norm(meta('citation_title'))==norm(paper['title']),
  'abstract_matches':norm(meta('citation_abstract'))==norm(paper['abstract']),
  'versions_observed':versions,'source_text_sha256':hashlib.sha256(json.dumps(paper['full_text'],sort_keys=True,ensure_ascii=False,separators=(',',':')).encode()).hexdigest(),
  'metadata_sha256':hashlib.sha256(metadata).hexdigest(),'license_url':'https://creativecommons.org/licenses/by/4.0/',
  'authors':re.findall(r'<meta name="citation_author" content="([^"]+)"',text),
  'questions':len(paper['qas']),'human_alternatives':sum(len(q['answers']) for q in paper['qas']),
  'question_ids':[q['question_id'] for q in paper['qas']],
  'human_annotation_ids':[a['annotation_id'] for q in paper['qas'] for a in q['answers']],
  'scope':'Development rights proposal only, no adoption/training/evaluation; quote-bearing paragraphs and all figures/tables excluded if approved.'}
 item['status']='await_primary_review_single_version_exact_title_abstract' if versions==['1'] and item['title_matches'] and item['abstract_matches'] else 'unresolved_source_version_or_alignment_excluded'
 rows.append(item)
result={'archive_sha256':hashlib.sha256(raw).hexdigest(),'test_access':'No held-out test metadata/fulltext inspected for these proposals.','proposals':rows}
destination=root/'expanded-development-rights-proposals.json'
assert not destination.exists()
destination.write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps([{'paper_id':r['paper_id'],'split':r['publisher_split'],'questions':r['questions'],'human_alternatives':r['human_alternatives'],'versions':r['versions_observed'],'title':r['title_matches'],'abstract':r['abstract_matches'],'status':r['status']} for r in rows],indent=2))
