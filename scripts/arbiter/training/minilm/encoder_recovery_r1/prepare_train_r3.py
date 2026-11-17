# SPDX-License-Identifier: Apache-2.0
"""Prepare one separately released finite epoch1 TRAIN seal after real resource R2 success.
Does not run training or alter old recipe, trainer, seals, fixture/source data.
"""
from pathlib import Path
import argparse,copy,hashlib,json,os
from prepare_resource_r2 import natural,LIMITS,PINS as RESOURCE_PINS
from train_output_paths import validate_training_output
def require_private_root(value):
    if not value:
        raise ValueError('explicit private root required')
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    private = Path(value).resolve()
    if not private.is_relative_to(root/'artifacts'):
        raise ValueError('private root must remain beneath repository artifacts; tracked or external roots refused')
    return private


HERE=Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str((next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file()))/'artifacts/advanced-split/models'))).resolve()/'encoder-recovery-preparation-r1'
TRAINER_SHA='3f220874eb2698085d2b04fb30379f3431b6ceb800c1a2c002bd321a276bf0df'
PROTOCOL_SHA='eb6f60be6af095c2c00ec091a0afb1668dc5e02af0cb5049a4b2a1540bcfaa2a'
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def validate_resource(seal,proof,receipt,seal_sha):
 if receipt.get('seal_sha256')!=seal_sha or not natural(receipt) or receipt.get('limits')!=LIMITS or seal.get('limits')!=LIMITS:raise ValueError('actual natural matching resource closure required')
 expected=[v.replace('<SEAL_SHA256>',seal_sha) for v in seal['command']]
 if receipt.get('command')!=expected:raise ValueError('actual resource command differs')
 for flag,path in [('--output',HERE/'resource-pilot-r2.json'),('--execution-seal',HERE/'execution-seal-resource-r2.json')]:
  if expected.count(flag)!=1 or Path(expected[expected.index(flag)+1]).resolve()!=path:raise ValueError('resource command output/seal differs')
 if (proof.get('complete') is not True or proof.get('protocol_sha256')!=PROTOCOL_SHA or proof.get('script_sha256')!=RESOURCE_PINS['resource_pilot.py'] or proof.get('cuda_equivalence_sha256')!=RESOURCE_PINS['cuda-equivalence-r2.json'] or proof.get('synthetic_only') is not True or proof.get('pretrained_weights_or_task_text_decoded') is not False or proof.get('shape')!={'queries':8,'units':27,'all_sequence_widths':256,'unit_microbatch':8} or proof.get('steps')!=2 or proof.get('optimizer')!='AdamW fp32' or not 0<proof.get('peak_cuda_allocated_bytes',0)<=4*1024**3):raise ValueError('exact finite original-cap synthetic resource proof required')
 if seal['file_roles']['trainer']['sha256']!=proof['script_sha256']:raise ValueError('resource script identity differs')
