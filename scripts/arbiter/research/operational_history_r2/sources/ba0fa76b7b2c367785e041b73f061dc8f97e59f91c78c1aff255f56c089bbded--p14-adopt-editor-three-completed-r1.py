# SPDX-License-Identifier: Apache-2.0
"""Read-only verification and create-new adoption of exactly three completed scenarios."""
import hashlib,json
from pathlib import Path
B=Path(__file__).resolve().parent
W=B/'p10-worktree'
R=W/'artifacts/editor-qualification/native-installed-cUDfG8'
OUT=B/'p14-editor-three-completed-adoption-r1.json'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
read=lambda p:json.loads(Path(p).read_text(encoding='utf-8'))
def main():
 assert not OUT.exists()
 owner=read(B/'p14-package-editor-continuation-r3.json')
 assert owner['status']=='failed' and owner['failure']=="AssertionError('five-private-scenarios')"
 step=owner['steps'][0];receipt=read(step['receipt'])
 assert sha(step['receipt'])==step['receipt_sha256'] and receipt['actual_exit_code']==receipt['exit_code']==1
 assert receipt['source_before']==receipt['source_after']==owner['native_source']
 log=B/'p14-package-continuation-five-private-scenarios-r3.log'
 assert sha(log)==receipt['log_sha256']=='027412339711d7c5e0f214b4fb23e323c8be2c44eb0fd783a8fa27f96f1b48a3'
 summary=read(R/'summary.json')
 assert 'status' not in summary and 'qualifiesFullScenarioSelection' not in summary
 assert summary['selectedScenarios']==['same','different','multi','usage','untrusted']
 assert [(x['scenario'],x['role']) for x in summary['scenarios']]==[('same','a'),('same','b'),('different','a'),('different','b'),('multi','a')]
 assert summary['nativeInstallDigest']==owner['native_install_sha256']
 assert Path(summary['source']).resolve()==Path(owner['vsix']['path']).resolve()
 assert sha(summary['source'])==owner['vsix']['sha256']=='49170b51765048e438dfd9516a35e4884d7667efe5ad2652d02303caf22f4c90'
 assert all(sha(W/p)==digest for p,digest in summary['harness'].items())
 evidence={str(p):sha(p) for p in [R/'summary.json',log,Path(step['receipt']),B/'p14-package-editor-continuation-r3.json']}
 rows=[]
 for name in ['same','different','multi']:
  folder=R/name;roles=['a','b'] if name!='multi' else ['a']
  cleanup=read(folder/'native-cleanup.json');stop=read(folder/'native-stop.json');capture=stop['capture']
  request=capture['response']['record']['request'];health=capture['before_health']['observation'];inspection=cleanup['observation']
  assert cleanup['qualified'] is True and cleanup['stop']==stop
  assert capture['qualification']=='historical_native_lifecycle_response' and capture['response']['status']=='lifecycle'
  assert capture['response']['record']['phase']=='complete' and request['action']=='stop' and capture['force'] is False
  assert request['expected_instance']==health['instance'] and request['runtime']==summary['nativeInstallDigest']
  assert health['build']['source_sha256']==owner['native_source']['sha256'] and health['build']['source_files']==960
  assert health['executable_sha256']=='b6b4ed967a13888ed238cf9111da4158821df5acff6993b37334ea0d5ef987d8'
  assert request['key']['namespace']==capture['namespace']==inspection['registry_namespace']
  assert capture['capturedAt']>=stop['started'] and stop['dispatch']['dispatch']!=stop['confirmation']['dispatch']
  assert capture['runtime']['installDir']==str(folder/'native-service')
  assert inspection['lifecycle']['explicitly_stopped'] is True and inspection['lifecycle']['pending'] is None and inspection['payload_verified'] is True
  assert inspection['approval']['manifest_sha256']==summary['nativeInstallDigest']
  assert read(folder/'capture-complete.json')=={}
  sources=read(folder/'source-preservation.json')
  for source in sources:
   assert sha(folder/source['path'])==source['after_sha256']
   assert source['intentional_peer_edit']==(name in ['same','different'] and source['path']=='workspace-a/fixture.ts')
   if not source['intentional_peer_edit']:assert source['before_sha256']==source['after_sha256']
  role_rows=[]
  for role in roles:
   result=read(folder/f'{role}-result.json');retained=read(folder/f'{role}-retained-evidence.json')
   expected=next(x for x in summary['scenarios'] if x['scenario']==name and x['role']==role)
   assert {**result,'sourcePreservation':sources}==expected
   assert result['installed'] is True and result['version']=='1.8.8' and result['scenario']==name and result['role']==role
   assert result['native'][:len(retained['native'])]==retained['native']
   assert all(result[k]==v for k,v in retained.items() if k!='native')
   assert read(folder/f'{role}-capture.json')=={} and len(result['checks'])>=8
   transports=[v for row in result['native'] for v in row.get('transport',[])]
   assert len(transports)>=10 and result['definitions']
   dispatch=read(folder/f'{role}-ui-dispatch-receipts.json');assert dispatch and all('completed' in x and x['completed']>=x['started'] for x in dispatch)
   editor=read(folder/f'{role}-editor.log.cleanup.json');assert editor['code']==0 and editor['forced'] is False
   mcp=list(folder.glob(role+'-mcp-*-cleanup.json'));assert mcp
   for p in mcp:
    v=read(p);assert v['closed'] is True and v['forced'] is False and v['result']['code']==0
   role_rows.append({'role':role,'extension_host_pid':result['extensionHostPid'],'checks':result['checks'],'raw_transport_count':len(transports),'dispatch_count':len(dispatch)})
  assert len({r['extension_host_pid'] for r in role_rows})==len(roles)
  if name=='multi':assert (folder/'explorer-checked.json').exists()
  for p in folder.iterdir():
   if p.is_file() and (p.suffix in ['.json','.png','.txt'] or p.name.endswith('.log')):evidence[str(p)]=sha(p)
  rows.append({'scenario':name,'status':'passed','roles':role_rows,'native_stop_complete':True,'native_cleanup_qualified':True})
 assert read(R/'usage/a-ui-request.json')['action']=='usageAdmin' and (R/'usage/a-error.json').exists()
 assert not (R/'untrusted').exists()
 output={'status':'passed','scope':'Exactly three completed scenarios adopted from failed five-scenario queue; no execution repeated.',
 'reviewer':'independent evidence_review agent; original harness author not this reviewer','native_source':owner['native_source'],
 'vsix':owner['vsix'],'harness':summary['harness'],'scenario_results':rows,'evidence_sha256':evidence,
 'original_queue_status':'failed','usage_status':'failed','untrusted_status':'not reached','legacy_status':'not reached',
 'limits':'No whole-five acceptance, publication, product rebuild, or post-recovery installer claim.'}
 with OUT.open('x',encoding='utf-8') as f:json.dump(output,f,indent=2);f.write('\n')
 print(json.dumps({'status':'passed','adopted':['same','different','multi'],'receipt':str(OUT),'sha256':sha(OUT)}))
if __name__=='__main__':main()
