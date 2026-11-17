# SPDX-License-Identifier: Apache-2.0
"""Read-only collector; requires both actual new whole scenarios completed successfully."""
import argparse,hashlib,json
from pathlib import Path
B=Path(__file__).resolve().parent;W=B/'p10-worktree';RUNS=W/'artifacts/editor-qualification'
CURRENT={'sha256':'237f896933ad59a4075f9395fd53fcb92e376946faf1fc01036c139fc0533c0f','files':960}
OLD={'sha256':'5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11','files':960}
OLD_VSIX='49170b51765048e438dfd9516a35e4884d7667efe5ad2652d02303caf22f4c90'
NEW_VSIX='3482475a658c74760f9dc3627cc493d5b57043e73a44fcae50eb0f7c18598d71'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
read=lambda p:json.loads(Path(p).read_text(encoding='utf-8'))
def evidence(paths):return [{'path':str(Path(p).resolve()),'sha256':sha(p)} for p in sorted(set(paths),key=str)]
def role(folder,name,summary):
 result=read(folder/(name+'-result.json'));row=next(r for r in summary['scenarios'] if r['scenario']==folder.name and r['role']==name)
 sources=read(folder/'source-preservation.json');assert {**result,'sourcePreservation':sources}==row
 assert result['installed'] is True and result['version']=='1.8.8' and result['scenario']==folder.name and result['role']==name
 assert read(folder/(name+'-capture.json'))=={} and read(folder/'capture-complete.json')=={}
 editor=read(folder/(name+'-editor.log.cleanup.json'));assert editor['code']==0 and editor['forced'] is False
 assert (folder/(name+'-workbench.png')).read_bytes().startswith(b'\x89PNG\r\n\x1a\n')
 dispatch=read(folder/(name+'-ui-dispatch-receipts.json'));assert dispatch and all('completed' in r and r['completed']>=r['started'] for r in dispatch)
 for item in sources:
  assert sha(folder/item['path'])==item['after_sha256']
  if not item['intentional_peer_edit']:assert item['before_sha256']==item['after_sha256']
 return result

def native_stop(folder,source,manifest,main):
 stop=read(folder/'native-stop.json');cleanup=read(folder/'native-cleanup.json');assert cleanup['qualified'] is True and cleanup['stop']==stop
 capture=stop['capture'];response=capture['response'];record=response['record'];request=record['request'];before=capture['before_health'];health=before['observation'];after=cleanup['observation']
 assert capture['qualification']=='historical_native_lifecycle_response' and response['status']=='lifecycle' and record['phase']=='complete'
 assert request['action']=='stop' and capture['force'] is False and request['expected_instance']==health['instance'] and before['status']=='health'
 assert health['build']['source_sha256']==source['sha256'] and health['build']['source_files']==source['files']
 assert health['runtime_sha256']==request['runtime']==manifest and health['executable_sha256']==main
 previous=stop.get('previous_capture') or {}
 assert capture['capturedAt']>=stop['started'] and record['id'] and record['id']!=previous.get('response',{}).get('record',{}).get('id')
 assert capture['runtime']['runtimeSha256']==manifest and stop['requested_binding']['runtimeSha256']==manifest
 assert Path(capture['runtime']['controlDir']).resolve()==Path(after['control_dir']).resolve()
 assert Path(capture['runtime']['installDir']).resolve()==Path(stop['requested_binding']['installDir']).resolve()
 assert stop['dispatch']['dispatch']!=stop['confirmation']['dispatch']
 assert capture['namespace']==request['key']['namespace']==stop['requested_binding']['namespace']==after['registry_namespace']
 assert after['payload_verified'] is True and after['approval']['manifest_sha256']==manifest and after['lifecycle']['pending'] is None and after['lifecycle']['explicitly_stopped'] is True
 return capture

