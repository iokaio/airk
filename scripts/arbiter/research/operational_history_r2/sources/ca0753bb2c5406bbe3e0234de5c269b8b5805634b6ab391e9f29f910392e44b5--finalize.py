# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib,re
p=Path(__file__).resolve().parent
r=p.parents[2]
base=json.loads((p/'BASE.json').read_text())
parts=[]; source={}; changed=[]
for n,item in base['files'].items():
    live=r/n
    assert (hashlib.sha256(live.read_bytes()).hexdigest()==item['sha256']) if 'sha256' in item else not live.exists(), n
    before=(p/'base'/n).read_text(encoding='utf-8') if 'sha256' in item else ''
    after=(p/'source'/n).read_text(encoding='utf-8')
    assert not re.search(r'(?m)^(<<<<<<<|=======|>>>>>>>)',after),n
    source[n]=hashlib.sha256((p/'source'/n).read_bytes()).hexdigest()
    if before!=after:
        changed.append(n)
        parts.extend(difflib.unified_diff(before.splitlines(True),after.splitlines(True),fromfile='a/'+n if 'sha256'in item else '/dev/null',tofile='b/'+n))
(p/'f10b-on-current-f11.patch').write_text(''.join(parts),encoding='utf-8',newline='\n')
(p/'SOURCE.json').write_text(json.dumps({'schema':1,'kind':'current F11 plus reviewed F10b owner/recovery; uncompiled','files':source,'changed':changed},indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({'files':len(source),'changed':len(changed),'paths':changed},indent=2))
