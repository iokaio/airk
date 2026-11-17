# SPDX-License-Identifier: Apache-2.0
"""Pinned publisher TRAIN source/rights audit. No native compilation, model, scoring, or nonTRAIN semantic access."""
import concurrent.futures,collections,hashlib,json,pathlib,re,threading,urllib.request,urllib.error,urllib.parse,time
ROOT=pathlib.Path(__file__).resolve().parent
OUT=ROOT/'full450-source-audit-r1'
LIMIT=160*1024*1024
lock=threading.Lock();transferred=0
CACHE={}
old=json.loads((ROOT.parent/'contextbench-source-join-r2/receipt.json').read_bytes())
for case in old['results']:
 for pin in case['source_files']+case['rights_files']:CACHE[pin['url']]=pin

def digest(raw):return hashlib.sha256(raw).hexdigest()
def ordinary_relative(value,repo):
 if value.startswith('/workspace/'):
  prefix='/workspace/'+repo.replace('/','__')+'__0.1/'
  if not value.startswith(prefix):raise ValueError('outside exact declared repository checkout')
  value=value[len(prefix):]
 elif value.startswith('/testbed/'):value=value[len('/testbed/'):]
 pure=pathlib.PurePosixPath(value)
 if pure.is_absolute() or '..' in pure.parts or ':' in value or not value:raise ValueError('foreign annotation path')
 return pure.as_posix()
def get(repo,commit,relative,kind):
 global transferred
 url='https://raw.githubusercontent.com/'+repo+'/'+commit+'/'+urllib.parse.quote(relative,safe='/')
 pin=CACHE.get(url)
 if pin:
  raw=pathlib.Path(pin['path']).read_bytes()
  if digest(raw)!=pin['sha256']:raise RuntimeError('retained source cache drift')
  return raw,dict(pin,cache_reused=True)
 cap=2_000_000 if kind=='source' else 250000
 try:
  with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'AIRK-approved-publisher-TRAIN-source-audit'}),timeout=25) as response:raw=response.read(cap+1)
 except urllib.error.HTTPError as error:
  if error.code==404:return None,None
  raise
 if len(raw)>cap:raise ValueError('original file exceeds predeclared audit cap')
 with lock:
  if transferred+len(raw)>LIMIT:raise ValueError('original source audit transfer cap reached')
  transferred+=len(raw)
 name=hashlib.sha256(url.encode()).hexdigest()+'.bytes';path=OUT/'objects'/name
 with path.open('xb') as output:output.write(raw)
 return raw,{'path':str(path),'url':url,'upstream_path':relative,'sha256':digest(raw),'bytes':len(raw),'cache_reused':False}
def license_kind(raw):
 text=raw.decode('utf-8',errors='replace')
 if 'GNU GENERAL PUBLIC LICENSE' in text:return 'GPL/manual use review'
 if 'GNU LESSER GENERAL PUBLIC LICENSE' in text:return 'LGPL/manual use review'
 if 'Apache License' in text and 'Version 2.0' in text:return 'Apache-2.0'
 if 'Permission is hereby granted, free of charge' in text:return 'MIT'
 if 'Redistribution and use in source and binary forms' in text:return 'BSD-family'
 return 'other/manual use review'
def decode_formatter(content,start,end):
 lines=content.strip('\r\n').splitlines();matches=[re.fullmatch(r'(\d+)\t(.*)',line) for line in lines]
 if matches and all(matches):
  if [int(m[1]) for m in matches]!=list(range(start,end+1)):raise ValueError('explicit counter/range inconsistency')
  return '\n'.join(m[2] for m in matches),'verified integer-line-counter formatter removed'
 return content,'none'
def source_job(key):
 repo,commit,path=key
 try:
  raw,pin=get(repo,commit,path,'source')
  if raw is None:return {'key':key,'refusal':'immutable original path404'}
  raw.decode('utf-8')
  pin['spdx_first8192']=re.findall(r'SPDX-License-Identifier:\s*([^\r\n*]+)',raw[:8192].decode('utf-8',errors='replace'))
  return {'key':key,'pin':pin}
 except Exception as error:return {'key':key,'refusal':type(error).__name__+': '+str(error)}