def fresh(run,log,kind,seal,vsix):
 run=run.resolve(strict=True);assert run.parent==RUNS.resolve()
 receipt=read(log.with_suffix('.receipt.json'));assert receipt['exit_code']==receipt['actual_exit_code']==0 and receipt['validation_error'] is None
 assert receipt['source_before']==receipt['source_after']==CURRENT and receipt['source_unchanged'] is True and sha(log)==receipt['log_sha256']
 assert Path(receipt['command'][-1]).resolve()==vsix.resolve()
 summary=read(run/'summary.json');assert summary['status']=='passed' and summary['qualifiesFullScenarioSelection'] is True and summary['selectedScenarios']==[kind]
 assert summary['staged'] is False and Path(summary['source']).resolve()==vsix.resolve() and summary['nativeInstallDigest']==seal['install_manifest_sha256']
 assert len(summary['scenarios'])==1
 folder=run/kind;result=role(folder,'a',summary);assert result['trusted'] is True
 retained=read(folder/'a-retained-evidence.json');assert result['native'][:len(retained['native'])]==retained['native']
 transports=[v for row in result['native'] for v in row.get('transport',[])];assert transports
 assert any(r.get('method')=='initialize' and 'result' in r for r in transports) and any(r.get('method')=='tools/list' and 'result' in r for r in transports)
 assert any('name' in r and 'args' in r and 'result' in r for r in transports)
 clients=list(folder.glob('a-mcp-*-cleanup.json'));assert clients
 for p in clients:
  cleanup=read(p);assert cleanup['closed'] is True and cleanup['forced'] is False and cleanup['result']['code']==0
 capture=native_stop(folder,CURRENT,seal['install_manifest_sha256'],seal['files']['airk.exe'])
 proofs=[run/'summary.json',log,log.with_suffix('.receipt.json'),*[p for p in folder.iterdir() if p.is_file()]]
 if kind=='usage':
  assert Path(capture['runtime']['installDir']).resolve()==(folder/'native-service').resolve()
  primary=read(folder/'a-created.json');secondary=read(folder/'a-secondary.json')
  reports=[row['administrativeUsage'] for row in result['native'] if 'administrativeUsage' in row]
  cleared=[row['primaryAfterClear'] for row in result['native'] if 'primaryAfterClear' in row];assert len(reports)==3 and len(cleared)==1
  assert [r['administrative_scope']['index_id'] for r in reports]==[secondary['index'],primary['index'],secondary['index']]
  assert int(reports[0]['report']['calls'])>0 and int(reports[1]['report']['calls'])>0 and int(reports[2]['report']['calls'])>=int(reports[0]['report']['calls'])
  value=cleared[0];assert value['administrative_scope']['index_id']==primary['index'] and int(value['report']['calls'])==int(value.get('lifetime',{}).get('calls',0))==0
  assert not value.get('supplemental',[]) and not value.get('history',[])
  for value in [*reports,*cleared]:assert value['administrative_scope']['qualification']=='host_administrative_context_not_read_grant' and value['administrative_scope']['native_observation']
 else:
  fixture=read(folder/'legacy-fixture.json');assert fixture['qualification']['status']=='qualified_native_fixture' and fixture['candidate']['install_manifest_sha256']==seal['install_manifest_sha256']
  root=Path(fixture['fixture_root']).resolve(strict=True);assert root.is_relative_to((W/'artifacts').resolve())
  assert Path(capture['runtime']['installDir']).resolve()==Path(fixture['install_dir']).resolve() and capture['namespace']==fixture['native']['inspect']['registry_namespace']
  assert sha(fixture['legacy_config']['path'])==fixture['legacy_config']['sha256'];proofs.append(Path(fixture['legacy_config']['path']))
  for name,digest in fixture['source_hashes_before'].items():
   p=(Path(fixture['source_root'])/name).resolve(strict=True);assert p.is_relative_to(root) and sha(p)==digest==fixture['source_hashes_after'][name];proofs.append(p)
  assert result['legacyUpgradeCoverage'] is True and result['legacyRetrievalParity'] is True and summary['legacyUpgradeCoverage'] is True
  probes=[row['legacyProbe'] for row in result['native'] if 'legacyProbe' in row];assert len(probes)==1
  value=probes[0];assert value['scope_observation']['provenance']=='imported_legacy_snapshot' and len(value['scope_observation']['members'])==1
  args=fixture['legacy_config']['args'];binding=args[args.index('--service-project')+1]
  imported=next(index for index in fixture['native']['upgrade_receipt']['plan']['indexes'] if index['registration']['binding']==binding)
  member=value['scope_observation']['members'][0];assert member['index_id']==imported['index_id'] and [m['project_id'] for m in member['memberships']]==[imported['project_id']]
  assert len(value['items'])==1 and value['items'][0]['citation']['path']==fixture['probe']['path'] and value['items'][0]['citation']['content_hash']==fixture['probe']['content_sha256']
  for proof in fixture['native']['evidence_files']:assert sha(proof['path'])==proof['sha256'];proofs.append(Path(proof['path']))
 return {'scenario':kind,'status':'passed','native_source':CURRENT,'vsix_sha256':NEW_VSIX,'retained':False,'evidence':evidence(proofs),'raw_transport_count':len(transports)}

