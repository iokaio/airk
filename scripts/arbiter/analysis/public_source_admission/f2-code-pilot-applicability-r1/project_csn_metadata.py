# SPDX-License-Identifier: Apache-2.0
import pathlib,hashlib,json,collections,urllib.parse,re
P=pathlib.Path(__file__).resolve().parent;J=P.parent/'cosqa-source-join-r1';raw=(P/'CSN-human-annotationStore.publisher.opaque').read_bytes()
def records(data):
 i=0;n=len(data)
 while i<n:
  while i<n and data[i]in(10,13):i+=1
  if i>=n:break
  fields=[]
  while True:
   quote=i<n and data[i]==34
   if quote:
    i+=1;start=i
    while i<n:
     if data[i]==34:
      if i+1<n and data[i+1]==34:i+=2;continue
      break
     i+=1
    assert i<n,'unclosed quoted field';end=i;i+=1
   else:
    start=i
    while i<n and data[i]not in(44,10,13):i+=1
    end=i
   fields.append((start,end,quote))
   if i<n and data[i]==44:i+=1;continue
   assert i==n or data[i]in(10,13),'unexpected after-field byte'
   break
  yield fields

def value(data,f):
 a,b,q=f;z=data[a:b];return z.replace(b'""',b'"')if q else z

def opaque_hash(data,f):
 a,b,q=f;h=hashlib.sha256()
 if not q:h.update(memoryview(data)[a:b]);return h.hexdigest()
 start=a;i=a
 while i<b:
  if data[i:i+2]==b'""':h.update(memoryview(data)[start:i]);h.update(b'"');i+=2;start=i
  else:i+=1
 h.update(memoryview(data)[start:b]);return h.hexdigest()
# Synthetic CSV byte-lexer controls only; no semantic labels or model inputs.
toy=b'A,B,C\r\nxx,"yy,z","a""b\nline"\r\r\n';rr=list(records(toy));assert len(rr)==2 and value(toy,rr[1][1])==b'yy,z'and value(toy,rr[1][2])==b'a"b\nline';assert opaque_hash(toy,rr[1][2])==hashlib.sha256(b'a"b\nline').hexdigest()
rr=iter(records(raw));head=[value(raw,f).decode('ascii')for f in next(rr)];assert head==['Language','Query','GitHubUrl','Relevance','Notes'];rows=[]
for ordinal,fields in enumerate(rr,1):
 assert len(fields)==5
 language=value(raw,fields[0]).decode('ascii');url=value(raw,fields[2]).decode('utf8');grade=value(raw,fields[3]).decode('ascii');qhash=opaque_hash(raw,fields[1]);parts=urllib.parse.urlsplit(url);segs=parts.path.strip('/').split('/');repo='/'.join(segs[:2]);commit=segs[3]if len(segs)>3 and segs[2]=='blob'else None;path='/'.join(segs[4:])if commit else None;span=re.fullmatch(r'L(\d+)-L(\d+)',parts.fragment);rows.append(dict(annotation_ordinal=ordinal,language=language,query_sha256=qhash,source_url=url,repository_literal=repo,commit=commit,path=path,line_range=[int(span[1]),int(span[2])]if span else None,published_relevance=grade,source_use='EVALUATION_ONLY',question_and_notes_decoded=False))
assert len(rows)==4006
source= json.loads((J/'source-and-rights-candidate-tasks.json').read_text());url_to_ids=collections.defaultdict(list)
for t in source:url_to_ids[t['source_url']].append(t['original_train_idx'])
right=json.loads((J/'source-rights-applicability-review-r1/review.json').read_text());file_scopes={(f['repository'].lower(),f['commit'],f['logical_path']):f['local_private_research_review']for f in right['source_files']};cosqa_q={t['question_sha256']for t in source};repos={t['repository'].lower()for t in source}
for r in rows:r.update(exact_existing_CoSQA_source_function_url_ids=url_to_ids.get(r['source_url'],[]),source_file_rights_review=file_scopes.get((r['repository_literal'].lower(),r['commit'],r['path']),'NOT_IN_REVIEWED_316_FILE_SCOPE'),source_repo_in_CoSQA38=r['repository_literal'].lower()in repos,query_in_original_CoSQA1955=r['query_sha256']in cosqa_q)
summary=dict(schema='airk.private.CSN-evaluation-only-metadata-projection/1',opaque_source_sha256=hashlib.sha256(raw).hexdigest(),rows=len(rows),unique_queries=len({r['query_sha256']for r in rows}),query_language_groups=len({(r['query_sha256'],r['language'])for r in rows}),languages=dict(collections.Counter(r['language']for r in rows)),literal_repository_names=len({r['repository_literal'].lower()for r in rows}),source_urls=len({r['source_url']for r in rows}),rows_with_exact_CoSQA_function_url=sum(bool(r['exact_existing_CoSQA_source_function_url_ids'])for r in rows),rows_with_CoSQA38_repo=sum(r['source_repo_in_CoSQA38']for r in rows),rows_with_CoSQA_exact_query=sum(r['query_in_original_CoSQA1955']for r in rows),source_file_rights_scopes=dict(collections.Counter(r['source_file_rights_review']for r in rows)),relevance_distribution=dict(collections.Counter(r['published_relevance']for r in rows)),question_and_notes_decoded=False,training=False,scoring=False,native_fit=False,source_code_downloaded=False,FINAL_created=False,lexer_controls='quotedcomma,doublequote,multiline,CRCRLF and opaqueCSV-unescaped-field hash PASS')
with(P/'CSN-eval-only-row-metadata.json').open('x')as f:json.dump(rows,f,indent=2)
with(P/'CSN-eval-only-metadata-summary.json').open('x')as f:json.dump(summary,f,indent=2)
print(json.dumps(summary))
