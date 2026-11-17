# SPDX-License-Identifier: Apache-2.0
"""Final receipt-bound Stop/uninstall of the recovered fictional r12 installation."""
import hashlib,json,pathlib,subprocess,time,uuid
base=pathlib.Path(__file__).resolve().parent
recovery=json.loads((base/'preserved-schema1-r12-recovery.json').read_text())
assert recovery['exit_code']==0 and recovery['activation_pending'] is False
fixture=pathlib.Path(recovery['fixture']).resolve(strict=True); install=fixture/'install';control=fixture/'state'/'control'
cli=base/'windows-debug-r8'/'runtime'/'airk.exe'
assert hashlib.sha256(cli.read_bytes()).hexdigest()==recovery['new_cli_sha256']
out=base/'preserved-schema1-r12-final-cleanup-r3.json'
assert not out.exists()
records=[]
def invoke(args,input=None,metadata=False):
    start=time.monotonic()
    actual_cli=payload if metadata else cli
    p=subprocess.run([str(actual_cli),*map(str,args)],input=input,capture_output=True,timeout=180,creationflags=subprocess.CREATE_NO_WINDOW)
    assert len(p.stdout)+len(p.stderr)<=2*1024*1024
    raw={'executable':str(actual_cli),'executable_sha256':hashlib.sha256(actual_cli.read_bytes()).hexdigest(),'argv':list(map(str,args)),'exit_code':p.returncode,'elapsed_seconds':time.monotonic()-start,
         'stdout':p.stdout.decode('utf8',errors='replace'),'stderr':p.stderr.decode('utf8',errors='replace')}
    records.append(raw);out.write_text(json.dumps({'schema':1,'fixture':str(fixture),'records':records},indent=2)+'\n')
    assert p.returncode==0,raw
    return json.loads(raw['stdout'])
def rpc(method):return invoke(['service','rpc','--runtime-dir',control,'--stdin'],json.dumps(method).encode(),metadata=True)
assert not (install/'activation.json').exists()
assert json.loads((install/'selected.json').read_text())['active']==recovery['accepted_candidate']
payload=install/'runtimes'/recovery['accepted_candidate']/'airk.exe'
assert hashlib.sha256(payload.read_bytes()).hexdigest()=='b3dc0c9f20ac17322751f525a9fb1e9074d8cd84833d0dbbcdbb7b6b7c1da103'
health=rpc('health')['observation']
assert health['runtime_sha256']==recovery['accepted_candidate'] and health['policy']['accepting'] is True
payload=install/'runtimes'/recovery['accepted_candidate']/'airk.exe'
assert health['executable_sha256']==hashlib.sha256(payload.read_bytes()).hexdigest()
epoch=rpc('request_epoch')
request={'caller':uuid.uuid4().hex,'key':{'namespace':epoch['namespace'],'epoch':epoch['epoch'],'nonce':uuid.uuid4().hex},
         'action':'stop','expected_instance':health['instance'],'runtime':health['runtime_sha256'],'force':False}
result=invoke(['runtime','lifecycle','--install-dir',install],json.dumps(request).encode())
expected={key:value for key,value in request.items() if key!='force'};expected['manager']=False
assert result['status']=='lifecycle' and result['record']['phase']=='complete' and result['record']['request']==expected
uninstalled=invoke(['runtime','uninstall','--install-dir',install])
assert uninstalled['uninstalled'] is True
print(json.dumps({'fixture':str(fixture),'stopped_instance':health['instance'],'uninstalled':True,'evidence':str(out)}))
