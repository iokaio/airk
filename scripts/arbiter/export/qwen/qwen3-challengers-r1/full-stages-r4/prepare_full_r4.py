# SPDX-License-Identifier: Apache-2.0
"""Additive full-reference/scores-only seal preparation; no process or NN launch."""
import argparse,copy,hashlib,json,os,sys
from pathlib import Path
from launch_full_r4 import LIMITS
from full_worker_r4 import PROFILE_PINS
HERE=Path(__file__).resolve().parent
PARENT=HERE.parent
ASSETS=PARENT.parent/'qwen3-challenger-assets-r1'
sys.path.insert(0,str(PARENT))
from prepare_qwen3_tiny_r3 import natural
TINY={'seal':('execution-seal-qwen3-tiny-r3.json','e748b1d5953615218354e7d3f1f3c2923dd0589d8ad159ba556760f40c65716c'),'owned':('owned-qwen3-tiny-r3/execution-receipt.json','0e94ea3f064ef9e89857fd1a5a8f2c2ce167056322d52e59073f0f9324b51a5b'),'outer':('owned-qwen3-tiny-r3/outer-closure.json','1f304e7b4edf617b9f5f04c753455abe6297f4add2183baa50e402008f1695e6'),'proof':('tiny-result-r3/receipt.json','654eb0a31dd69dad032e52e43fc85231a449636c7387aaed309011e550d0de71')}
SOURCES=['owned_profile_17.py','owned_profile_4.py','full_worker_r4.py','launch_full_r4.py','full_reference_r4.py','export_scores_r4.py','prepare_full_r4.py','test_full_r4.py']
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def source_pins():return {str(p.resolve()):sha(p) for p in [*[HERE/n for n in SOURCES],PARENT/'qwen3_reference_r2.py']}
def metadata_pins():return {str(p.resolve()):sha(p) for p in [HERE/'owner-profile-diffs-r4.json',PARENT/'selected-pipeline-design-r2.json',PARENT/'qwen-compiler-source-policy-proposal-r1.json',ASSETS/'research-asset-pins.json']}
def asset_rows(model):
 manifest=ASSETS/'research-asset-pins.json'
 if sha(manifest)!='4cac4fc751d996e987bb4701fb90ce8cbd600af06152151b4ea99bed8d6c664c':raise ValueError('original publisher manifest changed')
 entry=next(x for x in json.loads(manifest.read_bytes())['models'] if x['id']==model)
 folder=ASSETS/'candidate-assets'/model.replace('/','--');rows=[]
 for pin in entry['assets']:
  path=folder/pin['name']
  if path.is_symlink() or not path.is_file() or getattr(path.lstat(),'st_file_attributes',0)&0x400 or path.stat().st_size!=pin['bytes']:raise ValueError('asset alias/type/length differs')
  digest=pin['sha256'] or sha(path)
  if not pin['sha256'] and hashlib.sha1(('blob '+str(pin['bytes'])+'\0').encode()+path.read_bytes()).hexdigest()!=pin['git_blob']:raise ValueError('publisher metadata Git blob differs')
  rows.append({'path':str(path.resolve()),'sha256':digest,'bytes':pin['bytes']})
 return rows

def append_rows(base,rows):
 result=copy.deepcopy(base);indexed={os.path.normcase(str(Path(x['path']).resolve())):x for x in base['files']}
 if len(indexed)!=len(base['files']):raise ValueError('duplicate inherited pin')
 for row in rows:
  key=os.path.normcase(row['path'])
  if key in indexed and indexed[key]!=row:raise ValueError('inherited input identity conflict')
  if key not in indexed:result['files'].append(row);indexed[key]=row
 if result['files'][:len(base['files'])]!=base['files']:raise ValueError('inherited pin removed/changed')
 return result

