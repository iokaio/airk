# SPDX-License-Identifier: Apache-2.0
"""Verify exact labeled TRAIN functions against pinned source files and license text."""
import ast,collections,concurrent.futures,hashlib,json,re,urllib.request,urllib.error,time
from pathlib import Path,PurePosixPath
ROOT=Path(__file__).resolve().parent
rights=json.loads((ROOT/'repository-rights-evidence.json').read_bytes())['results'];joined=json.loads((ROOT/'literal-source-join.json').read_bytes())['sources']
code_map=ast.literal_eval((ROOT.parent/'code_idx_map.txt').read_text());code_by_idx={idx:code for code,idx in code_map.items()};questions=json.loads((ROOT.parent/'cosqa-train.json').read_bytes())
refined=[];allowed={};source_records=[]
for repo in rights:
 evidence=[];selected=True
 for row in repo['commits']:
  license=row['license'];families=[]
  if license:
   raw=Path(license['path']).read_bytes();assert hashlib.sha256(raw).hexdigest()==license['sha256'];text=raw.decode('utf-8','replace').lower()
   if 'apache license' in text and 'version 2.0' in text and 'end of terms and conditions' in text:families.append('Apache-2.0-text')
   if 'permission is hereby granted, free of charge' in text and 'deal in the software' in text:families.append('MIT-text')
   if 'redistribution and use in source and binary forms' in text and 'conditions are met' in text:families.append('BSD-family-text')
  single=license is not None and len(families)==1 and license['text_classification'].endswith('text-candidate')
  selected=selected and single
  evidence.append({'commit':row['commit'],'license':license,'recognized_families':families,'single_permissive_text_candidate':single,'notices':row['notice_attempts']})
 if selected:allowed[repo['repository']]=evidence
 refined.append({'repository':repo['repository'],'commits':evidence,'source_verification_candidate':selected,'rights_admission_granted':False})
with (ROOT/'refined-license-candidates.json').open('x',encoding='utf-8') as out:json.dump(refined,out,indent=2);out.write('\n')
selected_rows=[row for row in joined if row['status']=='unique_literal_source' and row['source_alternatives'][0]['repository'] in allowed]
files={}
for row in selected_rows:
 source=row['source_alternatives'][0];p=source['parsed_source'];repo=source['repository'];path=source['path']
 if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+',repo) or PurePosixPath(path).is_absolute() or '..' in PurePosixPath(path).parts:raise ValueError('unsafe source identity')
 url='https://raw.githubusercontent.com/'+repo+'/'+p['commit']+'/'+urllib.parse.quote(path,safe='/')
 files[url]={'repository':repo,'commit':p['commit'],'logical_path':path}
output=ROOT/'original-source-files';output.mkdir()

def fetch(item):
 url,source=item;path=output/(hashlib.sha256(url.encode()).hexdigest()+'.source');result=dict(source,url=url)
 try:
  with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'AIRK-local-source-rights-audit'}),timeout=30) as response:
   data=response.read((8<<20)+1)
   if len(data)>8<<20:raise ValueError('source file exceeds reviewed8MiB bound')
  path.write_bytes(data);result.update(status=200,path=str(path),bytes=len(data),sha256=hashlib.sha256(data).hexdigest(),git_blob_sha1=hashlib.sha1(('blob '+str(len(data))+'\0').encode()+data).hexdigest())
 except urllib.error.HTTPError as error:result.update(status=error.code,error=str(error))
 except Exception as error:result.update(error=type(error).__name__+': '+str(error))
 return result
results={};started=time.monotonic()
with concurrent.futures.ThreadPoolExecutor(max_workers=6) as executor:
 for result in executor.map(fetch,files.items()):results[result['url']]=result
