# SPDX-License-Identifier: Apache-2.0
"""Prepare a separately released resource stage from the successful amended CUDA R2 seal.
No execution, old-seal overwrite, runtime pin removal or candidate gradient release.
"""
from pathlib import Path
import argparse,copy,hashlib,json,os
def require_private_root(value):
    if not value:
        raise ValueError('explicit private root required')
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    private = Path(value).resolve()
    if not private.is_relative_to(root/'artifacts'):
        raise ValueError('private root must remain beneath repository artifacts; tracked or external roots refused')
    return private


HERE=Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str((next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file()))/'artifacts/advanced-split/models'))).resolve()/'encoder-recovery-preparation-r1'
PINS={'execution-seal-cuda-r2.json':'5c81890d6e0459662b877c91536a89d91a4924a2b00e74eab8f92034d98e1f56','cuda-equivalence-r2.json':'dee5b2378ca8206b66f8b0a39a02e6611fa94df0870fa13753cbd53592919136','owned-cuda-r2/execution-receipt.json':'ddd28ca3831eecbe1b8a86f6c9da90e7e6a62702bb385da395b280ee323bcc06','protocol.json':'eb6f60be6af095c2c00ec091a0afb1668dc5e02af0cb5049a4b2a1540bcfaa2a','resource_pilot.py':'16edc28f24d5b214f50a1cd7c279774a94d6c760d59f44210dc4cab2d8607d75'}
LIMITS={'wall_seconds':600,'host_committed_bytes':8589934592,'cuda_allocator_bytes':4294967296,'minimum_gpu_free_mib':8192,'owned_process_slots':2}
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def natural(receipt):
 r=receipt.get('owned') or {}
 return receipt.get('failure') is None and r.get('exit_code')==0 and r.get('timed_out') is False and r.get('log_oversized') is False and r.get('owned_descendants_settled') is True and r.get('owned_job_forced_termination') is False and r.get('owned_job_final_active_processes')==0 and r.get('owned_job_final_member_pids')==[] and r.get('owned_total_processes')==2 and r.get('unexpected_third_process') is False and len(r.get('retained_member_exits',[]))==2 and all(v.get('exit_observed') is True and v.get('exit_code')==0 for v in r.get('retained_member_exits',[]))
def validate_cuda(seal,proof,receipt,seal_sha,expected_proof_path,expected_seal_path):
 if seal_sha!=PINS['execution-seal-cuda-r2.json'] or receipt.get('seal_sha256')!=seal_sha:raise ValueError('wrong actual CUDA seal/receipt')
 expected=[v.replace('<SEAL_SHA256>',seal_sha) for v in seal['command']]
 if receipt.get('command')!=expected:raise ValueError('actual CUDA command differs from reviewed seal')
 if not natural(receipt) or receipt.get('limits')!=LIMITS or seal.get('limits')!=LIMITS:raise ValueError('actual natural CUDA closure/limits required')
 for flag,path in [('--output',expected_proof_path),('--execution-seal',expected_seal_path)]:
  if expected.count(flag)!=1 or Path(expected[expected.index(flag)+1]).resolve()!=Path(path).resolve():raise ValueError('CUDA command output/seal path differs')
 if (proof.get('complete') is not True or proof.get('device')!='cuda' or proof.get('selected')!=6 or proof.get('passed')!=6 or proof.get('protocol_sha256')!=PINS['protocol.json'] or proof.get('script_sha256')!=seal['file_roles']['trainer']['sha256']):raise ValueError('wrong actual CUDA proof')
 if expected.count('--device')!=1 or expected[expected.index('--device')+1]!='cuda':raise ValueError('CUDA device command required')
