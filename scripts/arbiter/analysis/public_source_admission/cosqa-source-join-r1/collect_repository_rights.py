# SPDX-License-Identifier: Apache-2.0
"""Pinned original-repository license/source evidence; no model or dataset split changes."""
import collections,concurrent.futures,hashlib,json,re,time,urllib.request,urllib.error
from pathlib import Path
ROOT=Path(__file__).resolve().parent
summary=json.loads((ROOT/'literal-source-join-summary.json').read_bytes());joined=json.loads((ROOT/'literal-source-join.json').read_bytes())['sources']
by_repo=collections.defaultdict(list)
for row in joined:
 if row['status']=='unique_literal_source':by_repo[row['source_alternatives'][0]['repository']].append(row)
repos=sorted(by_repo,key=lambda name:(-summary['positive_tasks_by_repository'].get(name,0),-len(by_repo[name]),name))[:90]
output=ROOT/'repository-evidence';output.mkdir()
selection={'basis':'TRAIN label counts and exact literal source membership only; no model scores/other split rows','repositories':repos,'source_candidates':len(repos),'admission_granted':False}
(ROOT/'rights-candidate-selection.json').write_text(json.dumps(selection,indent=2)+'\n',encoding='utf-8')

def fetch(url,path,maxbytes):
 started=time.monotonic()
 try:
  with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'AIRK-local-source-rights-audit'}),timeout=25) as response:
   data=response.read(maxbytes+1)
   if len(data)>maxbytes:raise ValueError('bounded source/license exceeded')
   path.write_bytes(data)
   return {'url':url,'status':response.status,'path':str(path),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'git_blob_sha1':hashlib.sha1(('blob '+str(len(data))+'\0').encode()+data).hexdigest(),'elapsed_seconds':time.monotonic()-started}
 except urllib.error.HTTPError as error:return {'url':url,'status':error.code,'error':str(error),'elapsed_seconds':time.monotonic()-started}
 except Exception as error:return {'url':url,'error':type(error).__name__+': '+str(error),'elapsed_seconds':time.monotonic()-started}

def license_kind(raw):
 text=raw.decode('utf-8','replace').lower()
 if any(term in text for term in ['non-commercial','noncommercial','not for commercial','commons clause','business source license']):return 'restricted_or_ambiguous'
 if 'permission is hereby granted, free of charge' in text and 'deal in the software' in text and 'the software is provided' in text:return 'MIT-text-candidate'
 if 'apache license' in text and 'version 2.0' in text and 'end of terms and conditions' in text:return 'Apache-2.0-text-candidate'
 if 'redistribution and use in source and binary forms' in text and 'this software is provided' in text and 'conditions are met' in text:return 'BSD-text-candidate'
 if 'gnu general public license' in text or 'gnu lesser general public license' in text or 'gnu affero general public license' in text:return 'copyleft_not_selected'
 return 'unknown_not_selected'

def repository(repo):
 directory=output/repo.replace('/','--');directory.mkdir();commits=sorted({row['source_alternatives'][0]['parsed_source']['commit'] for row in by_repo[repo]});evidence=[]
 for commit in commits:
  current=directory/commit;current.mkdir();attempts=[];license_record=None
  for filename in ['LICENSE','LICENSE.txt','LICENSE.md','LICENSE.rst','LICENCE','LICENCE.txt','COPYING','COPYING.txt']:
   record=fetch('https://raw.githubusercontent.com/'+repo+'/'+commit+'/'+filename,current/filename,512<<10);attempts.append(record)
   if record.get('status')==200:
    record['text_classification']=license_kind((current/filename).read_bytes());license_record=record;break
  notices=[]
  if license_record and license_record['text_classification'].endswith('text-candidate'):
   for filename in ['NOTICE','AUTHORS']:
    record=fetch('https://raw.githubusercontent.com/'+repo+'/'+commit+'/'+filename,current/filename,512<<10)
    notices.append(record)
  evidence.append({'commit':commit,'license_attempts':attempts,'license':license_record,'notice_attempts':notices})
 result={'repository':repo,'exact_literal_functions':len(by_repo[repo]),'positive_human_tasks':summary['positive_tasks_by_repository'].get(repo,0),'commits':evidence,'rights_admission_granted':False,'classification_scope':'Candidate text recognition only; actual source/file notices and Root rights review remain required.'}
 (directory/'rights-evidence.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
 return result

results=[];started=time.monotonic()
with concurrent.futures.ThreadPoolExecutor(max_workers=6) as executor:
 futures={executor.submit(repository,repo):repo for repo in repos}
 for future in concurrent.futures.as_completed(futures):
  result=future.result();results.append(result);print(json.dumps({'repository':result['repository'],'licenses':[row['license']['text_classification'] if row['license'] else 'missing' for row in result['commits']],'completed':len(results),'total':len(repos)}),flush=True)
  (ROOT/'repository-rights-evidence.json').write_text(json.dumps({'complete':len(results)==len(repos),'results':results,'elapsed_seconds':time.monotonic()-started,'model_execution':False,'admission_granted':False},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'complete':True,'repositories':len(results),'permissive_text_candidates':sum(all(row['license'] and row['license']['text_classification'].endswith('text-candidate') for row in result['commits']) for result in results),'elapsed_seconds':time.monotonic()-started}),flush=True)