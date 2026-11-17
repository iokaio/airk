# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,uuid
from pathlib import Path
owner=Path(r'D:\code\Github\airk\artifacts\one-process\native-fixtures-private\.tmpNfRH9B')
install=owner/'install';control=owner/'state/control'
digest=json.loads((install/'selected.json').read_text())['active']
binary=install/'runtimes'/digest/'airk.exe'
proof={'fixture':str(owner),'retained':True,'observations':[]}
receipt=owner/'manual-cleanup-receipt.json'
def invoke(*args,input=None,check=True):
    result=subprocess.run([str(binary),*map(str,args)],input=None if input is None else json.dumps(input),text=True,capture_output=True,timeout=50,creationflags=subprocess.CREATE_NO_WINDOW)
    proof['observations'].append({'args':list(map(str,args)),'exit_code':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
    receipt.write_text(json.dumps(proof,indent=2)+'\n')
    if check:assert result.returncode==0,result.stderr
    return json.loads(result.stdout) if result.returncode==0 else None
health=invoke('service','rpc','--runtime-dir',control,'--stdin',input='health')['observation']
inspection=invoke('runtime','inspect','--install-dir',install)
assert health['runtime_sha256']==digest==inspection['selection']['active']
assert health['executable_sha256']==hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()
assert Path(inspection['executable']).samefile(binary)
epoch=invoke('service','rpc','--runtime-dir',control,'--stdin',input='request_epoch')
request={'caller':uuid.uuid4().hex,'key':{'namespace':epoch['namespace'],'epoch':epoch['epoch'],'nonce':uuid.uuid4().hex},'action':'stop','expected_instance':health['instance'],'runtime':digest,'force':False}
(owner/'manual-shutdown-request.json').write_text(json.dumps(request,indent=2)+'\n')
response=invoke('runtime','lifecycle','--install-dir',install,input=request)
expected={k:v for k,v in request.items() if k!='force'};expected['manager']=False
assert response['status']=='lifecycle' and response['record']['phase']=='complete' and response['record']['request']==expected
assert invoke('service','health','--runtime-dir',control,check=False) is None
proof['exact_stop_complete']=True
receipt.write_text(json.dumps(proof,indent=2)+'\n')
print(json.dumps({'receipt':str(receipt),'exact_stop_complete':True,'fixture_retained':True}))