def admit_prerequisite(review):
 raw=review['prerequisite'];loaded={}
 for name in ['seal','owned','outer','proof']:
  item=raw[name];path=Path(item['path']).resolve()
  if sha(path)!=item['sha256']:raise ValueError('actual prerequisite bytes changed')
  loaded[name]=json.loads(path.read_bytes())
 base=loaded['seal'];owned=loaded['owned'];proof=loaded['proof'];outer=loaded['outer'];basepin=raw['seal']['sha256']
 if not natural(owned) or owned.get('seal_sha256')!=basepin or owned.get('command')!=[x.replace('<SEAL_SHA256>',basepin) for x in base['command']]:raise ValueError('matching actual natural owned command required')
 if outer.get('seal_sha256')!=basepin or outer.get('owned_receipt_sha256')!=raw['owned']['sha256'] or outer.get('pilot_receipt_sha256')!=raw['proof']['sha256']:raise ValueError('actual outer receipt chain differs')
 model=review['model_id'];stage=review['authorized_stage']
 if model=='Qwen/Qwen3-1.7B' and stage=='qwen3-full-reference':
  if len(base['files'])!=49294 or any(raw[k]!={'path':str((PARENT/n).resolve()),'sha256':pin} for k,(n,pin) in TINY.items()):raise ValueError('exact successful tiny closure required')
  if proof.get('stage')!='qwen3-tiny-export':raise ValueError('actual tiny math proof required')
 else:
  expected_model='Qwen/Qwen3-1.7B' if stage=='qwen3-full-reference' else model
  predecessor=base.get('qwen_full_stage',{})
  if predecessor.get('model_id')!=expected_model or proof.get('model_id')!=expected_model or proof.get('stage')!='qwen3-full-reference' or proof.get('complete') is not True:raise ValueError('exact prior selected full reference required')
  if proof.get('release_sha256')!=predecessor.get('release_sha256'):raise ValueError('prior model proof release differs')
  release=json.loads(Path(predecessor['release_path']).read_bytes())
  if str(Path(raw['proof']['path']).resolve())!=str(Path(release['fresh_output'])/'receipt.json') or sha(predecessor['release_path'])!=predecessor['release_sha256']:raise ValueError('prior command owning result/release differs')
  if [(x['tokens'],x['options']) for x in proof['cases']]!=[(17,3),(128,8),(2048,8)] or any(max(x['official_hidden_max_abs_error'],x['official_scores_max_abs_error'])>1e-3 for x in proof['cases']):raise ValueError('actual all-shape fixed-tolerance proof required')
 return base

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--release',type=Path,required=True);p.add_argument('--release-sha256',required=True);a=p.parse_args()
 if sha(a.release)!=a.release_sha256:raise ValueError('Root release bytes changed')
 review=json.loads(a.release.read_bytes());model=review['model_id'];stage=review['authorized_stage']
 if model not in LIMITS or stage not in ['qwen3-full-reference','qwen3-scores-only-export'] or review.get('execution_authorized') is not True or review.get('source_sha256')!=source_pins() or review.get('metadata_sha256')!=metadata_pins() or review.get('limits')!=LIMITS[model] or review.get('synthetic_shapes')!=[[17,3],[128,8],[2048,8]]:raise ValueError('exact separately reviewed full-stage release required')
 label=('17' if model.endswith('1.7B') else '4')+('-reference-r4' if stage=='qwen3-full-reference' else '-export-r4')
 output=HERE/(label+'-result');target=HERE/('execution-seal-'+label+'.json');logs=HERE/('owned-'+label);cache=HERE/('disabled-pycache-'+label)
 if a.release.resolve()!=HERE/('release-'+label+'.json') or review['fresh_output']!=str(output) or any(p.exists() for p in [output,target,logs,cache]):raise ValueError('fresh exact model-stage paths required')
 base=admit_prerequisite(review)
 extras=[a.release,*[HERE/n for n in SOURCES],HERE/'owner-profile-diffs-r4.json',*[Path(x['path']) for x in review['prerequisite'].values()]]
 proof_path=Path(review['prerequisite']['proof']['path']);prior=json.loads(proof_path.read_bytes())
 for name,pin in prior.get('artifact_sha256',{}).items():
  if Path(name).name!=name or sha(proof_path.parent/name)!=pin:raise ValueError('prior raw numeric artifact differs')
  extras.append(proof_path.parent/name)
 if stage=='qwen3-scores-only-export' and (review.get('reference_receipt_path')!=str(proof_path) or review.get('reference_receipt_sha256')!=review['prerequisite']['proof']['sha256']):raise ValueError('exact actual own-model reference arrays must bind export')
 rows=[{'path':str(p.resolve()),'sha256':sha(p),'bytes':p.stat().st_size} for p in extras]+asset_rows(model)
 result=append_rows(base,rows);owner=HERE/('owned_profile_17.py' if model.endswith('1.7B') else 'owned_profile_4.py')
 if sha(owner)!=PROFILE_PINS[model]:raise ValueError('resource profile source differs')
 result['file_roles']['execution_module']={'path':str(owner),'sha256':sha(owner),'bytes':owner.stat().st_size};worker=HERE/'full_worker_r4.py';result['file_roles']['trainer']={'path':str(worker),'sha256':sha(worker),'bytes':worker.stat().st_size}
 result['status']='ROOT_RELEASED_QWEN_FULL_SYNTHETIC_REFERENCE_OR_SCORES_EXPORT_ONLY';result['environment']['PYTHONPYCACHEPREFIX']=str(cache);result['limits']=dict(LIMITS[model]);result['launch_log_dir']=str(logs)
 result['command']=[base['file_roles']['venv_launcher']['path'],'-B','-X','pycache_prefix='+str(cache),str(worker),'--seal',str(target),'--seal-sha256','<SEAL_SHA256>']
 result['qwen_full_stage']={'model_id':model,'release_path':str(a.release.resolve()),'release_sha256':a.release_sha256,'pilot_path':str(HERE/('full_reference_r4.py' if stage=='qwen3-full-reference' else 'export_scores_r4.py')),'assets_root':str(ASSETS),'inherited_seal_sha256':review['prerequisite']['seal']['sha256'],'actual_natural_receipt_sha256':review['prerequisite']['owned']['sha256'],'metadata_sha256':review['metadata_sha256'],'all_inherited_pins_preserved':True,'reference_hidden_diagnostic_only':stage=='qwen3-full-reference','scores_only_export_native_ABI_shape_not_quality_qualification':stage=='qwen3-scores-only-export'}
 result['candidate']={'stage':stage,'model_id':model,'pretrained_weights':True,'synthetic_only':True,'task_text':False,'training':False,'release_or_promotion_qualified':False}
 target.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
 print(json.dumps({'seal':str(target),'sha256':sha(target),'inherited_files':len(base['files']),'added_files':len(result['files'])-len(base['files']),'root_release_sha256':a.release_sha256,'launch_command':[base['file_roles']['venv_launcher']['path'],'-B',str(HERE/'launch_full_r4.py'),'--seal',str(target),'--seal-sha256',sha(target),'--log-dir',str(logs)],'NN_executed':False}))
if __name__=='__main__':main()