def main():
 require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS'))
 for name in ('HERE','PREP','H','MODELS'):
  if name in globals():require_private_root(globals()[name])
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--release',type=Path,required=True);p.add_argument('--release-sha256',required=True);p.add_argument('--resource-seal-sha256',required=True);p.add_argument('--resource-proof-sha256',required=True);p.add_argument('--resource-owned-sha256',required=True);a=p.parse_args()
 release=a.release.resolve();trainer=Path(__file__).resolve().parent/'train_finite_r3.py';proto=HERE/'protocol.json';base_path=HERE/'execution-seal-resource-r2.json';proof_path=HERE/'resource-pilot-r2.json';owned_path=HERE/'owned-resource-r2/execution-receipt.json'
 for path,pin in [(release,a.release_sha256),(trainer,TRAINER_SHA),(proto,PROTOCOL_SHA),(base_path,a.resource_seal_sha256),(proof_path,a.resource_proof_sha256),(owned_path,a.resource_owned_sha256)]:
  if sha(path)!=pin:raise ValueError('reviewed actual prerequisite changed: '+str(path))
 base=json.loads(base_path.read_bytes());proof=json.loads(proof_path.read_bytes());owned=json.loads(owned_path.read_bytes());validate_resource(base,proof,owned,a.resource_seal_sha256);review=json.loads(release.read_bytes());private_anchor=HERE.parent/'native-parent-bag-training-anchor-r2';output=private_anchor/'result'
 if (release!=HERE/'release-train-r3.json' or review.get('action')!='epoch1-native-parent-bag-training' or review.get('trainer_sha256')!=TRAINER_SHA or review.get('protocol_sha256')!=PROTOCOL_SHA or review.get('output')!=str(output) or review.get('resource_proof_sha256')!=a.resource_proof_sha256 or review.get('resource_owned_receipt_sha256')!=a.resource_owned_sha256 or review.get('token_admission_witness_sha256')!='0e67df90c5208baa18b3cbd2e4e56d699b110bb6134e468f4e725e52e885b4e0'):raise ValueError('separate primary epoch1 TRAIN release required')
 target=HERE/'execution-seal-train-r3.json';logs=HERE/'owned-train-r3';cache=HERE/'disabled-pycache-train-r3'
 if any(v.exists() for v in [target,output,logs,cache]):raise ValueError('fresh TRAIN R3 paths required')
 token_witness=json.loads((HERE/'native-token-admission-witness-r3.json').read_bytes())
 if (token_witness.get('complete') is not True or token_witness.get('unit_count')!=180 or token_witness.get('query_count')!=99 or token_witness.get('admission_source_sha256')!=sha(Path(__file__).resolve().parent/'native_token_admission_r3.py')):raise ValueError('exact complete tokenizer-only witness/source required')
 packet=json.loads(proto.read_bytes());output_admission=validate_training_output(private_anchor,output,[HERE,*[Path(path).parent for path in packet['pins']]]);old_recipe=Path(packet['retained_recipe']);recipe=json.loads(old_recipe.read_bytes())
 if recipe.get('execution_authorized') is not False or recipe['epoch_policy']['epochs_to_execute']!=1 or recipe['population']!={'all_original_rows':315,'admitted':99,'refused':216,'eligible_qasper':96,'eligible_requests':3,'hotpot':0,'empty_original_parents':20,'refill':False}:raise ValueError('original recipe/population changed')
 extras={**packet['pins'],str(trainer):TRAINER_SHA,str(proto):PROTOCOL_SHA,str(release):a.release_sha256,str(base_path):a.resource_seal_sha256,str(proof_path):a.resource_proof_sha256,str(owned_path):a.resource_owned_sha256,str(Path(__file__).resolve()):sha(__file__),str(Path(__file__).resolve().parent/'prepare_resource_r2.py'):sha(Path(__file__).resolve().parent/'prepare_resource_r2.py'),str(Path(__file__).resolve().parent/'train_output_paths.py'):sha(Path(__file__).resolve().parent/'train_output_paths.py'),str(Path(__file__).resolve().parent/'native_token_admission_r3.py'):sha(Path(__file__).resolve().parent/'native_token_admission_r3.py'),str(HERE/'native-token-admission-witness-r3.json'):'0e67df90c5208baa18b3cbd2e4e56d699b110bb6134e468f4e725e52e885b4e0',str((next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file()))/'crates/airk-retrieval/src/onnx.rs'):token_witness['native_onnx_owner_sha256']};result=copy.deepcopy(base);indexed={os.path.normcase(str(Path(f['path']).resolve())):f for f in result['files']}
 for path,pin in extras.items():
  if sha(path)!=pin:raise ValueError('input changed: '+path)
  k=os.path.normcase(str(Path(path).resolve()))
  if k in indexed and indexed[k]['sha256']!=pin:raise ValueError('conflicting inherited input')
  if k not in indexed:result['files'].append({'path':str(Path(path).resolve()),'sha256':pin,'bytes':Path(path).stat().st_size})
 if not {f['path'] for f in base['files']}<={f['path'] for f in result['files']}:raise ValueError('inherited pin lost')
 source=HERE.parent/'minilm-training-source';result['file_roles']['trainer']={'path':str(trainer),'sha256':TRAINER_SHA,'bytes':trainer.stat().st_size};result['environment']['PYTHONPYCACHEPREFIX']=str(cache);result['command']=[base['file_roles']['venv_launcher']['path'],'-B','-X','pycache_prefix='+str(cache),str(trainer),'--source',str(source),'--output',str(output),'--primary-release',str(release),'--primary-release-sha256',a.release_sha256,'--execution-seal',str(target),'--execution-seal-sha256','<SEAL_SHA256>'];result['limits']=dict(LIMITS);result['launch_log_dir']=str(logs);result['candidate']={'epoch':1,'checkpoint':'epoch-1 ONLY','rows':315,'admitted_rows':99,'refused_rows':216,'steps':13,'no_refill':True,'no_new_mining':True,'no_DEV_checkpoint_search':True,'fragment_positive_labels':False,'finite_checks_every_step':True,'release_or_promotion_qualified':False};result['prospective_training_release']={'primary_release_sha256':a.release_sha256,'resource_seal_sha256':a.resource_seal_sha256,'resource_proof_sha256':a.resource_proof_sha256,'resource_owned_receipt_sha256':a.resource_owned_sha256,'token_admission_witness_sha256':'0e67df90c5208baa18b3cbd2e4e56d699b110bb6134e468f4e725e52e885b4e0','validator_only_correction_no_training_tensor_changes':True,'old_recipe_unchanged_false':True,'output_admission':output_admission};result['environment']['AIRK_RESEARCH_MODELS']=str(require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS')));target.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps({'seal':str(target),'sha256':sha(target),'log_dir':str(logs),'output':str(output),'executed':False}))
if __name__=='__main__':main()
