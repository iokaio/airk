# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
p=Path(__file__).resolve().parent
r=json.loads((p/'BASE.json').read_text(encoding='utf-8'))
parts=[];rows=[]
for n,identity in sorted(r['files'].items()):
 q=p/'source'/n;old=p/'base'/n
 if old.exists():
  assert hashlib.sha256(old.read_bytes()).hexdigest()==identity.get('effective_sha256',identity.get('sha256')),n
  a=old.read_text(encoding='utf-8').splitlines(True)
 else:
  assert identity.get('absent'),n
  a=[]
 b=q.read_text(encoding='utf-8').splitlines(True)
 if a!=b:
  parts.append(''.join(difflib.unified_diff(a,b,fromfile='a/'+n if old.exists() else '/dev/null',tofile='b/'+n)))
  rows.append({'path':n,'sha256':hashlib.sha256(q.read_bytes()).hexdigest()})
(p/'f10b-after-f11-f12.patch').write_text(''.join(parts),encoding='utf-8',newline='\n')
(p/'SOURCE.json').write_text(json.dumps(rows,indent=2)+'\n',encoding='utf-8')
print('changed_files',len(rows))
