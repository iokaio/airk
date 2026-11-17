# SPDX-License-Identifier: Apache-2.0
"""Approved original-image, same-operation r8 recovery, without UserStop changes."""
import hashlib,json,pathlib,subprocess,time
base=pathlib.Path(__file__).resolve().parent
case=json.loads((base/'windows-debug-pre14-adoption-result-r8.json').read_text(encoding='utf-8-sig'))['cases'][0]
fixture=pathlib.Path(case['preserved_owner']).resolve(strict=True)
install=fixture/'install';control=fixture/'state'/'control'
active='7249f2608dc246f58f90d5ad7e03cc98aada3d7f32e5fc4ae20bef0ffe9bcd06'
exe_hash='b3dc0c9f20ac17322751f525a9fb1e9074d8cd84833d0dbbcdbb7b6b7c1da103'
operation='8962dfaf2b464fa695030f5d4e34e975'
checkpoint='48d8fd9e21315a82781ccc8ec463f025'
original_instance='695eb7c5945a096d0318473b97c1fe9f'
cli=install/'runtimes'/active/'airk.exe'
def sha(path):
    with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
assert sha(cli)==exe_hash and sha(cli.parent/'airk-install.json')==active
assert json.loads((install/'selected.json').read_text())['active']==active
assert not (install/'activation.json').exists()
journal=json.loads((install/'legacy-maintenance.json').read_text())
assert journal['phase']=='restore_failed' and journal['operation']==operation
assert journal['original_explicit_user_stop'] is False and journal['original_running'] is True
assert journal['cycle_recorded_instance']==original_instance
saved=journal['checkpoint']
assert saved['state']=='ready' and saved['operation_id']==checkpoint and saved['service_instance']==original_instance
assert saved['execution_sha256']==exe_hash and saved['candidate_execution_sha256']==exe_hash
packet_path=control/f'native-checkpoint-{checkpoint}.json'
assert sha(packet_path)==saved['native_packet_sha256']
packet=json.loads(packet_path.read_text())
assert packet['operation_id']==checkpoint and packet['service_instance']==original_instance
assert packet['execution_sha256']==exe_hash and packet['candidate_execution_sha256']==exe_hash
cache=json.loads((install/'legacy-cache-retirement.json').read_text())
assert cache['inspection_sha256']==case['inspection']['inspection_sha256']
assert cache['withdrawal_operation'] is None
locators=pathlib.Path(case['native_locators_file']).resolve(strict=True)
assert json.loads(locators.read_text())==case['inspection']['origin_locators']
for origin in case['inspection']['origin_proofs']:
    assert sha(pathlib.Path(origin['config_path']))==origin['config_sha256']
out=base/'preserved-adoption-r8-recovery.json'
assert not out.exists()
record={'schema':1,'fixture':str(fixture),'operation':operation,'checkpoint':checkpoint,
        'old_cli_sha256':exe_hash,'maintenance_before':journal,'records':[]}
def save():out.write_text(json.dumps(record,indent=2)+'\n')
def run(args,input=None,timeout=90):
    started=time.monotonic()
    assert sha(cli)==exe_hash
    child=subprocess.Popen([str(cli),*map(str,args)],stdin=subprocess.PIPE if input else subprocess.DEVNULL,
                           stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
    try:stdout,stderr=child.communicate(input=input,timeout=timeout)
    except subprocess.TimeoutExpired:
        record['unknown_owned_command']={'pid':child.pid,'argv':list(map(str,args))};save();raise
    assert len(stdout)+len(stderr)<=16*1024*1024
    item={'argv':list(map(str,args)),'exit_code':child.returncode,'elapsed_seconds':time.monotonic()-started,
          'stdout':stdout.decode('utf8',errors='replace'),'stderr':stderr.decode('utf8',errors='replace')}
    record['records'].append(item);save();return item
save()
health_reply=run(['service','rpc','--runtime-dir',control,'--stdin'],json.dumps('health').encode())
assert health_reply['exit_code']==0
health=json.loads(health_reply['stdout'])['observation']
assert health['instance']==original_instance and health['runtime_sha256']==active
assert health['executable_sha256']==exe_hash and health['policy']['accepting'] is False
# The actual-selected console adapter authenticates the same image/version and
# sends Shutdown pinned to its observed native instance. No UserStop is written.
shutdown=run(['service','rpc','--runtime-dir',control,'--stdin'],json.dumps('shutdown').encode())
assert shutdown['exit_code']==0 and json.loads(shutdown['stdout'])['status']=='stopping'
resumed=run(['runtime','adopt-legacy-indexes','--install-dir',install,'--expected-active',active,
             '--operation-id',operation,'--locators-file',locators,
             '--expected-cache-inspection-sha256',case['inspection']['inspection_sha256']],timeout=420)
after=json.loads((install/'legacy-maintenance.json').read_text())
record['maintenance_after']=after
record['adoption_success']=resumed['exit_code']==0
record['restoration_complete']=after['operation']==operation and after['phase']=='restored'
assert after['original_explicit_user_stop'] is False
save()
print(json.dumps({'adoption_exit_code':resumed['exit_code'],'restoration_complete':record['restoration_complete'],
                  'phase':after['phase'],'evidence':str(out)}))
