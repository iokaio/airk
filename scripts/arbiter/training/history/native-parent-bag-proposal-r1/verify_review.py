# SPDX-License-Identifier: Apache-2.0
"""Fresh read-only byte/stdlib-census seal check; never materializes TRAIN sources."""
from pathlib import Path
import hashlib
import json
import os
import time
HERE=Path(__file__).resolve().parent
review_path=HERE/'preparation-review-r2.json';review_raw=review_path.read_bytes()
if hashlib.sha256(review_raw).hexdigest()!='25c1631f381f3a4509026332a52ed9629e5fb4d51b1e382fedf45685a59a10e9':raise ValueError('review packet changed')
review=json.loads(review_raw);started=time.monotonic()
def digest(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
rows=review['stdlib_runtime_file_rows']+review['inputs'];seen={}
for row in rows:
    if time.monotonic()-started>60:raise ValueError('read-only seal verification60second budget exhausted')
    if digest(row['path'])!=row['sha256']:raise ValueError('source/runtime/input byte drift: '+row['path'])
    seen[os.path.normcase(str(Path(row['path']).resolve()))]=row
base=Path(review['python_executable']['path']).parent
def census(root):
    found=set();stack=[root]
    while stack:
        path=stack.pop()
        for entry in os.scandir(path):
            if entry.name in ('__pycache__','site-packages'):continue
            metadata=entry.stat(follow_symlinks=False)
            if entry.is_symlink() or getattr(metadata,'st_file_attributes',0)&0x400:raise ValueError('stdlib runtime alias/reparse refused')
            if entry.is_dir(follow_symlinks=False):stack.append(Path(entry.path))
            elif entry.is_file(follow_symlinks=False) and Path(entry.name).suffix not in ('.pyc','.pyo'):found.add(os.path.normcase(str(Path(entry.path).resolve())))
    return found
for root in (base/'Lib',base/'DLLs'):
    expected={path for path in seen if Path(path).is_relative_to(root)}
    if census(root)!=expected:raise ValueError('stdlib runtime closure changed')
absent=[base/'python313.zip',base/'python313._pth',base/'python._pth',base/'pyvenv.cfg',base.parent/'pyvenv.cfg']
if any(p.exists() for p in absent):raise ValueError('unreviewed Python bootstrap config/zip present')
result={'schema':'airk.private-TRAIN-prep-input-verification/1','review_sha256':hashlib.sha256(review_raw).hexdigest(),'fresh_verified_files':len(seen),'fresh_verified_bytes':sum(r['bytes'] for r in seen.values()),'stdlib_closure_exact':True,'bootstrap_absences':[str(p) for p in absent],'elapsed_seconds':time.monotonic()-started,'actual_source_materialization':False,'native_or_model_execution':False}
output=HERE/'fresh-input-verification.json'
if output.exists():raise ValueError('fresh metadata proof required')
output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
