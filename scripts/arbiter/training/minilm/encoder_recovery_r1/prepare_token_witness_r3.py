# SPDX-License-Identifier: Apache-2.0
"""Frozen tokenizer-only all-used-unit/admitted-query witness, no model or GPU."""
from pathlib import Path
import hashlib,json,os
from tokenizers import Tokenizer
from native_token_admission_r3 import native_unit_admission
def require_private_root(value):
    if not value:
        raise ValueError('explicit private root required')
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    private = Path(value).resolve()
    if not private.is_relative_to(root/'artifacts'):
        raise ValueError('private root must remain beneath repository artifacts; tracked or external roots refused')
    return private


H=Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str((next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file()))/'artifacts/advanced-split/models'))).resolve()/'encoder-recovery-preparation-r1';ROOT=next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file());BASE=H.parent/'native-parent-bag-candidate-r1';sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS'))
 for name in ('HERE','PREP','H','MODELS'):
  if name in globals():require_private_root(globals()[name])
 recipe=json.loads((BASE/'recipe.json').read_bytes());master_path=BASE/'master.private.json';batches_path=BASE/'epoch1-batches.private.json';native_path=H.parent/'native-parent-bag-units-r4/units.private.json';tokenizer_path=H.parent/'minilm-training-source/tokenizer.json';paths={master_path:recipe['master_sha256'],batches_path:recipe['epoch1_batches_sha256'],native_path:recipe['native_units_sha256'],tokenizer_path:recipe['tokenizer_sha256']}
 for p,pin in paths.items():
  if sha(p)!=pin:raise ValueError('frozen original input differs')
 master=json.loads(master_path.read_bytes());batches=json.loads(batches_path.read_bytes());units={u['id']:u for u in json.loads(native_path.read_bytes())['units']};t=Tokenizer.from_file(str(tokenizer_path));t.no_padding();t.no_truncation();w=[]
 for identifier in sorted(set(i for batch in batches for i in batch['unit_ids'])):
  unit=units[identifier];w.append({'unit_id':identifier,**native_unit_admission(t,unit['input'],unit['native_input_tensors'],unit['embedding_admission'])})
 queries=[]
 for row in master['rows']:
  if row['status']=='ADMITTED':
   e=t.encode(row['original_row']['question'])
   if not 2<=len(e.ids)<=256:raise ValueError('query would crop')
   values={'input_ids':e.ids,'attention_mask':e.attention_mask,'token_type_ids':e.type_ids};queries.append({'ordinal':row['ordinal'],'token_count':len(e.ids),'encoding_sha256':hashlib.sha256(json.dumps(values,separators=(',',':')).encode()).hexdigest()})
 assert len(w)==180 and len(queries)==99
 result={'schema':'airk.private-native-token-admission-witness/1','complete':True,'source_pins':{str(p):pin for p,pin in paths.items()},'admission_source_sha256':sha(Path(__file__).resolve().parent/'native_token_admission_r3.py'),'native_onnx_owner_sha256':sha(ROOT/'crates/airk-retrieval/src/onnx.rs'),'native_owner_semantics':'onnx.rs encode_input+input_tensors use actualtokenizer postprocess once then padID/zero mask/type to owning shape_length. Consumer compares WHOLE emitted arrays to a fresh Tokenizers Encoding.pad result; never trims/crops/rewrites actualtensors.','unit_count':180,'query_count':99,'query_max_tokens':max(q['token_count'] for q in queries),'all_whole_native_tensors_match':True,'unit_witnesses':w,'query_witnesses':queries,'query_native_emission_claimed':False,'native_query_note':'Queries retain original unpadded training tokenizer representation and existing minibatch padding; exact native query runtime tensor emission was not executed or inferred.','native_tensors_changed':False,'training_math_or_rows_or_masks_changed':False,'Torch_or_weights_or_CUDA_or_NN':False}
 out=H/'native-token-admission-witness-r3.json'
 if out.exists():raise ValueError('retain original witness, no overwrite')
 out.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps({'witness_sha256':sha(out),'unit_count':180,'query_count':99,'native_tensors_changed':False,'NN':False}))
if __name__=='__main__':main()
