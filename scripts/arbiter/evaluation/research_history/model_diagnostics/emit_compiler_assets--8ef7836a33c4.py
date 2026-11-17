# SPDX-License-Identifier: Apache-2.0
import hashlib,json,sys
from pathlib import Path
from transformers import AutoTokenizer
root=Path(__file__).parent
folder_by_candidate={'intern08':'internlm--Intern-Decision-0.8B','intern4':'internlm--Intern-Decision-4B','strands2':'StrandsAgents--strands-decider-2B-qwen3.5-v1-2610','kev4':'jaredpalmer--kev-4b'}
destination=root/'compiler-assets-r1';destination.mkdir(exist_ok=False)
records=[]
for candidate,folder in folder_by_candidate.items():
 path=root/'candidate-assets'/folder;tokenizer=AutoTokenizer.from_pretrained(path,local_files_only=True,trust_remote_code=False)
 controls=['<decision>','<|im_start|>','<|im_end|>','<think>','</think>'] if candidate.startswith('intern') else ['<|fim_prefix|>','<|fim_middle|>','<|box_start|>','<|box_end|>','<|fim_suffix|>'] if candidate=='kev4' else []
 def bind(text):
  ids=tokenizer(text,add_special_tokens=False).input_ids
  if len(ids)!=1:raise ValueError('not one token: '+text)
  return {'text':text,'id':ids[0]}
 profile={'schema':'airk.arbiter.compiler/1','recipe':'intern_single_choice_v1' if candidate.startswith('intern') else 'strands_single_choice_v1' if candidate=='strands2' else 'kev_single_choice_v1',
  'tokenizer':hashlib.sha256((path/'tokenizer.json').read_bytes()).hexdigest(),'max_tokens':2048,'max_options':8,
  'controls':[bind(text) for text in controls],'symbols':[bind(text) for text in 'ABCDEFGH'] if candidate.startswith('intern') else []}
 data=(json.dumps(profile,indent=2)+'\n').encode();output=destination/(candidate+'-compiler.json');output.write_bytes(data)
 records.append({'candidate':candidate,'file':output.name,'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data),'profile':profile})
(destination/'inventory.json').write_text(json.dumps(records,indent=2)+'\n');print(json.dumps(records,indent=2))