def main():
 p=argparse.ArgumentParser(description=__doc__)
 for name in ['usage-run','usage-log','legacy-run','legacy-log','output']:p.add_argument('--'+name,type=Path,required=True)
 a=p.parse_args();assert a.output.resolve().is_relative_to(B.resolve()) and not a.output.exists()
 adopted_path=B/'p14-editor-three-completed-adoption-r1.json';adopted=read(adopted_path);assert adopted['status']=='passed' and adopted['native_source']==OLD and adopted['vsix']['sha256']==OLD_VSIX
 assert all(sha(path)==digest for path,digest in adopted['evidence_sha256'].items())
 rows=[]
 for kind in ['same','different','multi']:
  assert next(r for r in adopted['scenario_results'] if r['scenario']==kind)['status']=='passed'
  folder=RUNS/'native-installed-cUDfG8'/kind
  native_stop(folder,OLD,'45da6d3a10b34313dea53881ce95a930f89aa304f019ba095406eb40b852085c','b6b4ed967a13888ed238cf9111da4158821df5acff6993b37334ea0d5ef987d8')
  rows.append({'scenario':kind,'status':'passed','native_source':OLD,'vsix_sha256':OLD_VSIX,'retained':True,'evidence':evidence([adopted_path,RUNS/'native-installed-cUDfG8/summary.json',*[p for p in folder.iterdir() if p.is_file()]])})
 seal_path=B/'p14-runtime-final-r3-seal.json';assert sha(seal_path)=='46b3593803b7ce0f48c5811dd9e666faff416e15430a6251b58c44dccb2f6da2';seal=read(seal_path)
 package_path=B/'p14-current-windows-package-r1.json';assert sha(package_path)=='89060658418b68f5d135a00dd60053fbb42dcbfb3c52f2cab6287fce330d554b'
 package=read(package_path);assert package['status']=='passed' and package['native_source']==CURRENT
 vsix=W/'artifacts/p14-package-editor-r3/extension/publish/airk-1.8.8-win32-x64.vsix';assert sha(vsix)==NEW_VSIX
 rows.append(fresh(a.usage_run,a.usage_log,'usage',seal,vsix))
 historical=RUNS/'native-installed-lKgu1x';summary=read(historical/'summary.json');assert summary['status']=='passed' and summary['selectedScenarios']==['untrusted'] and summary['qualifiesFullScenarioSelection'] is True
 assert summary['nativeInstallDigest']=='45da6d3a10b34313dea53881ce95a930f89aa304f019ba095406eb40b852085c' and sha(summary['source'])==OLD_VSIX
 folder=historical/'untrusted';result=role(folder,'a',summary);assert result['trusted'] is False and not(folder/'native-service').exists() and list((folder/'storage').iterdir())==[]
 assert any(row.get('value',{}).get('blocked') is True for row in result['native'])
 log=B/'p14-package-continuation-untrusted-private-scenario-r5.log';receipt=read(log.with_suffix('.receipt.json'));assert receipt['exit_code']==receipt['actual_exit_code']==0 and receipt['source_before']==receipt['source_after']==OLD and sha(log)==receipt['log_sha256']
 rows.append({'scenario':'untrusted','status':'passed','native_source':OLD,'vsix_sha256':OLD_VSIX,'retained':True,'evidence':evidence([historical/'summary.json',log,log.with_suffix('.receipt.json'),*[p for p in folder.iterdir() if p.is_file()]])})
 rows.append(fresh(a.legacy_run,a.legacy_log,'legacy',seal,vsix))
 record={'schema':'airk.p14.editor-coverage/1','status':'passed','current_native_source':CURRENT,'scenario_results':rows,'package_evidence':evidence([package_path,seal_path,B/'p14-stock-before-current-editor-r1.json']),'limits':'Four historical scenarios retain original5d89/49170identity. Only whole fresh usage/legacy qualify current237/newVSIX. Failed older queues remain failed; no publication or postrecovery installer claim.'}
 with a.output.open('x',encoding='utf-8') as output:json.dump(record,output,indent=2);output.write('\n')
 print(json.dumps({'output':str(a.output),'sha256':sha(a.output),'scenarios':[r['scenario'] for r in rows]}))
if __name__=='__main__':main()
