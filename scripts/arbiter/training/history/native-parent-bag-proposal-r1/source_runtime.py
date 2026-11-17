# SPDX-License-Identifier: Apache-2.0
"""Verified stdlib-only source-stage runtime; no model or data conversion."""
from pathlib import Path
import hashlib
import os
import time
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def verify(review,started):
    rows=review['stdlib_runtime_file_rows']+review['inputs'];seen={}
    for row in rows:
        if time.monotonic()-started>=60:raise ValueError('original source-stage60second deadline exhausted')
        if sha(row['path'])!=row['sha256']:raise ValueError('source/runtime/input drift: '+row['path'])
        seen[os.path.normcase(str(Path(row['path']).resolve()))]=row
    base=Path(review['python_executable']['path']).parent
    for root in (base/'Lib',base/'DLLs'):
        found=set();stack=[root]
        while stack:
            for entry in os.scandir(stack.pop()):
                if entry.name in ('__pycache__','site-packages'):continue
                m=entry.stat(follow_symlinks=False)
                if entry.is_symlink() or getattr(m,'st_file_attributes',0)&0x400:raise ValueError('stdlib alias/reparse refused')
                if entry.is_dir(follow_symlinks=False):stack.append(entry.path)
                elif entry.is_file(follow_symlinks=False) and Path(entry.name).suffix not in ('.pyc','.pyo'):found.add(os.path.normcase(str(Path(entry.path).resolve())))
        if found!={p for p in seen if Path(p).is_relative_to(root)}:raise ValueError('stdlib closure drift')
    for name in ('python313.zip','python313._pth','python._pth','pyvenv.cfg'):
        if (base/name).exists():raise ValueError('unreviewed Python bootstrap input')
    if (base.parent/'pyvenv.cfg').exists():raise ValueError('unreviewed parent Python bootstrap input')
    return len(seen)
