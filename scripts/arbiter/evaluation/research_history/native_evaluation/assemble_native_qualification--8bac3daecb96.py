# SPDX-License-Identifier: Apache-2.0
"""Populate the Rust-owned Core Profile DATA for explicit raw-only qualification."""
import hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
scratch=Path(__file__).parent
catalog=json.loads((scratch/'research-asset-pins.json').read_bytes())
by_folder={model['id'].replace('/','--'):model for model in catalog['models']}
sha=lambda data:hashlib.sha256(data).hexdigest()
def source_bytes(folder,name):
 data=(scratch/'candidate-assets'/folder/name).read_bytes();asset=next(a for a in by_folder[folder]['assets'] if a['name']==name)
 assert len(data)==asset['bytes']
 assert sha(data)==asset['sha256'] if asset['sha256'] else hashlib.sha1(('blob '+str(len(data))+'\0').encode()+data).hexdigest()==asset['git_blob']
 return data
def create_or_verify(path,data):
 if path.exists():assert path.read_bytes()==data
 else:
  with path.open('xb') as stream:stream.write(data)
 return {'name':path.name,'bytes':len(data),'sha256':sha(data)}
output=scratch/'native-qualification-profiles-r1';output.mkdir(exist_ok=False)
policy=(ROOT/'tools/retrieval-train/a0-freeze.json').read_bytes()
calibration=(json.dumps({'schema':'airk.development.raw-score-calibration-status/1','status':'UNFITTED','scope':'Raw compiler/scoring parity only. No temperature, calibrated probability or task-qualification claim; release qualification remains null.'},indent=2)+'\n').encode()
entries=[]
for candidate,choice,folder,base,license_names in [
 ('intern08','arbiter-small-intern-08b','internlm--Intern-Decision-0.8B',None,['LICENSE','LICENSE-QWEN']),
 ('strands2','arbiter-small-strands-2b','StrandsAgents--strands-decider-2B-qwen3.5-v1-2610','Qwen--Qwen3.5-2B-Base',['LICENSE.md']),
 ('intern4','arbiter-large-intern-4b','internlm--Intern-Decision-4B',None,['LICENSE','LICENSE-QWEN']),
 ('kev4','arbiter-large-kev-4b','jaredpalmer--kev-4b','Qwen--Qwen3.5-4B-Base',[])]:
 assets_root=scratch/(candidate+'-consolidated-r1');source_receipt=(assets_root/'receipt.json').read_bytes();source=json.loads(source_receipt)
 fixtures_path=scratch/(candidate+'-compiler-r3')/'fixtures.json';fixtures=fixtures_path.read_bytes();fixture_value=json.loads(fixtures)
 assert len(fixture_value['cases'])==3
 assert '<think>' in fixture_value['cases'][2]['state']
 compiler=(scratch/'compiler-assets-r1'/(candidate+'-compiler.json')).read_bytes()
 tokenizer=source_bytes(folder,'tokenizer.json')
 assert json.loads(compiler)['tokenizer']==sha(tokenizer)
 notice=('DEVELOPMENT RAW-SCORE QUALIFICATION ONLY\nNo release/profile/task/resource/quality qualification. Text-only full-prefill FP32, all required original trained text layers/readout preserved. Original vision branches are not executed. Compiler escapes model controls; no claim semantic evidence instructions are neutralized.\nModel '+by_folder[folder]['id']+' @ '+by_folder[folder]['revision']+'\n').encode()
 for name in license_names:notice+=b'\n'+source_bytes(folder,name)
 if base:notice+=('\nBase '+by_folder[base]['id']+' @ '+by_folder[base]['revision']+'\n').encode()+source_bytes(base,'LICENSE')
 if candidate=='strands2':notice+=b'\nReconstructed exact profile: publisher inferred base revision and unknown training-code commit; independently evaluate pinned base plus entire published adapter/head. No inherited upstream evaluation claims.\n'
 if candidate=='kev4':notice+=b'\nAdapter/head publisher declares Apache-2.0 in immutable model metadata/card; no standalone model LICENSE. Actual pinned checkpoint loaded with restricted weights_only and all original head tensors retained; upstream temperature/quality not inherited.\n'
 inventory=[{**a,'role':'model' if a['name']=='model.onnx' else 'external_weights'} for a in source['assets']]
 for name,role,data in [('tokenizer.json','tokenizer',tokenizer),('compiler.json','compiler',compiler),('calibration-unfitted.json','calibration',calibration),('NOTICE.txt','notice',notice),('compiler-fixtures-r3.json','fixture',fixtures),('policy-raw-v1.json','fixture',policy)]:
  inventory.append({**create_or_verify(assets_root/name,data),'role':role})
 names=['input_ids','attention_mask','position_ids','decision_positions','candidate_positions','option_mask']
 inputs=[{'name':name,'element':'bool' if name=='option_mask' else 'int64','axes':['batch'] if name=='decision_positions' else ['batch','candidates'] if name in ('candidate_positions','option_mask') else ['batch','tokens']} for name in names]
 weight_bytes=sum(a['bytes'] for a in inventory if a['role']=='external_weights')
 profile={'model':choice,'profile_id':choice+'-fp32-raw-parity-unqualified-v1','runtime':'onnxruntime-cpu/1.30.0',
  'compiler':sha(compiler),'calibration':sha(calibration),'policy':sha(policy),'qualification':None,'tasks':['airk.evidence_relevance.v1'],
  'abi':{'readout':'decision_symbol' if candidate.startswith('intern') else 'pointer_head','inputs':inputs,'output':{'name':'scores','element':'float32','axes':['batch','candidates']}},
  'max_tokens':2048,'max_candidates':8,'memory_reservation_bytes':weight_bytes*4+(1<<30),'peak_disk_bytes':sum(a['bytes'] for a in inventory)*3,'assets':inventory}
 origin=(json.dumps({'scope':'Explicit test-support raw parity ONLY; requested task/resources are experimental, not qualified; conservative memory estimate is not a measured floor pass','source_model':by_folder[folder]['id'],'source_revision':by_folder[folder]['revision'],
  'base_model':by_folder[base]['id'] if base else None,'base_revision':by_folder[base]['revision'] if base else None,
  'consolidation_receipt_sha256':sha(source_receipt),'profile_without_manifest':profile},sort_keys=True,separators=(',',':'))+'\n').encode()
 origin_asset=create_or_verify(assets_root/'profile-origin-raw-v1.json',origin)
 profile['manifest']=sha(origin);profile['assets'].append({**origin_asset,'role':'fixture'})
 path=output/(candidate+'-profile.json');path.write_text(json.dumps(profile,indent=2)+'\n')
 entries.append({'profile':str(path.resolve()),'assets':str(assets_root.resolve()),'fixtures':str((assets_root/'compiler-fixtures-r3.json').resolve())})
(output/'inputs.json').write_text(json.dumps(entries,indent=2)+'\n')
(output/'scope.txt').write_text('Rust-owned Core Profile field shape, populated as development DATA only. Every qualification is null. Unfitted calibration is explicitly not a probability claim. Product release catalog never consumes these raw descriptors. Full model/compiler/native proof only; all human quality/resource/platform gates remain separate.\n')
print(json.dumps({'input':str((output/'inputs.json').resolve()),'models':len(entries),'source_sha256':sha(Path(__file__).read_bytes())}))
