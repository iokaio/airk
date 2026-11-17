# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,difflib,subprocess
p=Path(__file__).resolve().parent;r=p.parents[2]
base=json.loads((p/'BASE.json').read_text(encoding='utf-8'));source={};parts=[]
base['head']=subprocess.check_output(['git','-c','safe.directory=D:/code/Github/airk','rev-parse','HEAD'],cwd=r).decode().strip()
for n,digest in base['files'].items():
 assert hashlib.sha256((r/n).read_bytes()).hexdigest()==digest,n
 a=(p/'base'/n).read_text(encoding='utf-8');b=(p/'source'/n).read_text(encoding='utf-8')
 source[n]=hashlib.sha256((p/'source'/n).read_bytes()).hexdigest()
 parts.extend(difflib.unified_diff(a.splitlines(True),b.splitlines(True),fromfile='a/'+n,tofile='b/'+n))
(p/'BASE.json').write_text(json.dumps(base,indent=2)+'\n',encoding='utf-8',newline='\n')
(p/'SOURCE.json').write_text(json.dumps({'schema':1,'kind':'strict reader retained; private selected preparation candidate only','files':source},indent=2)+'\n',encoding='utf-8',newline='\n')
(p/'selected-recent-preparation.patch').write_text(''.join(parts),encoding='utf-8',newline='\n')
print('exact raw2file bases still match')
