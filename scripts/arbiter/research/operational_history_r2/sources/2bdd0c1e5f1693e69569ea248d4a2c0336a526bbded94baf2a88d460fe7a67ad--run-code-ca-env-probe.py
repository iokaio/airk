# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import uuid
from cryptography import x509
from cryptography.hazmat.primitives import hashes

root=Path(__file__).resolve().parents[4]
base=Path(__file__).resolve().parents[1]
private=Path(json.loads((base/'user-private-fixture-base.json').read_text())['path']).resolve(strict=True)
owner=private/('code-ca-env-probe-'+uuid.uuid4().hex);owner.mkdir()
assert owner.resolve().is_relative_to(private)
bundle=root/'artifacts/editor-qualification/native-installed-b1bqLY/multi/code-additive-ca.pem'
expected='da2c7c366a30735e6571b7ce788a0b23e4f13d3e3d4e8e91b66f732436fee8dc'
assert any(cert.fingerprint(hashes.SHA256()).hex()==expected for cert in x509.load_pem_x509_certificates(bundle.read_bytes()))
extension=owner/'probe-extension';extension.mkdir()
(extension/'package.json').write_text(json.dumps({'name':'airk-ca-observation','publisher':'local-test','version':'0.0.1',
    'engines':{'vscode':'^1.140.0'},'main':'probe.cjs','activationEvents':['onStartupFinished']}))
(extension/'probe.cjs').write_bytes(Path(__file__).with_name('code-ca-env-probe.cjs').read_bytes())
workspace=owner/'workspace';workspace.mkdir()
profile=owner/'profile';(profile/'User').mkdir(parents=True)
(profile/'User/settings.json').write_text(json.dumps({'workbench.startupEditor':'none','window.restoreWindows':'none',
    'extensions.autoUpdate':False,'extensions.autoCheckUpdates':False,'telemetry.telemetryLevel':'off','update.mode':'none'}))
extensions=owner/'extensions';extensions.mkdir()
home=owner/'client-home';home.mkdir()
env={key:value for key,value in os.environ.items() if not key.upper().startswith('VSCODE_')
     and key.upper() not in ('ELECTRON_RUN_AS_NODE','NODE_COMPILE_CACHE')}
env.update(HOME=str(home),USERPROFILE=str(home),CODEX_HOME=str(home/'.codex'),CLAUDE_CONFIG_DIR=str(home/'.claude'),
    XDG_CONFIG_HOME=str(home/'.config'),APPDATA=str(home/'AppData/Roaming'),LOCALAPPDATA=str(home/'AppData/Local'),
    NODE_EXTRA_CA_CERTS=str(bundle),AIRK_PROBE_CA_FILE=str(bundle),AIRK_PROBE_CA_SHA256=expected,AIRK_PROBE_RESULT=str(owner/'result.json'))
args=['C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe',str(workspace),'--new-window','--user-data-dir',str(profile),
    '--extensions-dir',str(extensions),'--extensionDevelopmentPath='+str(extension),'--skip-welcome','--skip-release-notes',
    '--disable-updates','--disable-telemetry','--disable-gpu','--disable-workspace-trust']
start=time.monotonic()
with (owner/'editor.log').open('xb') as log:
    child=subprocess.Popen(args,env=env,stdout=log,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW)
    try:code=child.wait(timeout=60)
    except subprocess.TimeoutExpired:
        child.terminate();code=child.wait(timeout=10)
receipt={'schema':1,'arguments':args,'exit_code':code,'elapsed_seconds':time.monotonic()-start,
    'owned_probe':str(owner),'bundle_sha256':hashlib.sha256(bundle.read_bytes()).hexdigest(),
    'mcp_or_model_requests':0,'os_trust_mutated':False,'source_sha256':hashlib.sha256(Path(__file__).with_name('code-ca-env-probe.cjs').read_bytes()).hexdigest()}
if (owner/'result.json').exists():receipt['actual_extension_host']=json.loads((owner/'result.json').read_text())
with (base/'windows-debug-r24-code-ca-environment-probe-r1.json').open('x') as output:json.dump(receipt,output,indent=2)
print(json.dumps(receipt),flush=True)
assert code==0 and receipt.get('actual_extension_host',{}).get('observed') is True
