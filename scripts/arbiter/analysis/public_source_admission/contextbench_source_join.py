# SPDX-License-Identifier: Apache-2.0
"""Read-only source/rights join for a fixed twenty-family publisher-TRAIN slice. No native examples or scoring."""
import concurrent.futures,collections,hashlib,json,pathlib,re,urllib.request,urllib.error
import pyarrow.parquet as pq
ROOT=pathlib.Path(__file__).resolve().parent
OUT=ROOT/'contextbench-source-join-r1'
SELECTED={'astropy/astropy','axios/axios','tokio-rs/bytes','clap-rs/clap','cli/cli','coder/code-server','vuejs/core','darkreader/darkreader','iamkun/dayjs','django/django','expressjs/express','alibaba/fastjson2','flipt-io/flipt','fmtlib/fmt','zeromicro/go-zero','grpc/grpc-go','google/gson','huggingface/transformers','keras-team/keras','langchain-ai/langchain'}
PATTERN=re.compile(r'(?m)^context\d+[\uff1a:]\s*\nfile:\s*([^\n]+)\nstart_line:\s*(\d+)\nend_line:\s*(\d+)\n')
def sha(raw):return hashlib.sha256(raw).hexdigest()
def fetch(url,cap):
 try:
  with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'AIRK-approved-human-source-rights-audit'}),timeout=25) as response:
   raw=response.read(cap+1)
   if len(raw)>cap:raise ValueError('source audit file cap exceeded')
   return raw
 except urllib.error.HTTPError as error:
  if error.code==404:return None
  raise

def canonical(row):
 if '/' in row['repo']:return row['repo']
 owner,rest=row['original_inst_id'].split('__',1)
 return owner+'/'+rest.rsplit('-',1)[0]

def inspect(item):
 ordinal,row,repo=item;matches=list(PATTERN.finditer(row['gold_context']));commit=row['base_commit']
 assert len(commit)==40 and all(c in '0123456789abcdef' for c in commit)
 entry={'ordinal':ordinal,'instance_id':row['instance_id'],'original_inst_id':row['original_inst_id'],'declared_repo':row['repo'],'resolved_repo':repo,'resolution':'exact declared owner/repo' if '/' in row['repo'] else 'publisher original_inst_id owner__repository suffix; validated by immutable source URL','base_commit':commit,'language':row['language'],'source':row['source'],'source_files':[],'span_results':[],'rights_files':[],'failures':[]}
 destination=OUT/(repo.replace('/','--')+'--'+commit);destination.mkdir()
 def retain(relative,raw,url):
  name=pathlib.PurePosixPath(relative)
  assert not name.is_absolute() and '..' not in name.parts and ':' not in relative
  path=destination/pathlib.Path(*name.parts);path.parent.mkdir(parents=True,exist_ok=True)
  with path.open('xb') as target:target.write(raw)
  return {'upstream_path':relative,'path':str(path),'url':url,'sha256':sha(raw),'bytes':len(raw)}
 files={m[1] for m in matches};opened={}
 for name in sorted(files):
  try:
   url='https://raw.githubusercontent.com/'+repo+'/'+commit+'/'+name;raw=fetch(url,2_000_000)
   if raw is None:raise ValueError('immutable source path404')
   pin=retain('source/'+name,raw,url);pin['upstream_path']=name;pin['spdx_in_first8192']=re.findall(r'SPDX-License-Identifier:\s*([^\r\n*]+)',raw[:8192].decode('utf-8',errors='replace'))
   entry['source_files'].append(pin);opened[name]=raw
  except Exception as error:entry['failures'].append({'file':name,'type':type(error).__name__,'message':str(error)})
 for index,m in enumerate(matches):
  annotation=row['gold_context'][m.end():(matches[index+1].start() if index+1<len(matches) else len(row['gold_context']))]
  begin,end=int(m[2]),int(m[3]);record={'file':m[1],'start_line':begin,'end_line':end,'annotation_sha256':sha(annotation.encode()),'annotation_bytes':len(annotation.encode()),'alignment':'source unavailable'}
  if m[1] in opened:
   try:
    lines=opened[m[1]].decode('utf-8').splitlines(keepends=True);literal=''.join(lines[begin-1:end]);record['original_span_sha256']=sha(literal.encode())
    valid=1<=begin<=end<=len(lines)
    normalized=lambda text:text.replace('\r\n','\n').strip('\r\n')
    record['exact_text_equal']=annotation==literal
    record['alignment']='line-ending/outer-linebreak equivalent' if valid and normalized(annotation)==normalized(literal) else 'MISMATCH'
   except Exception as error:record['alignment']='decode/refusal';record['reason']=str(error)
  entry['span_results'].append(record)
 for name in ['LICENSE','LICENSE.txt','LICENSE.md','COPYING','NOTICE']:
  try:
   url='https://raw.githubusercontent.com/'+repo+'/'+commit+'/'+name;raw=fetch(url,250000)
   if raw is None:continue
   pin=retain('rights/'+name,raw,url);pin['upstream_path']=name
   text=raw.decode('utf-8',errors='replace')
   pin['recognized_notice']='Apache-2.0' if 'Apache License' in text and 'Version 2.0' in text else 'MIT' if 'Permission is hereby granted, free of charge' in text else 'BSD-family' if 'Redistribution and use in source and binary forms' in text else 'other notice/manual review'
   entry['rights_files'].append(pin)
  except Exception as error:entry['failures'].append({'rights_file':name,'type':type(error).__name__,'message':str(error)})
 entry['source_span_join_passed']=bool(matches) and all(s['alignment']=='line-ending/outer-linebreak equivalent' for s in entry['span_results'])
 entry['root_license_retained']=any(x['upstream_path'] in ['LICENSE','LICENSE.txt','LICENSE.md','COPYING'] for x in entry['rights_files'])
 return entry

