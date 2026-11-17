# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,re,subprocess
p=Path('artifacts/optimizations/f10-consolidated');p.mkdir(exist_ok=True)
patch=Path('artifacts/optimizations/f10-f11-draft/foundation.patch')
names=re.findall(r'^\+\+\+ b/(.+)$',patch.read_text(encoding='utf-8'),re.M)
record={}
for name in names:
 name=name.strip();live=Path(name)
 if live.exists():
  record[name]={'sha256':hashlib.sha256(live.read_bytes()).hexdigest()}
  for layer in ['base','source']:
   dst=p/layer/name;dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(live.read_bytes())
 else:record[name]={'absent':True}
head=subprocess.check_output(['git','-c','safe.directory=D:/code/Github/airk','rev-parse','HEAD'],text=True).strip()
(p/'BASE.json').write_text(json.dumps({'head':head,'files':record,'historical_input':{'path':patch.as_posix(),'sha256':hashlib.sha256(patch.read_bytes()).hexdigest()}},indent=2)+'\n',encoding='utf-8')
print('inputs',len(names))
