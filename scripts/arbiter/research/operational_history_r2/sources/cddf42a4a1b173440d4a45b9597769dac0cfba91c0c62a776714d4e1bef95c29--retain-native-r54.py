# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,shutil
r=Path.cwd();logs=r/'artifacts/advanced-split/receipts/resume-native-r54';out=r/'artifacts/advanced-split/native-harnesses/context-integrated-r54';assert not out.exists();out.mkdir()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def copy(p,name):
 q=out/name;shutil.copyfile(p,q);assert sha(q)==sha(p);return {'path':str(q),'sha256':sha(q)}
rows=[]
for line in (logs/'capture-kernel-image.log').read_text().splitlines():
 try:x=json.loads(line)
 except ValueError:continue
 if x.get('reason')=='compiler-artifact':rows.append(x)
k=[x for x in rows if x.get('target',{}).get('name')=='airk_kernel' and x.get('profile',{}).get('test') and x.get('executable')];assert len(k)==1
v=[x for x in rows if x.get('target',{}).get('name')=='airk_retrieval' and 'test-support' in x.get('features',[]) and not x.get('profile',{}).get('test')];assert len(v)==1
binary=Path(k[0]['executable']);lock={'binary':copy(binary,'airk-kernel.exe'),'kernel_depinfo':copy(binary.with_suffix('.d'),'airk-kernel.d'),'artifacts':[]}
for name in v[0]['filenames']:
 p=Path(name)
 if p.suffix in ['.rlib','.rmeta']:lock['artifacts'].append(copy(p,p.name))
first=Path(v[0]['filenames'][0]);stem=first.stem.removeprefix('lib');dep=first.parent/(stem+'.d');lock['retrieval_depinfo']=copy(dep,'airk-retrieval.d')
lock['artifacts'].extend([copy(logs/'capture-kernel-image.log','compile.log'),copy(logs/'capture-kernel-image.json','compile.json')])
(out/'artifact-lock.json').write_text(json.dumps(lock,indent=2)+'\n');print(json.dumps({'retention':str(out),'binary':lock['binary'],'artifact_lock_sha256':sha(out/'artifact-lock.json'),'compile_receipt_sha256':sha(out/'compile.json')}))

