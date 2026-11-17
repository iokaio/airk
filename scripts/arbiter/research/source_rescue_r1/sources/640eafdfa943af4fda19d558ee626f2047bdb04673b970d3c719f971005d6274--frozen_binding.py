# SPDX-License-Identifier: Apache-2.0
import pathlib,json,hashlib,subprocess
BINDING=pathlib.Path(__file__).resolve().parent/'bindings.json'
def verify():
 d=json.loads(BINDING.read_bytes());root=pathlib.Path.cwd()
 assert root.samefile(d['candidate']),'Candidate cwd differs'
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True,encoding='utf-8').strip()==d['commit'],'Candidate HEAD drift'
 assert subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True,encoding='utf-8').strip()==d['tree'],'Candidate tree drift'
 assert subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True,encoding='utf-8')=='','Candidate tracked status changed'
 for name,want in d['files'].items():
  p=pathlib.Path(name)
  assert p.is_file() and hashlib.sha256(p.read_bytes()).hexdigest()==want,'Frozen input differs: '+name
 return {'binding_sha256':hashlib.sha256(BINDING.read_bytes()).hexdigest(),'files_verified':len(d['files']),'source_C7':d['source_C7']}
def settle(result):
 try:result['binding_post']=verify();return True
 except BaseException as error:result.update(passed=False,binding_failure={'type':type(error).__name__,'message':str(error)});return False
