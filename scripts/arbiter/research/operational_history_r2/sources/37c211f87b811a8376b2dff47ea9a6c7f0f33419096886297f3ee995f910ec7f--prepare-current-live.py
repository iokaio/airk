# SPDX-License-Identifier: Apache-2.0
"""Compose reviewed F07 slices onto exact current files, only in ignored copies."""
from pathlib import Path
import difflib
import hashlib
import json
import subprocess
from collections import Counter

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
original=HERE/'accounting-budget.patch'
refinement=HERE/'admission-only-after-accounting.patch'
names=sorted({line.removeprefix('+++ b/') for patch in (original,refinement)
              for line in patch.read_text(encoding='utf-8').splitlines() if line.startswith('+++ b/')})
base=HERE/'current-live-combined-base'
source=HERE/'current-live-combined-source'
captured={}
for name in names:
    live=ROOT/name
    raw=live.read_bytes()
    captured[name]=raw
    for directory in (base,source):
        output=directory/name
        output.parent.mkdir(parents=True,exist_ok=True)
        output.write_bytes(raw)
for patch in (original,refinement):
    subprocess.run(['git','-c',f'safe.directory={ROOT.as_posix()}','apply','--check','--ignore-space-change',
                    '--directory='+source.relative_to(ROOT).as_posix(),str(patch)],cwd=ROOT,check=True)
    subprocess.run(['git','-c',f'safe.directory={ROOT.as_posix()}','apply','--ignore-space-change',
                    '--directory='+source.relative_to(ROOT).as_posix(),str(patch)],cwd=ROOT,check=True)
for name,raw in captured.items():
    if (ROOT/name).read_bytes()!=raw:
        raise RuntimeError('live input changed during ignored composition: '+name)
    endings=raw.count(b'\r\n')
    if endings not in (0,raw.count(b'\n')):
        raise RuntimeError('mixed live line endings need explicit handling: '+name)
    output=source/name
    text=output.read_text(encoding='utf-8')
    output.write_text(text,encoding='utf-8',newline='\r\n' if endings else '\n')
    before=Counter(line for line in raw.decode('utf-8').splitlines() if any(ord(c)>127 for c in line))
    after=Counter(line for line in (source/name).read_text(encoding='utf-8').splitlines() if any(ord(c)>127 for c in line))
    if before-after:
        raise RuntimeError('existing non-ASCII line changed during composition: '+name)
patch=''.join(''.join(difflib.unified_diff(raw.decode('utf-8').replace('\r\n','\n').splitlines(True),
    (source/name).read_text(encoding='utf-8').splitlines(True),fromfile='a/'+name,tofile='b/'+name))
    for name,raw in captured.items())
(HERE/'current-live-combined.patch').write_text(patch,encoding='utf-8',newline='\n')
(HERE/'CURRENT-LIVE-BASE.json').write_text(json.dumps({
    'status':'ignored source composition only; uncompiled/unexecuted',
    'diff_line_endings':'normalized only for diff; exact raw live base is retained',
    'existing_non_ascii_lines':'preserved, checked for every affected file',
    'head':subprocess.check_output(['git','-c',f'safe.directory={ROOT.as_posix()}','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
    'files':{name:hashlib.sha256(raw).hexdigest() for name,raw in captured.items()},
    'input_patches':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in (original,refinement)}
},indent=2)+'\n',encoding='utf-8',newline='\n')
