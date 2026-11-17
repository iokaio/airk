# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,math
base=Path('artifacts/v190')
raw=base/'p6d-source-fairness-claim.json'
p=json.loads(raw.read_text())
frozen=base/'p6d-fairness-gates.json'
baseline=json.loads(frozen.read_text())
assert p['status']=='passed' and p['source_priority_assertions'] is True
assert p['runtime_native_source']=={'sha256':'a87a1bc8377699c7ae6da76f2aa7dbb23fe3386816bf5edb87af0546b2d6cbf7','files':789}
assert p['source_bytes_preserved'] is True and p['cleanup_failures']==[]
assert len(p['waves'])==len(baseline['gates'])==2
rows=[]
for index,(wave,prior) in enumerate(zip(p['waves'],baseline['gates'])):
 assert wave['manual_watch_disabled'] is bool(index)
 assert wave['status']=='passed'
 assert prior['bound_seconds']==10
 value=wave['source_overhead_seconds']
 assert isinstance(value,(int,float)) and math.isfinite(value) and value>=0
 rows.append({'gate':prior['gate'],'observed_seconds':value,'bound_seconds':10,'passed':value<=10})
result={'passed':all(x['passed'] for x in rows),'limits_changed':False,'qualification':'Derived frozen-bound evaluation after run; raw selected_max_overhead_seconds was null and is not rewritten.','runtime_native_source':p['runtime_native_source'],'runtime_binary_sha256':p['runtime_binary_sha256'],'raw_receipt':str(raw),'raw_sha256':hashlib.sha256(raw.read_bytes()).hexdigest(),'frozen_bound_receipt':str(frozen),'frozen_bound_receipt_sha256':hashlib.sha256(frozen.read_bytes()).hexdigest(),'gates':rows}
(base/'p6d-claim-source-gates.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
raise SystemExit(0 if result['passed'] else 1)