def main():
 require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS'))
 for name in ('HERE','PREP','H','MODELS'):
  if name in globals():require_private_root(globals()[name])
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--release',type=Path,required=True);p.add_argument('--release-sha256',required=True);p.add_argument('--loss-source-sha256',required=True);a=p.parse_args();loss_path=Path(__file__).resolve().parent.parent/'parent_bag_r1/bag_loss.py'
 if sha(loss_path)!=a.loss_source_sha256:raise ValueError('explicit maintained loss source pin differs')
 for name,pin in PINS.items():
  if sha(((Path(__file__).resolve().parent if name.endswith('.py') else HERE)/name))!=pin:raise ValueError('frozen actual prerequisite changed: '+name)
 if a.release.resolve()!=HERE/'release-resource-r2.json' or sha(a.release)!=a.release_sha256:raise ValueError('exact separate primary resource release required')
 release=json.loads(a.release.read_bytes())
 if release.get('action')!='synthetic-worst-shape-bert-resource' or release.get('protocol_sha256')!=PINS['protocol.json'] or release.get('script_sha256')!=PINS['resource_pilot.py']:raise ValueError('wrong separate primary resource release')
 base_path=HERE/'execution-seal-cuda-r2.json';proof_path=HERE/'cuda-equivalence-r2.json';receipt_path=HERE/'owned-cuda-r2/execution-receipt.json';base=json.loads(base_path.read_bytes());proof=json.loads(proof_path.read_bytes());receipt=json.loads(receipt_path.read_bytes());validate_cuda(base,proof,receipt,PINS['execution-seal-cuda-r2.json'],proof_path,base_path)
 result=copy.deepcopy(base);target=HERE/'execution-seal-resource-r2.json';output=HERE/'resource-pilot-r2.json';logs=HERE/'owned-resource-r2';cache=HERE/'disabled-pycache-resource-r2'
 if any(v.exists() for v in [target,output,logs,cache]):raise ValueError('fresh resource R2 paths required')
 extras=[loss_path]+[(Path(__file__).resolve().parent if n.endswith('.py') else HERE)/n for n in PINS]+[a.release.resolve(),Path(__file__).resolve()];indexed={os.path.normcase(str(Path(f['path']).resolve())):f for f in result['files']}
 for path in extras:
  pin=sha(path);k=os.path.normcase(str(path.resolve()))
  if k in indexed and indexed[k]['sha256']!=pin:raise ValueError('conflicting inherited input seal: '+str(path))
  if k not in indexed:result['files'].append({'path':str(path.resolve()),'sha256':pin,'bytes':path.stat().st_size})
 if not {f['path'] for f in base['files']}<={f['path'] for f in result['files']}:raise ValueError('inherited runtime pin lost')
 script=Path(__file__).resolve().parent/'resource_pilot.py';result['file_roles']['trainer']={'path':str(script),'sha256':PINS['resource_pilot.py'],'bytes':script.stat().st_size};result['environment']['PYTHONPYCACHEPREFIX']=str(cache);result['environment']['CUBLAS_WORKSPACE_CONFIG']=':4096:8';result['command']=[base['file_roles']['venv_launcher']['path'],'-B','-X','pycache_prefix='+str(cache),str(script),'--loss-source-sha256',a.loss_source_sha256,'--protocol-sha256',PINS['protocol.json'],'--release',str(a.release.resolve()),'--release-sha256',a.release_sha256,'--cuda-equivalence',str(proof_path),'--cuda-equivalence-sha256',PINS['cuda-equivalence-r2.json'],'--output',str(output),'--execution-seal',str(target),'--execution-seal-sha256','<SEAL_SHA256>'];result['launch_log_dir']=str(logs);result['limits']=dict(LIMITS);result['candidate']={'encoder_training':False,'task_examples':False,'optimizer_on_real_encoder_data':False,'action':'synthetic-worst-shape-bert-resource'};result['synthetic_resource_prerequisite']={'base_successful_cuda_seal_sha256':PINS['execution-seal-cuda-r2.json'],'actual_cuda_proof_sha256':PINS['cuda-equivalence-r2.json'],'actual_cuda_owned_receipt_sha256':PINS['owned-cuda-r2/execution-receipt.json'],'primary_resource_release_sha256':a.release_sha256,'all_inherited_current_runtime_pins_preserved':True,'candidate_gradients_released':False};result['environment']['AIRK_RESEARCH_MODELS']=str(require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS')));target.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps({'seal':str(target),'sha256':sha(target),'log_dir':str(logs),'output':str(output),'limits':LIMITS,'executed':False}))
if __name__=='__main__':main()
