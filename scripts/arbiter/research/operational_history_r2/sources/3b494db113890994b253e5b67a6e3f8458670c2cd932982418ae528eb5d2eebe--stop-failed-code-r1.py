# SPDX-License-Identifier: Apache-2.0
"""One ownership-verified Stop for this known failed Code fixture only."""
import hashlib
import importlib.util
import json
from pathlib import Path

root=Path(__file__).resolve().parents[4]
owner=root/'artifacts/editor-qualification/native-installed-b1bqLY/multi'
runtime=root/'artifacts/local/one-airk-1.8.14/windows-debug-r24/runtime'
manifest=hashlib.sha256((runtime/'airk-install.json').read_bytes()).hexdigest()
assert manifest=='501c8d85a8d6b836489d4d425f0cb142d3aa965221f70629899dd85de58b7acc'
prior=json.loads((owner/'managed-start.json').read_text())
assert prior['health']['runtime_sha256']==manifest
recovery=owner/'owned-stop-recovery-r1';recovery.mkdir(exist_ok=False)
spec=importlib.util.spec_from_file_location('clients',root/'scripts/check-service-clients.py')
clients=importlib.util.module_from_spec(spec);spec.loader.exec_module(clients)
fixture=clients.Fixture(runtime,recovery,command_timeout=300)
fixture.install=owner/'native-service';fixture.control=owner/'state/control'
inspection=json.loads(fixture.run('runtime','inspect','--install-dir',fixture.install).stdout)
expected=fixture.install/'runtimes'/manifest/'airk.exe'
assert Path(inspection['executable']).samefile(expected)
assert Path(inspection['control_dir']).samefile(fixture.control)
assert inspection['selection']['active']==manifest
assert hashlib.sha256(expected.read_bytes()).hexdigest()==hashlib.sha256((runtime/'airk.exe').read_bytes()).hexdigest()
health=fixture.rpc('health')['observation']
assert health['runtime_sha256']==manifest
assert health['executable_sha256']==hashlib.sha256(expected.read_bytes()).hexdigest()
fixture.binary=expected;fixture.managed_health=health;fixture.managed_started=True
fixture.close()
assert fixture.managed_stop['record']['phase']=='complete'
sources={}
for name,symbol in [('workspace-a','WindowAlphaEvidence'),('workspace-b','WindowBetaEvidence')]:
    sources[name+'/fixture.ts']=f'export function {symbol}() {{ return 42; }}\n'
    sources[name+'/hidden.ts']='export function HiddenPrivateEvidence() {}\n'
for name in ('shared-a','shared-b'):
    sources[name+'/shared.ts']='export function SharedAllowedEvidence() { return 19; }\n'
checks={name:{'expected':hashlib.sha256(text.encode()).hexdigest(),'observed':hashlib.sha256((owner/name).read_bytes()).hexdigest()} for name,text in sources.items()}
assert all(row['expected']==row['observed'] for row in checks.values())
result={'schema':1,'owned_fixture_only':True,'original_failed_campaign_relabelled':False,'runtime_manifest_sha256':manifest,
        'native_stop_complete':True,'source_hashes':checks,'qualified':True,'scope':'Separate known-fixture cleanup only; failed Code TLS/multi acceptance remains failed'}
(recovery/'result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'native_stop_complete':True,'six_source_hashes_preserved':True,'original_campaign_still_failed':True}))
