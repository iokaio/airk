# SPDX-License-Identifier: Apache-2.0
import hashlib,json,math
from pathlib import Path
base=Path('artifacts/v190');candidate=json.loads((base/'p8-relevance.json').read_text());assert candidate['passed']
actual=candidate['representative'];checks=[]
for filename in ['p5-188-relevance.json','p7-relevance-r8.json']:
 prior=json.loads((base/filename).read_text());expected=prior['representative'];assert prior['passed'] and actual['complete']
 assert actual['corpus_sha256']==expected['corpus_sha256'] and actual['source_hashes']==expected['source_hashes']
 assert set(actual['scores'])==set(expected['scores'])
 for group,metrics in expected['scores'].items():
  assert set(actual['scores'][group])==set(metrics)
  for key,value in metrics.items():
   if key in ['latency_samples','p50_ms','p95_ms']:continue
   observed=actual['scores'][group][key]
   passed=observed is None if value is None else math.isclose(observed,value,rel_tol=0,abs_tol=1e-12)
   checks.append({'reference':filename,'group':group,'metric':key,'observed':observed,'reference_value':value,'passed':passed})
result={'schema':'airk.p8.quality-comparison/1','passed':all(c['passed'] for c in checks),'candidate_sha256':hashlib.sha256((base/'p8-relevance.json').read_bytes()).hexdigest(),'checks':checks}
(base/'p8-quality-comparison.json').write_text(json.dumps(result,indent=2));print(json.dumps({'passed':result['passed'],'checks':len(checks)}));raise SystemExit(0 if result['passed'] else 1)
