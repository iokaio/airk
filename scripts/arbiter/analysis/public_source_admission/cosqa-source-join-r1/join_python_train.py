# SPDX-License-Identifier: Apache-2.0
"""Exact CoSQA literal-code join against pinned CSN Python TRAIN, without models."""
import ast,collections,hashlib,json,re,time
from pathlib import Path
import pyarrow.parquet as pq
ROOT=Path(__file__).resolve().parent
sha=lambda value:hashlib.sha256(value).hexdigest()
train=ROOT.parent/'cosqa-train.json';codemap=ROOT.parent/'code_idx_map.txt'
assert sha(train.read_bytes())=='dc04b32c222ef320eb32b8a8ed791b1f19ba0f39d8d299e75675dacfbe27a7e5'
assert sha(codemap.read_bytes())=='be8a910ca03cb9f1b38a0eba6dc8109eeb8040a40aaba1a467a7b8426d261f3f'
targets=ast.literal_eval(codemap.read_text());questions=json.loads(train.read_bytes());assert len(targets)==6267 and len(questions)==20000
parquet=pq.ParquetFile(ROOT/'codesearchnet-python-TRAIN.parquet')
columns=['repository_name','func_path_in_repository','func_name','whole_func_string','func_code_string','split_name','func_code_url']
assert all(name in parquet.schema_arrow.names for name in columns)
counts=collections.Counter();matches=collections.defaultdict(list);started=time.monotonic();source_index=0
with (ROOT/'literal-source-alternatives.jsonl').open('x',encoding='utf-8') as output:
 for batch in parquet.iter_batches(batch_size=256,columns=columns,use_threads=False):
  for row in batch.to_pylist():
   if row['split_name']!='train':raise ValueError('non-TRAIN data in publisher TRAIN file; refused')
   found=collections.defaultdict(list)
   for field in ['whole_func_string','func_code_string']:
    value=row[field]
    if value in targets:found[value].append(field)
   for literal,fields in found.items():
    match=re.fullmatch(r'https://github\.com/([^/]+/[^/]+)/blob/([0-9a-fA-F]{40})/(.+?)(?:#L([0-9]+)(?:-L([0-9]+))?)?',row['func_code_url'])
    parsed={'url_repo':match.group(1),'commit':match.group(2).lower(),'url_path':match.group(3),'line_start':int(match.group(4)) if match.group(4) else None,'line_end':int(match.group(5) or match.group(4)) if match.group(4) else None} if match else None
    record={'code_idx':targets[literal],'code_sha256':sha(literal.encode()),'codesearchnet_train_row':source_index,'matching_literal_fields':fields,
     'repository':row['repository_name'],'path':row['func_path_in_repository'],'function':row['func_name'],'source_url':row['func_code_url'],'parsed_source':parsed,
     'whole_function_sha256':sha(row['whole_func_string'].encode()),'code_field_sha256':sha(row['func_code_string'].encode()),'whole_function_matches_labeled_literal':row['whole_func_string']==literal,'split':'TRAIN'}
    matches[targets[literal]].append(record);output.write(json.dumps(record,ensure_ascii=False)+'\n')
   source_index+=1
  if source_index%16384==0:print(json.dumps({'scanned_train_rows':source_index,'matched_code_indices':len(matches)}),flush=True)
source_records=[];repos=collections.Counter();taskcounts=collections.Counter();repo_positive=collections.Counter();by_idx={value:key for key,value in targets.items()}
for index,literal in sorted(by_idx.items()):
 options=matches.get(index,[]);status='unmatched'
 if len(options)>1:status='ambiguous_multiple_source_rows'
 elif len(options)==1:
  source=options[0];parsed=source['parsed_source']
  status='unique_literal_source' if parsed and parsed['url_repo'].lower()==source['repository'].lower() and parsed['url_path']==source['path'] else 'unique_literal_unverified_url'
  if status=='unique_literal_source':repos[source['repository']]+=1
 source_records.append({'code_idx':index,'code_sha256':sha(literal.encode()),'status':status,'source_alternatives':options})
statusmap={row['code_idx']:row for row in source_records}
for question in questions:
 index=targets.get(question['code']);assert index is not None
 status=statusmap[index]['status'];taskcounts[status]+=1
 if status=='unique_literal_source' and question['label']==1:repo_positive[statusmap[index]['source_alternatives'][0]['repository']]+=1
with (ROOT/'literal-source-join.json').open('x',encoding='utf-8') as output:json.dump({'schema':'airk.private.cosqa-csn-literal-source-join/1','sources':source_records,'literal_normalization_performed':False,'unmatched_or_multiple_matches_never_admitted':True},output,ensure_ascii=False,indent=2);output.write('\n')
summary={'complete':True,'source_train_rows_scanned':source_index,'cosqa_function_indices':len(targets),'status_functions':dict(collections.Counter(row['status'] for row in source_records)),
 'status_tasks':dict(taskcounts),'unique_source_repositories':len(repos),'unique_functions_by_repository':dict(repos),'positive_tasks_by_repository':dict(repo_positive),
 'actual_rights_admitted_repositories':0,'rights_status':'Per-repository pinned license/source notices still required; literal match is not rights approval.','elapsed_seconds':time.monotonic()-started,'model_execution':False,'nontrain_text_accessed':False}
with (ROOT/'literal-source-join-summary.json').open('x',encoding='utf-8') as output:json.dump(summary,output,indent=2);output.write('\n')
print(json.dumps({key:value for key,value in summary.items() if key not in ['unique_functions_by_repository','positive_tasks_by_repository']}),flush=True)