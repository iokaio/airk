# SPDX-License-Identifier: Apache-2.0
import hashlib,json,zipfile
from pathlib import Path
B=Path(__file__).resolve().parent
W=B/'p10-worktree'
RUN=W/'artifacts/editor-qualification/native-installed-I8ic3Y'
D=RUN/'legacy'
OUT=B/'p14-current-legacy-independent-audit-r1.json'
assert not OUT.exists()
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
read=lambda p:json.loads(Path(p).read_text(encoding='utf-8'))
expected={'sha256':'237f896933ad59a4075f9395fd53fcb92e376946faf1fc01036c139fc0533c0f','files':960}
s=read(RUN/'summary.json');f=read(D/'legacy-fixture.json');r=read(D/'a-result.json')
assert s['status']=='passed' and s['selectedScenarios']==['legacy'] and s['qualifiesFullScenarioSelection'] is True
assert s['legacyUpgradeCoverage'] is True and s['staged'] is False and len(s['scenarios'])==1
assert r['installed'] is True and r['trusted'] is True and r['version']=='1.8.8'
assert r['legacyRetrievalParity'] is True and r['legacyUpgradeCoverage'] is True and 'failure' not in r
assert f['qualification']['status']=='qualified_native_fixture' and f['legacy_config']['binding_kind']=='stable_launcher'
config=f['legacy_config'];assert sha(config['path'])==config['sha256']
for name,digest in f['source_hashes_before'].items():
 assert f['source_hashes_after'][name]==digest and sha(Path(f['source_root'])/name)==digest
probe=next(n['legacyProbe'] for n in r['native'] if 'legacyProbe' in n)
transport=next(n['transport'] for n in r['native'] if 'transport' in n)
launch=transport[0]['launch'];assert Path(launch['command']).samefile(config['command'])
assert launch['args']==config['args'] and Path(launch['cwd']).samefile(config['cwd'])
assert [n.get('method',n.get('name')) for n in transport[1:]]==['initialize','tools/list','symbols']
wire=transport[-1]['result'];assert not wire.get('error') and wire['result'].get('isError') in [None,False]
body=json.loads(wire['result']['content'][0]['text']);assert body==probe
if 'structuredContent' in wire['result']:assert wire['result']['structuredContent']==body
assert body['scope_observation']['provenance']=='imported_legacy_snapshot'
binding=config['args'][config['args'].index('--service-project')+1]
imported=next(i for i in f['native']['upgrade_receipt']['plan']['indexes'] if i['registration']['binding']==binding)
member=body['scope_observation']['members'];assert len(member)==1 and member[0]['index_id']==imported['index_id']
assert [m['project_id'] for m in member[0]['memberships']]==[imported['project_id']]
assert len(body['items'])==1 and body['items'][0]['citation']['path']==f['probe']['path']
assert body['items'][0]['citation']['content_hash']==f['probe']['content_sha256']
assert [n['action'] for n in r['native'] if 'action' in n]==['startService','serviceStop']
before=read(D/'native-pre-start-inspect.json');assert before['lifecycle']['explicitly_stopped'] is True
stop=read(D/'native-stop.json');captured=stop['capture'];rec=captured['response']['record'];req=rec['request']
assert captured['qualification']=='historical_native_lifecycle_response' and captured['response']['status']=='lifecycle'
assert rec['phase']=='complete' and req['action']=='stop' and captured['force'] is False
assert req['expected_instance']==captured['before_health']['observation']['instance']
assert captured['before_health']['observation']['build']['source_sha256']==expected['sha256']
assert req['runtime']==captured['runtime']['runtimeSha256']==s['nativeInstallDigest']==f['candidate']['install_manifest_sha256']
assert req['key']['namespace']==captured['namespace']==stop['requested_binding']['namespace']==f['native']['inspect']['registry_namespace']
assert Path(captured['runtime']['installDir']).samefile(f['install_dir'])
assert captured['capturedAt']>=stop['started'] and stop['dispatch']['dispatch']!=stop['confirmation']['dispatch']
assert rec['id']!=(stop.get('previous_capture') or {}).get('response',{}).get('record',{}).get('id')
cleanup=read(D/'native-cleanup.json');assert cleanup['qualified'] is True and cleanup['stop']==stop
inspect=cleanup['observation'];assert inspect['payload_verified'] is True
assert inspect['approval']['manifest_sha256']==req['runtime'] and inspect['registry_namespace']==captured['namespace']
assert inspect['lifecycle']['explicitly_stopped'] is True and inspect['lifecycle']['pending'] is None
assert Path(inspect['control_dir']).samefile(f['control_dir']) and Path(captured['runtime']['controlDir']).samefile(f['control_dir'])
for p in D.glob('a-mcp-*-cleanup.json'):
 c=read(p);assert c['closed'] is True and c['forced'] is False and c['result']['code']==0
