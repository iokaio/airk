# SPDX-License-Identifier: Apache-2.0
"""Read-only historical checkpoint section and custody validation."""
import datetime,math
CAMPAIGN=datetime.datetime(2026,11,19,5,29,42,tzinfo=datetime.timezone.utc)

def require_complete_tree(command):
 tree=command.get('actual_tree_result')
 if not isinstance(tree,dict) or tree.get('schema')!='airk.private.process-tree-result/1' or tree.get('exit_code')!=0 or tree.get('wait')!='Start-Process-Wait-process-and-descendants':raise ValueError('actual original complete process-tree result required')
 if command.get('owned_job_final_active_processes')!=0 or command.get('owned_job_final_member_pids')!=[]:raise ValueError('actual final original Job0 required')

def verify_section(value,index,o,plan_sha):
 expected=['flash-remainder','llm-remainder','final-provider-all-closure'][index]
 require_complete_tree(value.get('command',{}))
 if value.get('schema')!='airk.private.cuda-checkpoint-section/1' or value.get('status')!='PASS_TARGET_SECTION_ONLY_UNQUALIFIED' or value.get('qualification') is not False or value.get('section_id')!=expected or value.get('section_index')!=index or value.get('plan',{}).get('sha256')!=plan_sha or not o.successful(value.get('command',{})):raise ValueError('actual natural predecessor target success required')
 first=datetime.datetime.fromisoformat(value['started_utc']);last=datetime.datetime.fromisoformat(value['completed_utc']);cutoff=datetime.datetime.fromisoformat(value['absolute_cutoff_utc'])
 if value.get('original_section_seconds')!=10800 or cutoff!=min(first+datetime.timedelta(seconds=10800),CAMPAIGN) or not first<=last<=cutoff or value.get('elapsed_seconds',10801)>10800:raise ValueError('predecessor fixed section/campaign deadline refused')
 for key in ['plan','runner','input_contract','warm_checkpoint_before','warm_checkpoint_after','source_manifest_before','source_manifest_after']:o.verify_file(o.plain(value[key]['path'],regular=True),value[key])
 if o.read_json(value['source_manifest_before']['path'])!=o.read_json(value['source_manifest_after']['path']):raise ValueError('predecessor source/tool/config changed')
 if index==0 and (value['runner']['sha256']!='b382a216d1a46a1245b6fc3f78289e9abb61a027eed5f86699713c0b4db0d619' or value['input_contract']['sha256']!='68a5f3109e3ad78a5453a966fd1d00db4217096536f7b7f7253ef309d9a396eb'):raise ValueError('exact first section source/input bridge required')
def same_bytes(a,b):return (a['bytes'],a['sha256'])==(b['bytes'],b['sha256'])

def normalized_custody(o,index,section,all_sections):
 if index==0:
  declared=all_sections[1]['predecessor_custody'];o.verify_file(o.plain(declared['path'],regular=True),declared);body=o.read_json(declared['path'])
  if body.get('schema')!='airk.private.cuda-first-section-custody/1' or body.get('phase_receipt_unchanged') is not True:raise ValueError('explicit first immutable custody bridge required')
  result={}
  for name in ['ninja_log','ninja_deps']:
   row=next(r for r in body['rows'] if r['kind']==name)
   if row['before_original']!=section[name+'_before'] or row['after_original']!=section[name+'_after'] or not same_bytes(row['before_original'],row['before_copy']) or not same_bytes(row['after_original'],row['after_copy']):raise ValueError('first original/copied Ninja identities differ')
   result['before_'+('log' if name=='ninja_log' else 'deps')]=row['before_copy'];result['after_'+('log' if name=='ninja_log' else 'deps')]=row['after_copy']
 else:result={'before_log':section['ninja_log_before'],'before_deps':section['ninja_deps_before'],'after_log':section['ninja_log_after'],'after_deps':section['ninja_deps_after']}
 for row in result.values():o.verify_file(o.plain(row['path'],regular=True),row)
 return result

def validate_sections(o,helper,rows,expected_successor_source,expected_successor_input,plan_sha):
 if len(rows)!=3:raise ValueError('exact three ordered actual sections required')
 bodies=[]
 for index,row in enumerate(rows):
  o.verify_file(o.plain(row['path'],regular=True),row);body=o.read_json(row['path']);helper.verify_section(body,index,o,plan_sha)
  if body.get('cold_build_qualified') is not False or not math.isfinite(body['elapsed_seconds']) or body['elapsed_seconds']<0:raise ValueError('no cold qualification/invalid elapsed')
  if index and (body['runner']['sha256']!=expected_successor_source or body['input_contract']['sha256']!=expected_successor_input or body.get('predecessor')!=rows[index-1]):raise ValueError('exact successor/predecessor source ancestry differs')
  if index and body['warm_checkpoint_before']!=bodies[-1]['warm_checkpoint_after']:raise ValueError('warm checkpoint chain differs')
  if index and o.read_json(body['source_manifest_after']['path'])!=o.read_json(bodies[0]['source_manifest_after']['path']):raise ValueError('section source/tool/config changed')
  bodies.append(body)
 return bodies
