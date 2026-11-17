# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,difflib
p=Path(__file__).resolve().parent;record=json.loads((p/'BASE.json').read_text(encoding='utf-8'));parts=[]
for f in sorted((p/'source').rglob('*')):
 if not f.is_file():continue
 name=f.relative_to(p/'source').as_posix();old=p/'base'/name
 before=old.read_text(encoding='utf-8').splitlines(keepends=True) if old.exists() else []
 after=f.read_text(encoding='utf-8').splitlines(keepends=True)
 if before==after:continue
 if old.exists():assert hashlib.sha256(old.read_bytes()).hexdigest()==record[name]['sha256'],name
 parts.append(''.join(difflib.unified_diff(before,after,fromfile='a/'+name if old.exists() else '/dev/null',tofile='b/'+name)))
(p/'owner-events.patch').write_text(''.join(parts),encoding='utf-8',newline='\n')
print('changed_files',len(parts))
