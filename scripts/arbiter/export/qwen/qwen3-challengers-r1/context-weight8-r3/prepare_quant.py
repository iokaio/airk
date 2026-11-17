# SPDX-License-Identifier: Apache-2.0
"""Additive derivative from actual same-model FP32 export; no NN/process launch."""
import argparse,copy,hashlib,json,os,sys
from pathlib import Path
HERE=Path(__file__).resolve().parent
R4=HERE.parent/'full-stages-r4'
sys.path.insert(0,str(R4))
from prepare_full_r4 import natural,append_rows
from launch_full_r4 import LIMITS
SOURCES=['quantize_context.py','quant_worker.py','prepare_quant.py','test_quant.py','graph_closure.py','projection_census.py','focus-graph-closure-source.sha256']
EXPECTED={'Qwen/Qwen3-1.7B':{'proof':'ab7eb82a84050a5ceaad8f8fc6c7d2efbcd158288a0022bbbc482b92973fb759','owned':'93b7b9f1abf823ccc6d4c02a430f1fe3273bb240713994e4e5f6f0d81f94ebf7'},'Qwen/Qwen3-4B':{'proof':'64f3b825a7c831bd263a6d3475d21cc41013fbb7a5a05b00ab663abd074bcb19','owned':'d3560c382e6846fb849fdc1b05c9edb58ef719eb2275c437e604425ddbf7a93b'}}
NUMERIC={'raw_max_absolute_error':0.25,'normalized_mass_max_error':0.05,'synthetic_argmax_must_match':True,'all_live_mask_control':True,'fp32_original_tolerance_unchanged':0.001,'all_live_against_original_trained_rows':True,'projection_recipe':'MatMul-only 7/layer, embedding/gather-head FP32','single_thread_head_and_ORT':True,'full_graph_ranges_hash_before_after':True,'native_external_storage_normalized_exactly':True}
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
NORMALIZER=Path(__file__).resolve().parents[6]/'scripts/arbiter/export/onnx_storage/storage_normalizer.py'
def source_pins():return {str(path.resolve()):sha(path) for path in [*[HERE/n for n in SOURCES],NORMALIZER]}
def normalizer_pin():return {'path':str(NORMALIZER),'sha256':sha(NORMALIZER)}
def head_pins(model):
 label='17' if model=='Qwen/Qwen3-1.7B' else '4'
 reference=R4/(label+'-reference-r4-result/receipt.json')
 expected={'17':'c5f5b210f8f494e223d0abfc33983b13f8df0f91e3fd234d3dce8a47f00058c2','4':'9e662f231ad689c617c96eecd337cc60b08a6f6616034842d9604a4aaa93f034'}[label]
 if sha(reference)!=expected:raise ValueError('actual original hidden producer changed')
 assets=HERE.parent.parent/'qwen3-challenger-assets-r1'
 manifest=assets/'research-asset-pins.json'
 if sha(manifest)!='4cac4fc751d996e987bb4701fb90ce8cbd600af06152151b4ea99bed8d6c664c':raise ValueError('original publisher manifest changed')
 model_folder=assets/'candidate-assets'/model.replace('/','--');index=model_folder/'model.safetensors.index.json'
 shard_name=json.loads(index.read_bytes())['weight_map']['model.embed_tokens.weight']
 entry=next(x for x in json.loads(manifest.read_bytes())['models'] if x['id']==model)
 shard_pin=next(x['sha256'] for x in entry['assets'] if x['name']==shard_name)
 return {'reference_receipt':{'path':str(reference),'sha256':expected},'embedding_index':{'path':str(index),'sha256':sha(index)},'embedding_shard':{'path':str(model_folder/shard_name),'sha256':shard_pin}}