def rights_job(key):
 repo,commit=key;result={'key':key,'pins':[],'refusals':[]}
 known=[]
 for case in old['results']:
  if case['resolved_repo'].lower()==repo.lower():
   known=[p['upstream_path'] for p in case['rights_files'] if p['upstream_path']!='NOTICE'];break
 paths=list(dict.fromkeys(known+['LICENSE','LICENSE.txt','LICENSE.md','LICENSE.rst','LICENSE-MIT','LICENSE-APACHE','COPYING']))
 for name in paths:
  try:
   raw,pin=get(repo,commit,name,'rights')
   if raw is not None:
    pin['kind']=license_kind(raw);result['pins'].append(pin)
    if not name.startswith('LICENSE-'):break
  except Exception as error:result['refusals'].append({'path':name,'reason':type(error).__name__+': '+str(error)})
 for name in ['NOTICE','AUTHORS']:
  try:
   raw,pin=get(repo,commit,name,'rights')
   if raw is not None:pin['kind']='notice/attribution';result['pins'].append(pin)
  except Exception as error:result['refusals'].append({'path':name,'reason':type(error).__name__+': '+str(error)})
 return result

def main():
 started=time.monotonic();assert not OUT.exists();OUT.mkdir();(OUT/'objects').mkdir()
 source=ROOT/'original-TRAIN-only-current-publisher.json';rows=json.loads(source.read_bytes());assert len(rows)==450
 prepared=[];keys=set();legal=set()
 for ordinal,row in enumerate(rows):
  parsed=urllib.parse.urlparse(row['repo_url']);repo=parsed.path.strip('/')
  if repo.endswith('.git'):repo=repo[:-4]
  assert parsed.scheme=='https' and parsed.hostname=='github.com' and repo.count('/')==1
  commit=row['base_commit'];assert re.fullmatch('[0-9a-f]{40}',commit)
  gold=json.loads(row['gold_context']);assert isinstance(gold,list) and gold
  spans=[]
  for span in gold:
   r=dict(span)
   try:
    relative=ordinary_relative(span['file'],repo);r['relative']=relative;keys.add((repo,commit,relative))
   except ValueError as error:r['path_refusal']=str(error)
   spans.append(r)
  prepared.append({'ordinal':ordinal,'row':row,'repo':repo,'commit':commit,'spans':spans});legal.add((repo,commit))
 selection={'input_sha256':digest(source.read_bytes()),'TRAIN450_fixed_before_fetch':True,'unique_immutable_source_files':len(keys),'unique_repository_commits':len(legal),'repository_identifiers':len({p['repo'].lower() for p in prepared}),'source_transfer_cap':LIMIT,'only_exact_assigned_TRAIN_and_original_basecommit':True}
 (OUT/'pre-fetch-selection.json').write_text(json.dumps(selection,indent=2)+'\n',encoding='utf-8');print('SELECTION',json.dumps(selection),flush=True)
 with concurrent.futures.ThreadPoolExecutor(max_workers=12) as pool:observed=list(pool.map(source_job,sorted(keys)))
 bykey={tuple(x['key']):x for x in observed};(OUT/'source-observations.json').write_text(json.dumps(observed,indent=2)+'\n',encoding='utf-8');print('SOURCE FILES',sum('pin'in x for x in observed),'refused',sum('refusal'in x for x in observed),'bytes',transferred,flush=True)
 with concurrent.futures.ThreadPoolExecutor(max_workers=12) as pool:rights=list(pool.map(rights_job,sorted(legal)))
 byrights={tuple(x['key']):x for x in rights};(OUT/'rights-observations.json').write_text(json.dumps(rights,indent=2)+'\n',encoding='utf-8');print('RIGHTS',sum(bool(x['pins']) for x in rights),'of',len(rights),flush=True)
 results=[];counts=collections.Counter()
 for case in prepared:
  spanresults=[];row=case['row']
  for span in case['spans']:
   record={'original_annotation':span,'declared_span_match':False,'strict_derived_span_match':False}
   try:
    if 'path_refusal'in span:raise ValueError(span['path_refusal'])
    sourceobs=bykey[(case['repo'],case['commit'],span['relative'])]
    if 'refusal'in sourceobs:raise ValueError(sourceobs['refusal'])
    pin=sourceobs['pin'];raw=pathlib.Path(pin['path']).read_bytes();assert digest(raw)==pin['sha256'];text=raw.decode('utf-8');start,end=span['start_line'],span['end_line'];assert type(start)is int and type(end)is int and 1<=start<=end
    content,formatter=decode_formatter(span['content'],start,end);record['formatter']=formatter;record['source_pin']=pin
    lines=text.splitlines();literal='\n'.join(lines[start-1:end]);normal=lambda s:s.replace('\r\n','\n').strip('\r\n')
    record['declared_span_match']=end<=len(lines) and normal(content)==normal(literal)
    if not record['declared_span_match'] and formatter=='none' and content:
     positions=[];position=0
     while (found:=text.find(content,position))>=0:positions.append(found);position=found+1
     if len(positions)==1:
      begin=positions[0];finish=begin+len(content);bounds=(begin==0 or text[begin-1]=='\n')and(finish==len(text)or content.endswith('\n')or text[finish:finish+1]in['\n','\r'])
      if bounds:
       beginbytes=len(text[:begin].encode());endbytes=len(text[:finish].encode());assert raw[beginbytes:endbytes]==content.encode()
       record['strict_derived_span_match']=True;record['derived']={'utf8_start':beginbytes,'utf8_end':endbytes,'start_line':text.count('\n',0,begin)+1,'end_line':text.count('\n',0,finish-1)+1,'rule':'entire raw current gold content exactunique same pinned file at line boundaries; original declared range unchanged'}
     record['entire_raw_content_occurrences']=len(positions)
    if not record['declared_span_match']and not record['strict_derived_span_match']:record['refusal']='exact current human content differs/absent/nonunique; no fuzzy or revision substitution'
   except Exception as error:record['refusal']=type(error).__name__+': '+str(error)
   counts['spans']+=1;counts['declared_aligned_spans']+=record['declared_span_match'];counts['strict_derived_aligned_spans']+=record['strict_derived_span_match'];spanresults.append(record)
  complete=all(s['declared_span_match']or s['strict_derived_span_match']for s in spanresults);multifile=len({s.get('relative',s['file'])for s in case['spans']})>1;legalobs=byrights[(case['repo'],case['commit'])];recognized=any(p['kind']in['MIT','Apache-2.0','BSD-family']for p in legalobs['pins']);counts['whole_cases_source_complete']+=complete;counts['multi_file_cases_source_complete']+=complete and multifile;counts['whole_cases_source_complete_recognized_rights_candidate']+=complete and recognized;counts['multi_file_cases_source_complete_recognized_rights_candidate']+=complete and multifile and recognized
  results.append({'original_inst_id':row['original_inst_id'],'instance_id':row['instance_id'],'question_sha256':digest(row['problem_statement'].encode()),'repo':case['repo'],'base_commit':case['commit'],'language':row['language'],'source':row['source'],'whole_case_source_complete':complete,'multi_file':multifile,'recognized_root_rights_candidate':recognized,'rights':legalobs,'spans':spanresults,'native_admitted':False,'human_annotation_authority':'current pinned publisher JSON; original historical annotation/ranges separately retained'})
 receipt={'schema':'airk.private.contextbench450-original-source-rights-alignment/1','source_input_sha256':digest(source.read_bytes()),'population':450,'counts':dict(counts),'unique_source_files_observed':sum('pin'in x for x in observed),'source_file_refusals':sum('refusal'in x for x in observed),'transfer_bytes':transferred,'elapsed_seconds':time.monotonic()-started,'native_admitted':0,'no_gradient_model_scoring_nonTRAIN_question_or_context_use':True,'source_ready_is_not_native_or_final_label_acceptance':True,'results':results}
 (OUT/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print('RESULT',json.dumps({k:v for k,v in receipt.items()if k!='results'}),flush=True);print('RECEIPTSHA',digest((OUT/'receipt.json').read_bytes()),flush=True)
if __name__=='__main__':main()
