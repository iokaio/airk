# SPDX-License-Identifier: Apache-2.0
"""Compose latest P11a proposals additively in ignored copies; no Git/compiler."""
from pathlib import Path
import hashlib,json,re,difflib
b=Path(__file__).resolve().parent;w=b/'p10-worktree';out=b/'p11a-composed-current-proposed';out.mkdir(exist_ok=True)
patches=['p11a-corrected-report-review.patch','p11a-transport-report-review.patch','p11a-ui-docs-report-review.patch']
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
originals={};current={};resolutions=[];applied=[]
def parse(name):
 lines=(b/name).read_bytes().decode('utf-8',errors='surrogateescape').replace('\r\n','\n').splitlines(keepends=True);i=0;files=[]
 while i<len(lines):
  if not lines[i].startswith('--- '):i+=1;continue
  old=lines[i][4:].strip();new=lines[i+1][4:].strip();assert new.startswith('b/');path=new[2:];i+=2;hunks=[]
  while i<len(lines) and not lines[i].startswith('--- '):
   if not lines[i].startswith('@@ '):i+=1;continue
   match=re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@',lines[i]);assert match,lines[i];i+=1;body=[]
   while i<len(lines) and lines[i][:1] in [' ','+','-'] and not lines[i].startswith('--- '):body.append(lines[i]);i+=1
   oldcount=sum(line[0]!='+' for line in body);newcount=sum(line[0]!='-' for line in body)
   assert oldcount==int(match[2] or 1) and newcount==int(match[4] or 1),(path,oldcount,newcount,match.groups())
   hunks.append((int(match[1]),body))
  files.append((path,old=='/dev/null',hunks))
 return files
for patch in patches:
 for path,new,hunks in parse(patch):
  if path not in originals:
   src=w/path;raw=src.read_bytes() if src.exists() else None;originals[path]=raw;current[path]=(raw.decode('utf-8',errors='surrogateescape').replace('\r\n','\n').splitlines(keepends=True) if raw is not None else [])
  lines=current[path];delta=0
  for oldstart,body in hunks:
   before=[line[1:] for line in body if line[0]!='+'];after=[line[1:] for line in body if line[0]!='-'];expected=max(oldstart-1+delta,0)
   positions=[offset for offset in range(len(lines)-len(before)+1) if lines[offset:offset+len(before)]==before]
   if before and len(positions)==1:
    pos=positions[0];lines[pos:pos+len(before)]=after;delta+=len(after)-len(before);applied.append({'patch':patch,'path':path,'mode':'exact_context','line':pos+1});continue
   if not before and new and not lines:
    lines[:]=after;applied.append({'patch':patch,'path':path,'mode':'new_file'});continue
   if path=='crates/airk-cli/tests/combined_reads.rs' and not any(line.startswith('-') for line in body):
    added=[line[1:] for line in body if line.startswith('+')]
    if ''.join(added).strip()=='#[path = "combined_reads/profile_families.rs"]\nmod profile_families;':
     assert not any('mod profile_families;' in line for line in lines)
     lines.extend(added);resolutions.append({'patch':patch,'path':path,'resolution':'Append only new top-level profile_families test module at current EOF; retain all newer normalization helper/tests instead of matching repeated old EOF context.'});continue
   # Only additive introductory documentation context conflicts are resolved:
   # insert the new P11 paragraph after the unchanged title; retain ALL P10 text.
   if path in ['README.md','packages/vscode/README.md','docs/README.md','docs/releases/release-1.8.8.md'] and not any(line.startswith('-') for line in body) and oldstart==1:
    added=[line[1:] for line in body if line.startswith('+')];assert added and 'P11a' in ''.join(added) and lines[0]==before[0]
    lines[1:1]=added;resolutions.append({'patch':patch,'path':path,'resolution':'Add only P11a introduction after original title; preserve current qualified P10 paragraphs and all remaining current bytes.'});continue
   context=''.join(before[:18]);raise RuntimeError(f'Unresolved hunk {patch}:{path}:{oldstart}; matches={len(positions)}; expected={expected}; context={context!r}')
  current[path]=lines
# Fresh current base bytes are retained even when their old proposal base drifted.
# Preserve the full current P10 ledger unchanged; it is outside these patches.
ledger='docs/research/implementation-v190.md';ledger_before=(w/ledger).read_bytes()
for path,lines in current.items():
 raw=originals[path];newline='\r\n' if raw is not None and b'\r\n'in raw else '\n';text=''.join(lines);dest=out/path;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(text.replace('\n',newline).encode('utf-8',errors='surrogateescape'))
assert (w/ledger).read_bytes()==ledger_before
manifest={'status':'INERT additive composition, not applied/compiled/tested','inputs':[{'patch':p,'sha256':sha(b/p)} for p in patches],'files':[{'path':path,'base_sha256':hashlib.sha256(raw).hexdigest() if raw is not None else None,'proposal_sha256':sha(out/path)} for path,raw in originals.items()],'context_resolutions':resolutions,'applied_hunks':len(applied),'preserved_ledger_sha256':hashlib.sha256(ledger_before).hexdigest(),'original_utf8_invalid_files':[]}
for path,raw in originals.items():
 if raw is not None:
  try:raw.decode('utf-8')
  except UnicodeDecodeError as error:manifest['original_utf8_invalid_files'].append({'path':path,'offset':error.start,'byte':hex(raw[error.start]),'preserved':'surrogateescape; unrelated original bytes unchanged'})
parts=[]
for path,raw in originals.items():
 a=raw.decode('utf-8',errors='surrogateescape').replace('\r\n','\n') if raw is not None else '';z=(out/path).read_bytes().decode('utf-8',errors='surrogateescape').replace('\r\n','\n');parts.append(''.join(difflib.unified_diff(a.splitlines(True),z.splitlines(True),fromfile='a/'+path if raw is not None else '/dev/null',tofile='b/'+path)))
p=b/'p11a-composed-current.proposed.patch';p.write_bytes(''.join(parts).encode('utf-8',errors='surrogateescape'));manifest['combined_patch_sha256']=sha(p)
# Recheck live bases after composition; no concurrent changes can be hidden.
for path,raw in originals.items():assert ((w/path).read_bytes() if (w/path).exists() else None)==raw,path
(b/'p11a-composed-current-base.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'files':len(current),'hunks':len(applied),'doc_context_resolutions':len(resolutions),'patch_sha256':sha(p),'live_bases_unchanged':True},indent=2))