def main():
 assert not OUT.exists(),'fresh immutable source audit required';OUT.mkdir()
 rows=pq.read_table(ROOT/'contextbench-train.parquet').to_pylist();selected={}
 for index,row in enumerate(rows):
  repo=canonical(row)
  if repo in SELECTED and repo not in selected:selected[repo]=(index,row,repo)
 assert set(selected)==SELECTED,(SELECTED-set(selected))
 # These families are all publisher TRAIN. No unseen family is assigned FINAL here.
 (OUT/'pre-fetch-selection.json').write_text(json.dumps({'publisher_partition':'data/train.parquet','selection':'first publisher ordinal in each explicit twenty family set; no label/score selection','families':sorted(SELECTED),'ordinals':{r:v[0] for r,v in selected.items()},'use':'source/rights audit only; no admission or gradient'},indent=2)+'\n',encoding='utf-8')
 with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:results=list(pool.map(inspect,selected.values()))
 aggregate={'schema':'airk.private.contextbench-original-train-source-join/1','source_file_policy':'actual pinned basecommit bytes; UTF8 +CRLF to LF and outer newline-only comparison explicitly disclosed','publisher_train_rows':450,'selected_families':20,'selected_tasks':len(results),'source_span_join_passed':sum(r['source_span_join_passed'] for r in results),'root_license_retained':sum(r['root_license_retained'] for r in results),'source_files':sum(len(r['source_files']) for r in results),'span_count':sum(len(r['span_results']) for r in results),'failures':sum(len(r['failures']) for r in results),'dataset_admitted':False,'native_visible_alignment':False,'training_scoring_final_access':False,'results':results}
 (OUT/'receipt.json').write_text(json.dumps(aggregate,indent=2)+'\n',encoding='utf-8')
 print(json.dumps({k:v for k,v in aggregate.items() if k!='results'}))
 for r in results:print(r['resolved_repo'],'spans',len(r['span_results']),'joined',r['source_span_join_passed'],'rights',[p['recognized_notice'] for p in r['rights_files']],'failures',len(r['failures']))
if __name__=='__main__':main()
