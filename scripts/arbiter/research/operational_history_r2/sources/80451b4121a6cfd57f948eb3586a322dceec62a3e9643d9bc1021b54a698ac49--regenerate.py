# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,difflib
p=Path(__file__).resolve().parent;r=json.loads((p/'BASE.json').read_text(encoding='utf-8'));parts=[];output=[]
for name,identity in sorted(r['files'].items()):
 source=p/'source'/name
 if not source.exists():raise SystemExit('Missing draft source '+name)
 old=p/'base'/name
 before=old.read_text(encoding='utf-8').splitlines(keepends=True) if old.exists() else []
 after=source.read_text(encoding='utf-8').splitlines(keepends=True)
 if old.exists():assert hashlib.sha256(old.read_bytes()).hexdigest()==identity['sha256'],name
 else:assert identity.get('absent'),name
 if before!=after:
  parts.append(''.join(difflib.unified_diff(before,after,fromfile='a/'+name if old.exists() else '/dev/null',tofile='b/'+name)))
  output.append({'path':name,'sha256':hashlib.sha256(source.read_bytes()).hexdigest()})
(p/'f11-after-f10.patch').write_text(''.join(parts),encoding='utf-8',newline='\n');(p/'SOURCE.json').write_text(json.dumps(output,indent=2)+'\n',encoding='utf-8')
print('changed_files',len(parts))
