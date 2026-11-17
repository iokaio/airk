# SPDX-License-Identifier: Apache-2.0
"""Read actual independent executions/assemblies; no scoring or qualification."""
import hashlib,json,math
from pathlib import Path
ROOT=Path(__file__).resolve().parent

def read(p):return json.loads(Path(p).read_bytes())
def pin(p):
 p=Path(p).resolve(strict=True)
 return {'path':str(p),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}

def main():
 batch=read(ROOT/'batch-observations.json');assert batch['complete'] and not batch['failed'] and len(batch['results'])==6
 results=[]
 for observed in batch['results']:
  assert observed['exit_code']==0 and observed['failure'] is None
  directory=Path(observed['output']);run=read(directory/'execution.json');raw=read(directory/'torch-output.json')
  assert pin(directory/'execution.json')['sha256']==observed['actual_execution']['sha256']
  assert run['complete'] is True and run['failure'] is None and not run['whole_phase_expired'] and run['post_input_drift']==[]
  assert run['owned']['exit_code']==0 and run['owned']['owned_total_processes']==1 and run['owned']['owned_descendants_settled'] is True
  assert run['owned']['owned_job_forced_termination'] is False and run['owned']['owned_job_final_active_processes']==0 and run['owned']['owned_job_final_member_pids']==[]
  assert run['owned']['retained_member_exits']==[{'exit_observed':True,'exit_code':0}]
  close=run['owned']['handle_close_proof']
  assert close['settled'] is True and close['forced'] is False and close['members_before']==[] and close['final_active']==0 and close['final_member_pids']==[]
  assert raw['complete'] is True and raw['rows']==run['rows'] and run['torch_forward_invocations']==len(raw['rows'])
  assert all(math.isfinite(score) for row in raw['rows'] for score in row['scores'])
  assert raw['cuda_observation']['uuid']=='08e2b3f6-6a39-3456-77cb-a57287a702ec'
  results.append({'phase':observed['phase'],'reference_kind':run['reference_kind'],'execution':pin(directory/'execution.json'),'raw_output':pin(directory/'torch-output.json'),
   'source_plan':observed['plan'],'primary_review':observed['primary_review'],'actual_forward_invocations':run['torch_forward_invocations'],
   'parent_elapsed_seconds':run['elapsed_seconds'],'wrapper_elapsed_seconds':observed['elapsed_seconds'],
   'peak_host_committed_bytes':run['owned']['peak_host_committed_bytes'],'peak_cuda_allocated_bytes':raw['peak_cuda_allocated_bytes'],'peak_cuda_reserved_bytes':raw['peak_cuda_reserved_bytes'],
   'rows':[{'name':row['name'],'tokens':len(row['input']['input_ids']),'candidates':len(row['scores']),'forward_elapsed_seconds':row['forward_elapsed_seconds']} for row in raw['rows']],
   'actual_natural_single_child_empty_job_and_handle_close':True,'post_input_drift':run['post_input_drift']})
 assemblies=[]
 for tier in ['focus','compact','large']:
  directory=ROOT/('assembled-'+tier);projection=read(directory/'assembly.json');producer=read(directory/'reference-producer.json');numeric_input=read(directory/'resource-run-input-numeric.private.json')
  assert projection['qualification_granted'] is False and producer['qualification'] is False
  assert numeric_input['numeric_fixture']==projection['fixture']
  assert producer['rows']==read(directory/'numeric-fixture.json')['rows']
  assemblies.append({'tier':tier,'producer':pin(directory/'reference-producer.json'),'fixture':projection['fixture'],'numeric_input':pin(directory/'resource-run-input-numeric.private.json'),
   'assembly':pin(directory/'assembly.json'),'opaque_input_preparation':pin(directory/'numeric-input-preparation.json'),'reference_executions':producer['reference_executions']})
 output={'schema':'airk.private.completed-independent-reference-campaign/1','complete':True,'successful_independent_executions':len(results),
  'successful_selected_forward_invocations':sum(row['actual_forward_invocations'] for row in results),'executions':results,'assemblies':assemblies,
  'original_failed_attempt':pin(ROOT.parent/'focus-original_fp32/execution.json'),'failed_attempt_disposition':'Original global-site startup refusal retained as failure; source correction independently reviewed before fresh R2 attempts.',
  'native_airk_qwen_forward_invocations_by_this_campaign':0,'semantic_gold_created':False,'training_performed':False,'qualification_granted':False,
  'pending':['Native numerical comparison with actual current image/source custody.','Independent native resource/p95 and product/platform qualification.','Human labels remain a separate dependent training/promotion gate.']}
 destination=ROOT/'reference-campaign-summary.json'
 with destination.open('x',encoding='utf-8') as stream:json.dump(output,stream,indent=2);stream.write('\n')
 print(json.dumps({'summary':pin(destination),'successful_independent_executions':len(results),'successful_forward_invocations':output['successful_selected_forward_invocations']}))
if __name__=='__main__':main()