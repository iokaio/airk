# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
p=Path(__file__).resolve().parent;record=json.loads((p/'BASE.json').read_text(encoding='utf-8'))['files'];parts=[];manifest=[]
for f in sorted((p/'source').rglob('*')):
 if not f.is_file():continue
 name=f.relative_to(p/'source').as_posix();old=p/'base'/name
 before=old.read_text(encoding='utf-8').splitlines(keepends=True) if old.exists() else []
 after=f.read_text(encoding='utf-8').splitlines(keepends=True)
 if before==after:continue
 if old.exists():assert hashlib.sha256(old.read_bytes()).hexdigest()==record[name]['sha256'],name
 else:assert record[name].get('absent'),name
 parts.append(''.join(difflib.unified_diff(before,after,fromfile='a/'+name if old.exists() else '/dev/null',tofile='b/'+name)))
 manifest.append({'path':name,'source_sha256':hashlib.sha256(f.read_bytes()).hexdigest(),'base_sha256':record[name].get('sha256'),'new':not old.exists()})
(p/'f10-foundation.patch').write_text(''.join(parts),encoding='utf-8',newline='\n')
(p/'SOURCE.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print('changed_files',len(parts),'new_files',sum(row['new'] for row in manifest))
