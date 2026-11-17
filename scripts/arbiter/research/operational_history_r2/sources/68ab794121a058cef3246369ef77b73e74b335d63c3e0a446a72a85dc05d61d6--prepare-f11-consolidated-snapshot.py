# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import re,json,hashlib
p=Path('artifacts/optimizations/f11-consolidated');p.mkdir(exist_ok=True)
f10=Path('artifacts/optimizations/f10-consolidated');historical=Path('artifacts/optimizations/f10-f11-draft')
patches=[historical/'audit-admission-after-foundation.patch',historical/'verify-after-foundation.patch']
names=set()
for patch in patches:names.update(name.strip() for name in re.findall(r'^\+\+\+ b/(.+)$',patch.read_text(encoding='utf-8'),re.M))
record={}
for name in sorted(names):
 effective=f10/'source'/name;live=Path(name);src=effective if effective.exists() else live
 if src.exists():
  record[name]={'sha256':hashlib.sha256(src.read_bytes()).hexdigest(),'layer':'f10-consolidated/source' if effective.exists() else 'live'}
  for layer in ['base','source']:
   dst=p/layer/name;dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(src.read_bytes())
 else:record[name]={'absent':True}
(p/'BASE.json').write_text(json.dumps({'dependency':{'f10_patch':hashlib.sha256((f10/'f10-foundation.patch').read_bytes()).hexdigest(),'f10_base':hashlib.sha256((f10/'BASE.json').read_bytes()).hexdigest(),'f10_sources':hashlib.sha256((f10/'SOURCE.json').read_bytes()).hexdigest()},'files':record,'historical_inputs':{x.name:hashlib.sha256(x.read_bytes()).hexdigest() for x in patches}},indent=2)+'\n',encoding='utf-8')
print('inputs',len(names))
