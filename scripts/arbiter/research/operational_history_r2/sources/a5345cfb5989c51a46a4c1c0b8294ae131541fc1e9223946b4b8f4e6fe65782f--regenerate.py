# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,difflib
p=Path(__file__).resolve().parent;record=json.loads((p/'BASE.json').read_text(encoding='utf-8'));parts=[]
for name,identity in sorted(record.items()):
 before=(p/'base'/name).read_text(encoding='utf-8').splitlines(keepends=True);after=(p/'source'/name).read_text(encoding='utf-8').splitlines(keepends=True)
 assert hashlib.sha256((p/'base'/name).read_bytes()).hexdigest()==identity['sha256']
 if before!=after:parts.append(''.join(difflib.unified_diff(before,after,fromfile='a/'+name,tofile='b/'+name)))
(p/'lazy-discovery.patch').write_text(''.join(parts),encoding='utf-8',newline='\n');print('changed_files',len(parts))
