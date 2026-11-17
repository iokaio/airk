# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,os,plistlib,subprocess,time,zipfile
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source';evidence=run/'evidence'
assert json.loads((run/'gui-sign-result.json').read_text())['status']=='passed' and not (run/'target').exists()
spec=importlib.util.spec_from_file_location('packager',source/'scripts/package-extension.py');packager=importlib.util.module_from_spec(spec);spec.loader.exec_module(packager)
vsix=source/'artifacts/neo/vscode-signed-gui1/publish/airk-1.8.12-darwin-arm64.vsix'
with zipfile.ZipFile(vsix) as z:inventory=packager.read_update_inventory(z,'1.8.12','darwin-arm64')
spec=importlib.util.spec_from_file_location('builder',source/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
native_source=builder.source_identity();assert native_source=={'sha256':'51536a705cf9cbabe4947cacddf43a8cb29b0af8bad4e23ca7f293aa6dd347fa','files':978}
native=source/'artifacts/neo/native/publish';receipt=json.loads((native/'manifest.json').read_text())
with (native/receipt['archive']).open('rb') as f:original_sha=hashlib.file_digest(f,'sha256').hexdigest()
assert original_sha=='e0a9ef2d9d36ff9da7262e3303f866b0c5df8b2030dc8cc644ddf1018d579388'
tools=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools';node=tools/'node-v24.15.0-darwin-arm64/bin/node';contents=Path('/Applications/Visual Studio Code.app/Contents')
executable=contents/'MacOS'/plistlib.loads((contents/'Info.plist').read_bytes())['CFBundleExecutable'];cli=contents/'Resources/app/out/cli.js'
env=dict(os.environ,PATH=str(node.parent)+':'+str(tools)+':'+str(Path.home()/'.cargo/bin')+':'+os.environ.get('PATH',''),PYTHONDONTWRITEBYTECODE='1')
command=[str(node),'scripts/check-installed-explorer.cjs',str(executable),str(cli),str(vsix)];start=time.monotonic()
with (evidence/'signed-installed-explorer.log').open('x') as output:p=subprocess.run(command,cwd=source,env=env,stdout=output,stderr=subprocess.STDOUT,timeout=200)
with (evidence/'cargo-clean-signed-ui.log').open('x') as output:clean=subprocess.run(['cargo','clean','--manifest-path',str(source/'Cargo.toml'),'--target-dir',str(run/'target')],cwd=source,env=env,stdout=output,stderr=subprocess.STDOUT)
with vsix.open('rb') as f:vsix_sha=hashlib.file_digest(f,'sha256').hexdigest()
report={'schema':'airk.neo-signed-installed-explorer/1','status':'passed' if p.returncode==0 and clean.returncode==0 else 'failed','sealed_inventory_verified':True,'command':command,'exit_code':p.returncode,'elapsed_seconds':round(time.monotonic()-start,3),'vsix_sha256':vsix_sha,'native_execution_in_editor':False,'native_signed_install_and_model_checks_separate':True,'cargo_clean_exit_code':clean.returncode,'target_absent':not (run/'target').exists(),'notarization_submitted':False,'publication':False}
report.update({'native_source':native_source,'original_native_archive_sha256':original_sha,'original_native_producer_reused':True})
(evidence/'signed-ui-result.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report),flush=True)
