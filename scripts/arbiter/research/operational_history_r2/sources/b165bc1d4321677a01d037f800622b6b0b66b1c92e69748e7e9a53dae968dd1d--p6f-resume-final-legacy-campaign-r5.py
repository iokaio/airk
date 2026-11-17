# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import importlib.util,json,os,subprocess,time,uuid
ROOT=Path(__file__).resolve().parents[2]
node=ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe'
code=Path('C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe')
cli=code.parent/'261c84cb1e/resources/app/out/cli.js'
vsix=ROOT/'artifacts/v190/p6f-resume-final-installed-extension-r8/publish/airk-1.8.8-win32-x64.vsix'
payload=ROOT/'artifacts/v190/p6f-resume-final-release-shared-copy-r5'
fixture_root=ROOT/'artifacts/v190/p6f-resume-legacy-upgrade-editor-owner-r6'
deadline=time.monotonic()+1800
while not (payload/'airk-install.json').is_file() or not vsix.is_file():
 if time.monotonic()>deadline:raise TimeoutError('Final immutable editor package unavailable')
 time.sleep(2)
def run(command,log,environment=None,input=None):
 with (ROOT/'artifacts/v190'/log).open('w',encoding='utf-8') as output:
  result=subprocess.run(list(map(str,command)),cwd=ROOT,env=environment,input=input,text=True,stdout=output,stderr=subprocess.STDOUT,creationflags=0x08000000)
 if result.returncode:raise RuntimeError(f'{log} failed: {result.returncode}')
run(['python',ROOT/'artifacts/v190/p6f-resume-legacy-upgrade-editor-owner-r6.py'],'p6f-resume-final-legacy-fixture-r6.log')
fixture=json.loads((fixture_root/'fixture.json').read_bytes())
run([payload/'airk.exe','runtime','start','--install-dir',fixture['install_dir']],'p6f-resume-final-legacy-start-r6.log')
try:
 environment=dict(os.environ,AIRK_EDITOR_SCENARIOS='legacy',AIRK_EDITOR_LEGACY_FIXTURE=str(fixture_root/'fixture.json'))
 run([node,ROOT/'scripts/check-native-editor.cjs',code,cli,vsix],'p6f-resume-final-installed-legacy-r5.log',environment)
finally:
 specification=importlib.util.spec_from_file_location('owned_native_console',ROOT/'scripts/check-client-lifetime.py');module=importlib.util.module_from_spec(specification);specification.loader.exec_module(module)
 console=module.NativeConsole(payload/'airk.exe',fixture['control_dir'])
 try:
  health=console.request('health')['observation'];epoch=console.request('request_epoch')
 finally:console.close()
 request={'caller':uuid.uuid4().hex,'key':{'namespace':epoch['namespace'],'epoch':epoch['epoch'],'nonce':uuid.uuid4().hex},'action':'stop','expected_instance':health['instance'],'runtime':fixture['candidate']['install_manifest_sha256'],'force':False}
 result=subprocess.run([str(payload/'airk.exe'),'runtime','lifecycle','--install-dir',fixture['install_dir']],input=json.dumps(request),text=True,capture_output=True,timeout=180,creationflags=0x08000000)
 (fixture_root/'editor-native-stop.stdout').write_text(result.stdout);(fixture_root/'editor-native-stop.stderr').write_text(result.stderr)
 assert result.returncode==0,result.stderr
 receipt=json.loads(result.stdout);assert receipt['status']=='lifecycle' and receipt['record']['phase']=='complete',receipt
 (fixture_root/'editor-native-stop.json').write_text(json.dumps(receipt,indent=2))
print('Final receipt-only repack legacy scenario passed; native Stop complete',flush=True)
