# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import subprocess, hashlib, json, shutil
root=Path.cwd(); draft=root/'artifacts/optimizations/f06-draft'; out=draft/'rebased-f05'
(out/'base').mkdir(parents=True,exist_ok=True); (out/'source').mkdir(exist_ok=True)
records={}
for proposed in (draft/'source').rglob('*'):
 if not proposed.is_file(): continue
 rel=proposed.relative_to(draft/'source'); live=root/rel; old=draft/'base'/rel
 target=out/'source'/rel; target.parent.mkdir(parents=True,exist_ok=True)
 if live.exists():
  baseline=out/'base'/rel; baseline.parent.mkdir(parents=True,exist_ok=True); shutil.copyfile(live,baseline)
  records[rel.as_posix()]={'sha256':hashlib.sha256(live.read_bytes()).hexdigest()}
  if old.exists():
   # Current side is exact live F05, other side is the latest corrected draft.
   p=subprocess.run(['git','merge-file','-p','--diff3',str(live),str(old),str(proposed)],capture_output=True)
   if p.returncode<0 or p.returncode>127: raise RuntimeError(p.stderr.decode())
   target.write_bytes(p.stdout); records[rel.as_posix()]['conflict_count']=p.returncode
  else:
   shutil.copyfile(proposed,target); records[rel.as_posix()]['new_draft_with_existing_live']=True
 else:
  shutil.copyfile(proposed,target); records[rel.as_posix()]={'new':True}
(out/'PROVENANCE.json').write_text(json.dumps(records,indent=2)+'\n')
print(json.dumps(records,indent=2))