def main():
 p=argparse.ArgumentParser();p.add_argument('--release',type=Path,required=True);p.add_argument('--release-sha256',required=True);a=p.parse_args()
 if sha(a.release)!=a.release_sha256:raise ValueError('Root quant release changed')
 review=json.loads(a.release.read_bytes());model=review['model_id'];label='17' if model=='Qwen/Qwen3-1.7B' else '4'
 if model not in EXPECTED or review.get('execution_authorized') is not True or review.get('authorized_stage')!='qwen3-context-weight8' or review.get('source_sha256')!=source_pins() or review.get('limits')!=LIMITS[model] or review.get('numeric_limits')!=NUMERIC or review.get('original_head_readout')!=head_pins(model) or review.get('storage_normalizer')!=normalizer_pin():raise ValueError('exact reviewed separate quantization protocol required')
 source=R4/(label+'-export-r4-result');base_path=R4/('execution-seal-'+label+'-export-r4.json');owned_path=R4/('owned-'+label+'-export-r4/execution-receipt.json');outer_path=R4/('owned-'+label+'-export-r4/outer-closure.json');proof_path=source/'receipt.json'
 if review.get('source_export')!=str(source) or review.get('source_export_receipt_sha256')!=EXPECTED[model]['proof'] or sha(proof_path)!=EXPECTED[model]['proof'] or sha(owned_path)!=EXPECTED[model]['owned']:raise ValueError('exact actual own-model FP32 producer required')
 basepin=sha(base_path);base=json.loads(base_path.read_bytes());owned=json.loads(owned_path.read_bytes());outer=json.loads(outer_path.read_bytes());proof=json.loads(proof_path.read_bytes())
 if not natural(owned) or owned.get('seal_sha256')!=basepin or owned.get('command')!=[x.replace('<SEAL_SHA256>',basepin) for x in base['command']] or outer.get('seal_sha256')!=basepin or outer.get('owned_receipt_sha256')!=sha(owned_path) or outer.get('pilot_receipt_sha256')!=sha(proof_path):raise ValueError('actual source owner/outer command closure differs')
 if proof.get('complete') is not True or proof.get('model_id')!=model or proof.get('stage')!='qwen3-scores-only-export' or [(x['tokens'],x['options']) for x in proof['rows']]!=[(17,3),(128,8),(2048,8)] or any(x['maximum_absolute_error']>0.001 for x in proof['rows']):raise ValueError('actual original strict FP32 numerical proof required')
 target=HERE/('execution-seal-'+label+'-weight8.json');logs=HERE/('owned-'+label+'-weight8');output=HERE/(label+'-weight8-result');cache=HERE/('disabled-pycache-'+label+'-weight8')
 if a.release.resolve()!=HERE/('release-'+label+'-weight8.json') or review['fresh_output']!=str(output) or any(p.exists() for p in [target,logs,output,cache]):raise ValueError('fresh exact model-specific weight8 paths required')
 extras=[a.release,base_path,owned_path,outer_path,proof_path,*[HERE/n for n in SOURCES],NORMALIZER]
 inherited={os.path.normcase(row['path']):row['sha256'] for row in base['files']}
 for item in head_pins(model).values():
  if inherited.get(os.path.normcase(str(Path(item['path']).resolve())))!=item['sha256']:raise ValueError('independent original head/hidden pin absent from actual predecessor')
 rows=[{'path':str(path.resolve()),'sha256':sha(path),'bytes':path.stat().st_size} for path in extras]
 for name,pin in proof['artifact_sha256'].items():
  path=source/name
  if Path(name).name!=name or path.is_symlink() or not path.is_file():raise ValueError('invalid produced model/array artifact')
  rows.append({'path':str(path.resolve()),'sha256':pin,'bytes':path.stat().st_size})
 result=append_rows(base,rows);worker=HERE/'quant_worker.py';result['file_roles']['trainer']={'path':str(worker),'sha256':sha(worker),'bytes':worker.stat().st_size};result['environment']['PYTHONPYCACHEPREFIX']=str(cache);result['environment']['OMP_NUM_THREADS']='1';result['environment']['MKL_NUM_THREADS']='1';result['launch_log_dir']=str(logs)
 result['command']=[base['file_roles']['venv_launcher']['path'],'-B','-X','pycache_prefix='+str(cache),str(worker),'--seal',str(target),'--seal-sha256','<SEAL_SHA256>'];result['status']='ROOT_RELEASED_SELECTED_CONTEXT_WEIGHT8_NUMERIC_ONLY'
 result['qwen_full_stage']={'model_id':model,'release_path':str(a.release.resolve()),'release_sha256':a.release_sha256,'source_fp32_seal_sha256':basepin,'source_fp32_owned_sha256':sha(owned_path),'source_fp32_proof_sha256':sha(proof_path),'source_fp32_outer_sha256':sha(outer_path),'numeric_limits':NUMERIC,'all_inherited_runtime_and_source_pins_preserved':True}
 result['candidate']={'stage':'selected Context weight8 numeric artifact','no_training':True,'no_task_text':True,'qualified':False,'fp32_original_tolerance_unchanged':True}
 target.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps({'seal':str(target),'sha256':sha(target),'inherited_files':len(base['files']),'appended_files':len(result['files'])-len(base['files']),'launch_command':[base['file_roles']['venv_launcher']['path'],'-B',str(R4/'launch_full_r4.py'),'--seal',str(target),'--seal-sha256',sha(target),'--log-dir',str(logs)],'NN_executed':False}))
if __name__=='__main__':main()
