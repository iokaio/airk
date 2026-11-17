# SPDX-License-Identifier: Apache-2.0
"""Tokenizer-only diagnosis; no Torch, weights, CUDA or model forward."""
from pathlib import Path
import json,hashlib,collections
from tokenizers import Tokenizer
def require_private_root(value):
    if not value:
        raise ValueError('explicit private root required')
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    private = Path(value).resolve()
    if not private.is_relative_to(root/'artifacts'):
        raise ValueError('private root must remain beneath repository artifacts; tracked or external roots refused')
    return private


ROOT=next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file());H=(Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str(ROOT/'artifacts/advanced-split/models'))).resolve())/'encoder-recovery-preparation-r1';BASE=H.parent/'native-parent-bag-candidate-r1'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS'))
require_private_root(H)
recipe=json.loads((BASE/'recipe.json').read_bytes());master=json.loads((BASE/'master.private.json').read_bytes());batches=json.loads((BASE/'epoch1-batches.private.json').read_bytes());native=json.loads((H.parent/'native-parent-bag-units-r4/units.private.json').read_bytes());path=H.parent/'minilm-training-source/tokenizer.json';assert sha(path)==recipe['tokenizer_sha256'];t=Tokenizer.from_str(path.read_text(encoding='utf-8'));t.no_padding();t.no_truncation();units={r['id']:r for r in native['units']};used=sorted({i for b in batches for i in b['unit_ids']});rows=[]
training_iteration_order=list(set(i for batch in batches for i in batch['unit_ids']))
for identifier in used:
 u=units[identifier];e=t.encode(u['input']);actual=u['native_input_tensors'];raw={'input_ids':e.ids,'attention_mask':e.attention_mask,'token_type_ids':e.type_ids};widths={name:len(value[0]) for name,value in actual.items()};raw_differences=[]
 for name,value in raw.items():
  other=actual[name][0]
  first=next((i for i,(a,b) in enumerate(zip(value,other)) if a!=b),None)
  if first is None and len(value)!=len(other):first=min(len(value),len(other))
  if value!=other:raw_differences.append({'tensor':name,'first_difference_index':first,'raw_count':len(value),'native_count':len(other),'difference_is_length_only':first==min(len(value),len(other))})
 width=widths['input_ids'];e.pad(width,pad_id=t.token_to_id('[PAD]'),pad_type_id=0,pad_token='[PAD]',direction='right');padded={'input_ids':e.ids,'attention_mask':e.attention_mask,'token_type_ids':e.type_ids};equal=all(padded[k]==actual[k][0] for k in padded)
 rows.append({'unit_id':identifier,'input_sha256':u['embedding_input_sha256'],'raw_tokens':len(raw['input_ids']),'native_widths':widths,'raw_differences':raw_differences,'whole_native_tensor_equals_tokenizers_padding':equal,'native_tensor_sha256':hashlib.sha256(json.dumps(actual,separators=(',',':')).encode()).hexdigest()})
queries=[]
for row in master['rows']:
 if row['status']=='ADMITTED':
  e=t.encode(row['original_row']['question']);queries.append({'ordinal':row['ordinal'],'token_count':len(e.ids),'cropping_required':len(e.ids)>256,'encoding_sha256':hashlib.sha256(json.dumps({'input_ids':e.ids,'attention_mask':e.attention_mask,'token_type_ids':e.type_ids},separators=(',',':')).encode()).hexdigest()})
result={'schema':'airk.private-native-padding-diagnosis/1','tokenizer_sha256':sha(path),'recipe_sha256':sha(BASE/'recipe.json'),'units_sha256':sha(H.parent/'native-parent-bag-units-r4/units.private.json'),'batches_sha256':sha(BASE/'epoch1-batches.private.json'),'unit_count':len(rows),'raw_exact_count':sum(not r['raw_differences'] for r in rows),'padded_exact_count':sum(r['whole_native_tensor_equals_tokenizers_padding'] for r in rows),'first_sorted_mismatch':next((r for r in rows if r['raw_differences']),None),'reconstructed_training_first_mismatch':next((next(r for r in rows if r['unit_id']==identifier) for identifier in training_iteration_order if next(r for r in rows if r['unit_id']==identifier)['raw_differences']),None),'PYTHONHASHSEED':__import__('os').environ.get('PYTHONHASHSEED'),'width_counts':dict(collections.Counter(r['native_widths']['input_ids'] for r in rows)),'rows':rows,'query_count':len(queries),'query_max_tokens':max(r['token_count'] for r in queries),'query_crop_count':sum(r['cropping_required'] for r in queries),'queries':queries,'native_tensors_modified':False,'modelweights_or_NN_or_CUDA':False}
(H/'native-padding-diagnosis-r3-seed1908.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps({k:v for k,v in result.items() if k not in ['rows','queries']}))