with (ROOT/'original-source-file-acquisition.json').open('x',encoding='utf-8') as out:json.dump({'complete':True,'files':list(results.values()),'elapsed_seconds':time.monotonic()-started,'model_execution':False},out,indent=2);out.write('\n')
verified=[];repos=collections.Counter();failures=collections.Counter();task_counts=collections.Counter();repo_tasks=collections.defaultdict(collections.Counter)
for row in selected_rows:
 source=row['source_alternatives'][0];p=source['parsed_source'];url='https://raw.githubusercontent.com/'+source['repository']+'/'+p['commit']+'/'+urllib.parse.quote(source['path'],safe='/');file=results[url]
 literal=code_by_idx[row['code_idx']].encode('utf-8');status='source_unavailable';offsets=[];header_notices=[]
 if file.get('status')==200:
  raw=Path(file['path']).read_bytes();count=raw.count(literal)
  if count==1:
   at=raw.find(literal);start=raw[:at].count(b'\n')+1;end=start+literal.count(b'\n');offsets=[{'byte_offset':at,'line_start':start,'line_end':end}]
   status='exact_source_literal_confirmed' if p['line_start'] is not None and p['line_start']<=start<=end<=p['line_end'] else 'literal_outside_published_line_span'
  elif count>1:status='ambiguous_repeated_source_literal'
  else:status='source_literal_mismatch'
  header=raw[:min(len(raw),16384)].decode('utf-8','replace')
  header_notices=[line.strip() for line in header.splitlines() if any(term in line.lower() for term in ['copyright','spdx-license','license','licence'])]
  if status=='exact_source_literal_confirmed' and any(term in header.lower() for term in ['gnu general public license','gnu lesser general public license','gnu affero general public license','non-commercial','noncommercial','commons clause']):status='file_specific_terms_require_review'
 candidate={'code_idx':row['code_idx'],'status':status,'original_source':source,'pinned_source_file':file,'literal_byte_matches':offsets,'file_header_notice_lines':header_notices,'pinned_repository_rights':allowed[source['repository']],'rights_admission_granted':False}
 verified.append(candidate)
 if status=='exact_source_literal_confirmed':repos[source['repository']]+=1
 else:failures[status]+=1
by_idx={row['code_idx']:row for row in verified}
accepted=[]
for q in questions:
 index=code_map[q['code']];row=by_idx.get(index)
 if row and row['status']=='exact_source_literal_confirmed':
  repo=row['original_source']['repository'];task_counts['positive' if q['label']==1 else 'negative']+=1;repo_tasks[repo]['positive' if q['label']==1 else 'negative']+=1
  accepted.append({'original_train_idx':q['idx'],'code_idx':index,'label':q['label'],'question_sha256':hashlib.sha256(q['doc'].encode()).hexdigest(),'code_sha256':hashlib.sha256(q['code'].encode()).hexdigest(),'repository':repo,'commit':row['original_source']['parsed_source']['commit'],'source_url':row['original_source']['source_url'],'source_evidence_index':index,'native_visibility_status':'pending native256MiniLM/512Focus audit; no Python token-count proxy','rights_admission_granted':False})
with (ROOT/'source-and-rights-joined-functions.json').open('x',encoding='utf-8') as out:json.dump(verified,out,ensure_ascii=False,indent=2);out.write('\n')
with (ROOT/'source-and-rights-candidate-tasks.json').open('x',encoding='utf-8') as out:json.dump(accepted,out,ensure_ascii=False,indent=2);out.write('\n')
summary={'complete':True,'single_permissive_license_repository_candidates':len(allowed),'downloaded_unique_source_files':len(files),'source_confirmed_functions':sum(repos.values()),'source_confirmed_repositories':len(repos),'source_confirmed_human_train_tasks':len(accepted),'human_task_labels':dict(task_counts),'functions_by_repository':dict(repos),'task_labels_by_repository':{repo:dict(counts) for repo,counts in repo_tasks.items()},'unknown_source_function_reasons':dict(failures),'exact_source_identity':'Unnormalized literal bytes occur exactly once inside published original file line span at exact40hexcommit; actual source/license/notice content SHA retained.','rights_status':'Candidate evidence complete; Root rights review and native visiblealignment still required.','rights_admission_granted':False,'nontrain_text_accessed':False,'model_execution':False,'elapsed_seconds':time.monotonic()-started}
with (ROOT/'source-and-rights-join-summary.json').open('x',encoding='utf-8') as out:json.dump(summary,out,indent=2);out.write('\n')
print(json.dumps({k:v for k,v in summary.items() if k not in ['functions_by_repository','task_labels_by_repository']}),flush=True)