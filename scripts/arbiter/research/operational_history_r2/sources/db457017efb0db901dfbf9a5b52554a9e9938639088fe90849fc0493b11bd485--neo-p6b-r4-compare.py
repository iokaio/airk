# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
root=Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/source-r2/artifacts')
names=['airk','airk-embed','airk-launcher-image','libonnxruntime.dylib']
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
r={n:{'r3':sha(root/'p6b-r3-runtime/AIRK.app/Contents/MacOS'/n),'r4':sha(root/'p6b-r4-runtime/AIRK.app/Contents/MacOS'/n)} for n in names}
r['all_identical']=all(v['r3']==v['r4'] for v in r.values())
(root.parent.parent/'p6b-r4-component-comparison.json').write_text(json.dumps(r,indent=2)+'\n')
print(json.dumps(r,indent=2));assert r['all_identical']
