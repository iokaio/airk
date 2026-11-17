# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import subprocess,json
p=Path('artifacts/optimizations/f06-draft'); out=p/'rebased-f05'; records=json.loads((out/'PROVENANCE.json').read_text())
for name,r in records.items():
 if 'conflict_count' not in r:continue
 rel=Path(name); temp=out/'merge-inputs'/rel.parent;temp.mkdir(parents=True,exist_ok=True)
 inputs=[]
 for label,source in [('live',out/'base'/rel),('old',p/'base'/rel),('draft',p/'source'/rel)]:
  target=temp/(rel.name+'.'+label);target.write_text(source.read_text(),newline='\n');inputs.append(str(target))
 result=subprocess.run(['git','merge-file','-p','--diff3',*inputs],capture_output=True)
 (out/'source'/rel).write_bytes(result.stdout);r['conflict_count']=result.returncode
(out/'PROVENANCE.json').write_text(json.dumps(records,indent=2)+'\n')
print([(n,r.get('conflict_count')) for n,r in records.items()])
