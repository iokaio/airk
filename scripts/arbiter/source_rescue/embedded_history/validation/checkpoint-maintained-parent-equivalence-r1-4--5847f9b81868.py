# SPDX-License-Identifier: Apache-2.0

import hashlib,importlib.util,json,os,sys,time
from pathlib import Path
root=Path('D:/code/Github/airk');environment=Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/gpu-checkpointed-static-environment-r1.json')
loaded=set()
def audit(event,args):
 if event=='open':
  mode=args[1];flags=args[2]
  if (isinstance(mode,str) and any(x in mode for x in 'wax+')) or (isinstance(flags,int) and flags & (os.O_WRONLY|os.O_RDWR|os.O_CREAT|os.O_TRUNC|os.O_APPEND)):raise RuntimeError('read-only verification refused filesystem write')
 if event in ['os.mkdir','os.remove','os.rmdir','os.rename','os.chmod','os.chown','subprocess.Popen','os.system']:raise RuntimeError('read-only verification refused mutation/process')
 if event=='exec':
  name=getattr(args[0],'co_filename','').replace('\\','/').lower()
  if '/artifacts/' in name or '/temp/' in name:raise RuntimeError('ignored implementation execution refused: '+name)
  if name.startswith(str(root).replace('\\','/').lower()+'/scripts/'):loaded.add(getattr(args[0],'co_filename'))
sys.addaudithook(audit)
raw=environment.read_bytes();assert hashlib.sha256(raw).hexdigest()=='a348c538e349e8311841654ddce2365c229fb5972670f0b33cd0c71a3c882ef2'
retained=json.loads(raw)
def module(name):
 p=root/'scripts'/name;s=importlib.util.spec_from_file_location(name.replace('.','_'),p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
owner=module('cuda-runtime-environment.py');runtime=module('cuda-static-runtime.py')
start=time.monotonic();observed=owner.verify_handoff(environment,'a348c538e349e8311841654ddce2365c229fb5972670f0b33cd0c71a3c882ef2',root);elapsed=time.monotonic()-start
assert observed['gpu_build']==retained['build'];assert observed['gpu_aggregate']==retained['aggregate'];assert environment.read_bytes()==raw
print(json.dumps({'status':'PASS_READONLY_RETAINED_PARENT_EQUIVALENCE','elapsed_seconds':elapsed,'proof_schema':observed['gpu_build']['schema'],'canonical_proof_sha256':runtime.canonical(observed['gpu_build']),'canonical_retained_proof_sha256':runtime.canonical(retained['build']),'build_receipt':observed['gpu_build']['build_receipt'],'aggregate_output':observed['gpu_aggregate']['output'],'canonical_aggregate_sha256':runtime.canonical(observed['gpu_aggregate']),'environment_sha256':hashlib.sha256(raw).hexdigest(),'loaded_maintained_sources':[{'path':p,'sha256':hashlib.sha256(Path(p).read_bytes()).hexdigest()} for p in sorted(loaded)],'input_writes_allowed':False,'child_processes_allowed':False,'ignored_code_execution_allowed':False,'native_or_model_execution':False}))
