# SPDX-License-Identifier: Apache-2.0
"""Read-only DEV shown-unit/token/target metadata; no new grade, NN or final access."""
from pathlib import Path
import hashlib
import json
import os
import time
os.environ['RAYON_NUM_THREADS']='1'
from tokenizers import Tokenizer
ROOT=Path(__file__).resolve().parents[4]
MODELS=ROOT/'artifacts/advanced-split/models'
started=time.monotonic()
paths=[Path(__file__),MODELS/'candidate-native-r20-r1/visible-prefix-mining-r1-u8/capture.json',MODELS/'candidate-native-r20-r1/grading/grading.private.json',MODELS/'a1-component-dev-native-fixture-r1/rubric.private.json',MODELS/'visible-prefix-mining-exports-r1/u8/tokenizer.json']
raw={p:p.read_bytes() for p in paths};sha=lambda b:hashlib.sha256(b).hexdigest()
tokenizer=Tokenizer.from_str(raw[paths[-1]].decode());tokenizer.no_padding();tokenizer.no_truncation()
capture=json.loads(raw[paths[1]]);graded=json.loads(raw[paths[2]])['families'][0];rubric={t['id']:t for t in json.loads(raw[paths[3]])['tasks']}
shown=[]
for record in capture['records']:
    for hit in record['primary_vector']['hits']:
        encoding=tokenizer.encode(hit['text'],add_special_tokens=False)
        shown.append({'raw_tokens':len(encoding.ids),'bytes':len(hit['text'].encode()),'hit_truncated':hit['truncated']})
target_counts={'covered_by_shown_exact_span':0,'same_file_shown_different_span':0,'target_file_not_shown':0}
for row in graded['rows']:
    if row['quality_status']!='ALIGNED':continue
    task=rubric[row['id']];spans=row['source_spans']
    for alternative in task['alternatives']:
        for fact in alternative['facts']:
            for location in fact['locations']:
                candidates=[s for s in spans if s['file']==location['file']]
                if any(s['byte_start']<=location['byte_start'] and s['byte_end']>=location['byte_end'] for s in candidates):kind='covered_by_shown_exact_span'
                elif candidates:kind='same_file_shown_different_span'
                else:kind='target_file_not_shown'
                target_counts[kind]+=1
result={'schema':'airk.private-DEV-shown-unit-visibility-diagnosis/1','no_NN_or_new_grade':True,'source_inputs':{str(p.relative_to(ROOT)):sha(b) for p,b in raw.items()},'shown_hits':len(shown),'max_body_raw_tokens':max(x['raw_tokens'] for x in shown),'body_over254_tokens':sum(x['raw_tokens']>254 for x in shown),'max_body_bytes':max(x['bytes'] for x in shown),'hit_truncated_count':sum(x['hit_truncated'] for x in shown),'target_location_metadata_not_quality_denominator':target_counts,'scope':'Only native returned body visibility and already-graded original DEV location metadata; same-file misses are not evidence of crop and receive no whole-file credit. Full indexed context-token inputs are not persisted in these captures; source-chain proof and native planner tests own that invariant.','elapsed_seconds':time.monotonic()-started}
if any(p.read_bytes()!=b for p,b in raw.items()):raise ValueError('diagnostic source/input changed')
output=Path(__file__).with_suffix('.json')
if output.exists():raise ValueError('fresh metadata diagnostic required')
output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
