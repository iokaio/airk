# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
command=[r'C:\Program Files\nodejs\node.exe','scripts/check-native-editor.cjs','C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe','C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js','artifacts/local/one-airk-1.8.14/windows-debug-r39-vsix-final/publish/airk-1.8.16-win32-x64.vsix']
env=dict(os.environ,AIRK_EDITOR_SCENARIOS='untrusted,multi,same')
for key in ['AIRK_TEST_BINARY','AIRK_NATIVE_TEST_BINARY','AIRK_TEST_F09_QUALIFICATION']:env.pop(key,None)
print(json.dumps({'command':command,'scenarios':env['AIRK_EDITOR_SCENARIOS'],'native_overrides':'removed'}),flush=True)
sys.exit(subprocess.run(command,env=env).returncode)