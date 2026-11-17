# SPDX-License-Identifier: Apache-2.0
"""Prepare a derivative seal for ONE separately released synthetic prerequisite.
Does not execute a process, mutate old seal or authorize encoder training.
"""
from pathlib import Path
import argparse,hashlib,json
def require_private_root(value):
    if not value:
        raise ValueError('explicit private root required')
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    private = Path(value).resolve()
    if not private.is_relative_to(root/'artifacts'):
        raise ValueError('private root must remain beneath repository artifacts; tracked or external roots refused')
    return private


HERE=Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str((next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file()))/'artifacts/advanced-split/models'))).resolve()/'encoder-recovery-preparation-r1'

def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def natural_owned(receipt):
 r=receipt.get('owned') or {}
 return (receipt.get('failure') is None and r.get('exit_code')==0 and r.get('timed_out') is False and r.get('log_oversized') is False and r.get('owned_descendants_settled') is True and r.get('owned_job_forced_termination') is False and r.get('owned_job_final_active_processes')==0 and r.get('owned_job_final_member_pids')==[] and r.get('owned_total_processes')==2 and r.get('unexpected_third_process') is False and len(r.get('retained_member_exits',[]))==2 and all(v.get('exit_observed') is True and v.get('exit_code')==0 for v in r.get('retained_member_exits',[])))
def main():
 require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS'))
 for name in ('HERE','PREP','H','MODELS'):
  if name in globals():require_private_root(globals()[name])
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--mode',choices=['cuda','resource'],required=True);p.add_argument('--protocol-sha256',required=True);p.add_argument('--release',type=Path,required=True);p.add_argument('--release-sha256',required=True);p.add_argument('--cuda-equivalence',type=Path);p.add_argument('--cuda-equivalence-sha256');p.add_argument('--loss-source-sha256',required=True);a=p.parse_args();loss_path=Path(__file__).resolve().parent.parent/'parent_bag_r1/bag_loss.py'
 if sha(loss_path)!=a.loss_source_sha256:raise ValueError('explicit maintained loss source pin differs')
 proto=HERE/'protocol.json'
 if sha(proto)!=a.protocol_sha256 or sha(a.release)!=a.release_sha256:raise ValueError('primary reviewed input changed')
 packet=json.loads(proto.read_bytes());release=json.loads(a.release.read_bytes());script=Path(__file__).resolve().parent/('check_equivalence.py' if a.mode=='cuda' else 'resource_pilot.py');action='tiny-synthetic-cuda-equivalence' if a.mode=='cuda' else 'synthetic-worst-shape-bert-resource'
 if release.get('protocol_sha256')!=a.protocol_sha256 or release.get('action')!=action or release.get('script_sha256')!=sha(script):raise ValueError('wrong separate primary stage release')
 old=Path(packet['runtime_seal'])
 if sha(old)!=packet['runtime_seal_sha256']:raise ValueError('retained runtime seal changed')
 result=json.loads(old.read_bytes());target=HERE/('execution-seal-'+a.mode+'.json');output=HERE/('cuda-equivalence.json' if a.mode=='cuda' else 'resource-pilot.json');log=HERE/('owned-'+a.mode);cache=HERE/('disabled-pycache-'+a.mode)
 if any(p.exists() for p in [target,output,log,cache]):raise ValueError('fresh non-overwrite stage paths required')
 extras={str(loss_path):a.loss_source_sha256,str(proto):a.protocol_sha256,str(script):sha(script),str(Path(__file__).resolve()):sha(__file__),str(a.release.resolve()):a.release_sha256,**packet['pins']}
 args=['--loss-source-sha256',a.loss_source_sha256,'--protocol-sha256',a.protocol_sha256,'--release',str(a.release.resolve()),'--release-sha256',a.release_sha256,'--output',str(output),'--execution-seal',str(target),'--execution-seal-sha256','<SEAL_SHA256>']
 if a.mode=='cuda':args+=['--device','cuda']
 else:
  if not a.cuda_equivalence or sha(a.cuda_equivalence)!=a.cuda_equivalence_sha256:raise ValueError('actual CUDA proof required')
  proof=json.loads(a.cuda_equivalence.read_bytes())
  if proof.get('complete') is not True or proof.get('device')!='cuda' or proof.get('protocol_sha256')!=a.protocol_sha256 or proof.get('selected')!=6 or proof.get('passed')!=6:raise ValueError('same-protocol CUDA controls must pass')
  closure=HERE/'owned-cuda/execution-receipt.json'
  owned=json.loads(closure.read_bytes())
  if not natural_owned(owned):raise ValueError('natural owned CUDA closure required')
  extras[str(a.cuda_equivalence.resolve())]=a.cuda_equivalence_sha256;extras[str(closure)]=sha(closure);args+=['--cuda-equivalence',str(a.cuda_equivalence.resolve()),'--cuda-equivalence-sha256',a.cuda_equivalence_sha256]
 indexed={str(Path(f['path']).resolve()).lower():f for f in result['files']}
 for file,pin in extras.items():
  if sha(file)!=pin:raise ValueError('new-stage input changed: '+file)
  k=str(Path(file).resolve()).lower()
  if k in indexed and indexed[k]['sha256']!=pin:raise ValueError('conflicting runtime/source pin')
  if k not in indexed:result['files'].append({'path':str(Path(file).resolve()),'sha256':pin,'bytes':Path(file).stat().st_size})
 result['file_roles']['trainer']={'path':str(script),'sha256':sha(script),'bytes':script.stat().st_size};result['environment']['PYTHONPYCACHEPREFIX']=str(cache);result['environment']['PYTHONHASHSEED']='1908';result['environment']['CUDA_VISIBLE_DEVICES']='0';result['environment']['CUBLAS_WORKSPACE_CONFIG']=':4096:8';result['command']=[result['file_roles']['venv_launcher']['path'],'-B','-X','pycache_prefix='+str(cache),str(script),*args];result['launch_log_dir']=str(log);result['status']='PRIMARY_RELEASED_SYNTHETIC_PREREQUISITE_ONLY';result['candidate']={'encoder_training':False,'task_examples':False,'optimizer_on_real_encoder_data':False,'action':action};result['limits']={'wall_seconds':600,'host_committed_bytes':8589934592,'cuda_allocator_bytes':4294967296,'minimum_gpu_free_mib':8192,'owned_process_slots':2};result['preparation_parent']={'protocol_sha256':a.protocol_sha256,'release_sha256':a.release_sha256,'old_runtime_seal_sha256':packet['runtime_seal_sha256'],'runtime_pins_reused_not_reverified_by_preparer':True};result['environment']['AIRK_RESEARCH_MODELS']=str(require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS')));target.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps({'seal':str(target),'sha256':sha(target),'log_dir':str(log),'action':action,'executed':False}))
if __name__=='__main__':main()
