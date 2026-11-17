# SPDX-License-Identifier: Apache-2.0
"""Final canonical Stop of restored r8; refuse any unresolved native fence."""
import hashlib,json,pathlib,subprocess,time,uuid
base=pathlib.Path(__file__).resolve().parent
recovery=json.loads((base/'preserved-adoption-r8-recovery.json').read_text())
assert recovery['restoration_complete'] is True
fixture=pathlib.Path(recovery['fixture']).resolve(strict=True);install=fixture/'install';control=fixture/'state'/'control'
active='7249f2608dc246f58f90d5ad7e03cc98aada3d7f32e5fc4ae20bef0ffe9bcd06'
old=install/'runtimes'/active/'airk.exe';new=base/'windows-debug-r8'/'runtime'/'airk.exe'
assert hashlib.sha256(old.read_bytes()).hexdigest()==recovery['old_cli_sha256']
assert hashlib.sha256(new.read_bytes()).hexdigest()=='f9cdb76fab4c5d58b9033d101645bf09a592cb830e946279b97480aba03249d8'
assert json.loads((install/'legacy-maintenance.json').read_text())['phase']=='restored'
out=base/'preserved-adoption-r8-final-stop.json';assert not out.exists()
records=[]
def run(exe,args,input=None):
    start=time.monotonic();p=subprocess.run([str(exe),*map(str,args)],input=input,capture_output=True,timeout=180,creationflags=subprocess.CREATE_NO_WINDOW)
    assert len(p.stdout)+len(p.stderr)<=2*1024*1024
    records.append({'exe':str(exe),'argv':list(map(str,args)),'exit_code':p.returncode,'elapsed_seconds':time.monotonic()-start,
                    'stdout':p.stdout.decode('utf8',errors='replace'),'stderr':p.stderr.decode('utf8',errors='replace')})
    out.write_text(json.dumps({'schema':1,'fixture':str(fixture),'records':records},indent=2)+'\n')
    assert p.returncode==0,records[-1];return json.loads(records[-1]['stdout'])
def rpc(method):return run(old,['service','rpc','--runtime-dir',control,'--stdin'],json.dumps(method).encode())
health=rpc('health')['observation']
assert health['policy']['accepting'] is True and health['runtime_sha256']==active
assert health['executable_sha256']==recovery['old_cli_sha256']
epoch=rpc('request_epoch')
assert 'namespace' in epoch and 'epoch' in epoch, 'Native epoch unavailable; no Stop submitted'
request={'caller':uuid.uuid4().hex,'key':{'namespace':epoch['namespace'],'epoch':epoch['epoch'],'nonce':uuid.uuid4().hex},
         'action':'stop','expected_instance':health['instance'],'runtime':active,'force':False}
stopped=run(new,['runtime','lifecycle','--install-dir',install],json.dumps(request).encode())
expected={key:value for key,value in request.items() if key!='force'};expected['manager']=False
assert stopped['status']=='lifecycle' and stopped['record']['phase']=='complete' and stopped['record']['request']==expected
print(json.dumps({'stopped_instance':health['instance'],'complete':True,'evidence':str(out)}))