assert list(D.glob('a-mcp-*-cleanup.json'))
assert (D/'capture-complete.json').is_file() and (D/'a-workbench.png').is_file() and (D/'a-service.txt').is_file()
for row in read(D/'source-preservation.json'):assert row['before_sha256']==row['after_sha256'] and row['intentional_peer_edit'] is False
vsix=Path(s['source']);vsix_sha=sha(vsix)
with zipfile.ZipFile(vsix) as z:
 names=[n for n in z.namelist() if n.endswith('/airk-install.json')]
 assert len(names)==1
 native=z.read(names[0]);assert hashlib.sha256(native).hexdigest()==req['runtime']
manifest_path=B/'p14-runtime-final-r3/airk-install.json';assert sha(manifest_path)==req['runtime']
assert native==manifest_path.read_bytes()
producer=read(B/'p14-release-runtime-r3.receipt.json')
assert producer['source_before']==producer['source_after']==expected and producer['source_unchanged'] is True
assert producer['exit_code']==producer['actual_exit_code']==0
assert sha(B/'p14-release-runtime-r3.log')==producer['log_sha256']
for name in ['p14-current-editor-legacy-r1','p14-current-legacy-fixture-r1','p14-current-native-package-r1','p14-current-vsix-package-r1']:
 receipt=read(B/(name+'.receipt.json'));assert receipt['exit_code']==receipt['actual_exit_code']==0
 assert receipt['source_before']==receipt['source_after']==expected and receipt['source_unchanged'] is True
 assert sha(B/(name+'.log'))==receipt['log_sha256']
evidence=[RUN/'summary.json',D/'a-result.json',D/'a-retained-evidence.json',D/'legacy-fixture.json',D/'native-pre-start-inspect.json',D/'native-pre-stop-inspect.json',D/'native-stop.json',D/'native-cleanup.json',D/'source-preservation.json',B/'p14-current-editor-legacy-r1.receipt.json',B/'p14-current-legacy-fixture-r1.receipt.json',B/'p14-release-runtime-r3.receipt.json']
result={'status':'passed','scope':'Independent bytes/raw/receipt review of actual new-source WHOLE legacy editor scenario, not a rerun or all-scenario acceptance','native_source':expected,'vsix_sha256':vsix_sha,'native_install_sha256':req['runtime'],'actual_scenario_seconds':read(B/'p14-current-editor-legacy-r1.receipt.json')['seconds'],'actual_fixture_seconds':read(B/'p14-current-legacy-fixture-r1.receipt.json')['seconds'],'producer_seconds':producer['seconds'],'original_config_and_source_preserved':True,'raw_imported_scope_and_citation_verified':True,'explicit_start_manual_close_single_stop_cleanup_verified':True,'proofs':{str(p):sha(p) for p in evidence},'limits':'Old retained scenarios preserve their original5d89/49170 identity. Current usage remains failed/pending. Current full qualification and P14 acceptance not established by this review.'}
with OUT.open('x') as o:json.dump(result,o,indent=2)
print(json.dumps({'audit_sha256':sha(OUT),'vsix_sha256':vsix_sha,'native_install_sha256':req['runtime'],'status':'passed'}))
