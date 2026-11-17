# SPDX-License-Identifier: Apache-2.0
"""Additive owned Qwen tiny seal preparation. Never starts an owned process/NN."""
import argparse,copy,hashlib,importlib.metadata,json,os
from pathlib import Path
HERE=Path(__file__).resolve().parent
PREP=HERE.parent/'encoder-recovery-preparation-r1'
BASE_SHA='fdfedb8a858eb5083a6bc182cab4be75360be15d46b5c57025017f1f5eefcf8a'
OWNED_SHA='a26c11811dead9dd8c8e4cda0914075755d94c22eb61fd7b8ffd89fd0a96a40a'
OWNER_SHA='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'
LIMITS={'wall_seconds':600,'host_committed_bytes':8*1024**3,'cuda_allocator_bytes':4*1024**3,'minimum_gpu_free_mib':8192,'owned_process_slots':2}
SCRIPTS=['qwen3_reference_r2.py','qwen3_pilot_r2.py','qwen3_tiny_worker_r3.py','launch_qwen3_tiny_r3.py','prepare_qwen3_tiny_r3.py','test_qwen3_preparation_r2.py','test_qwen3_owned_r3.py']
METADATA=[HERE/'selected-pipeline-design-r2.json',HERE/'qwen-single-choice-compiler-proposal-r1.json',HERE/'qwen-compiler-source-policy-proposal-r1.json',HERE/'safetensors-header-inventory.json',HERE.parent/'qwen3-challenger-assets-r1/research-asset-pins.json',HERE.parent/'qwen3-challenger-assets-r1/publisher-metadata-inventory.json',HERE.parent/'qwen3-challenger-assets-r1/candidate-assets/acquisition-1794713782830027400.json']
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def natural(r):
 o=r.get('owned') or {};exits=o.get('retained_member_exits',[])
 return (r.get('failure') is None and o.get('exit_code')==0 and o.get('timed_out') is False and o.get('log_oversized') is False and o.get('owned_descendants_settled') is True and o.get('owned_job_forced_termination') is False and o.get('unexpected_third_process') is False and o.get('owned_job_final_active_processes')==0 and o.get('owned_job_final_member_pids')==[] and o.get('owned_total_processes')==2 and len(exits)==2 and all(x.get('exit_observed') is True and x.get('exit_code')==0 for x in exits))
def append_only(base,extras):
 result=copy.deepcopy(base);indexed={os.path.normcase(str(Path(f['path']).resolve())):f for f in result['files']}
 if len(indexed)!=len(result['files']):raise ValueError('duplicate inherited path')
 for path in extras:
  path=Path(path).resolve();pin=sha(path);key=os.path.normcase(str(path))
  row={'path':str(path),'sha256':pin,'bytes':path.stat().st_size}
  if key in indexed:
   if indexed[key]!=row:raise ValueError('conflicting inherited identity')
  else:result['files'].append(row);indexed[key]=row
 if result['files'][:len(base['files'])]!=base['files']:raise ValueError('old pins removed or modified')
 return result

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--release',type=Path,required=True);p.add_argument('--release-sha256',required=True);a=p.parse_args()
 base_path=PREP/'execution-seal-train-r3.json';owned_path=PREP/'owned-train-r3/execution-receipt.json'
 if sha(base_path)!=BASE_SHA or sha(owned_path)!=OWNED_SHA or sha(a.release)!=a.release_sha256:raise ValueError('reviewed actual provenance changed')
 base=json.loads(base_path.read_bytes());owned=json.loads(owned_path.read_bytes());release=json.loads(a.release.read_bytes())
 if len(base['files'])!=49277 or not natural(owned) or owned['seal_sha256']!=BASE_SHA or owned['command']!=[x.replace('<SEAL_SHA256>',BASE_SHA) for x in base['command']]:raise ValueError('exact successful old owned closure required')
 if base['file_roles']['execution_module']['sha256']!=OWNER_SHA or sha(base['file_roles']['execution_module']['path'])!=OWNER_SHA:raise ValueError('original owner identity changed')
 target=HERE/'execution-seal-qwen3-tiny-r3.json';logs=HERE/'owned-qwen3-tiny-r3';cache=HERE/'disabled-pycache-qwen3-tiny-r3';output=HERE/'tiny-result-r3'
 if any(x.exists() for x in [target,logs,cache,output]):raise ValueError('fresh tiny-only paths required')
 sourcepins={str(HERE/n):sha(HERE/n) for n in SCRIPTS};metapins={str(x.resolve()):sha(x) for x in METADATA}
 if (a.release.resolve()!=HERE/'release-qwen3-tiny-r3.json' or release.get('execution_authorized') is not True or release.get('action')!='qwen3-tiny-export' or release.get('authorized_stage')!='qwen3-tiny-export' or release.get('model_id','absent') is not None or release.get('synthetic_shapes')!=[[17,3],[64,4],[128,8]] or release.get('fresh_output')!=str(output) or release.get('source_sha256')!=sourcepins or release.get('metadata_sha256')!=metapins or release.get('limits')!=LIMITS or release.get('production_abi') is not False):raise ValueError('exact separate Root tiny release required')
 extras=[base_path,owned_path,a.release,*[HERE/n for n in SCRIPTS],*METADATA]
 result=append_only(base,extras)
 result['status']='ROOT_RELEASED_QWEN_SYNTHETIC_TINY_ONLY_NOT_PRODUCTION_ABI'
 result['packages'].update({name:importlib.metadata.version(name) for name in ['onnx','onnxruntime']})
 result['environment']['PYTHONPYCACHEPREFIX']=str(cache)
 worker=HERE/'qwen3_tiny_worker_r3.py'
 result['file_roles']['trainer']={'path':str(worker),'sha256':sha(worker),'bytes':worker.stat().st_size}
 result['command']=[base['file_roles']['venv_launcher']['path'],'-B','-X','pycache_prefix='+str(cache),str(worker),'--seal',str(target),'--seal-sha256','<SEAL_SHA256>']
 result['launch_log_dir']=str(logs);result['limits']=dict(LIMITS)
 result['inherited_candidate_historical_only']=result['candidate'];result['candidate']={'stage':'qwen3 synthetic tiny full-prefill','weights_loaded':False,'task_text':False,'training':False,'production_abi':False,'release_or_promotion_qualified':False}
 result['qwen_tiny_stage']={'release_path':str(a.release.resolve()),'release_sha256':a.release_sha256,'pilot_path':str(HERE/'qwen3_pilot_r2.py'),'assets_root':str(HERE.parent/'qwen3-challenger-assets-r1'),'inherited_seal_sha256':BASE_SHA,'actual_natural_owned_receipt_sha256':OWNED_SHA,'all_49277_inherited_pins_preserved':True,'metadata_sha256':metapins,'diagnostic_hidden_states_output_not_production':True,'production_requires_scores_only_with_same_six_inputs':True,'no_training_or_full_model_authorization':True}
 target.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
 print(json.dumps({'seal':str(target),'sha256':sha(target),'inherited_files':len(base['files']),'added_files':len(result['files'])-len(base['files']),'root_release_sha256':a.release_sha256,'launch_command':[base['file_roles']['venv_launcher']['path'],'-B',str(HERE/'launch_qwen3_tiny_r3.py'),'--seal',str(target),'--seal-sha256',sha(target),'--log-dir',str(logs)],'NN_executed':False}))
if __name__=='__main__':main()
