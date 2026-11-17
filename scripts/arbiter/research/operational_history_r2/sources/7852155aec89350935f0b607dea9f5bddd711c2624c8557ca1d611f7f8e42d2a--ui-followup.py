# SPDX-License-Identifier: Apache-2.0
"""Correct the Code executable path only; run no compiler or native service."""
import hashlib,json,os,plistlib,signal,subprocess,time
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source';evidence=run/'evidence'
assert run.parent==Path.home()/'airk-builds' and run.name.startswith('explorer-1812-')
assert (run/'qualification.json').is_file() and (run/'cleanup.json').is_file()
assert json.loads((run/'cleanup.json').read_text())['exit_code']==0 and not (run/'target').exists()
tools=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools';node=tools/'node-v24.15.0-darwin-arm64/bin/node'
contents=Path('/Applications/Visual Studio Code.app/Contents');executable=contents/'MacOS'/plistlib.loads((contents/'Info.plist').read_bytes())['CFBundleExecutable'];cli=contents/'Resources/app/out/cli.js'
env=dict(os.environ,PATH=str(node.parent)+':'+str(tools)+':'+str(Path.home()/'.cargo/bin')+':'+os.environ.get('PATH',''))
rows=[]
for name,args in [('installed-explorer-corrected',['scripts/check-installed-explorer.cjs',str(executable),str(cli),'artifacts/neo/vscode/publish/airk-1.8.12-darwin-arm64.vsix']),('bootstrap-ui-corrected',['scripts/check-extension-update-editor.cjs',str(executable)])]:
 start=time.monotonic();log=evidence/(name+'.log')
 with log.open('x') as out:
  child=subprocess.Popen([str(node),*args],cwd=source,env=env,stdout=out,stderr=subprocess.STDOUT,start_new_session=True)
  timeout=False
  try:code=child.wait(timeout=180)
  except subprocess.TimeoutExpired:
   timeout=True;os.killpg(child.pid,signal.SIGTERM)
   try:code=child.wait(timeout=10)
   except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);code=child.wait(timeout=10)
 rows.append({'name':name,'command':[str(node),*args],'exit_code':code,'timed_out':timeout,'elapsed_seconds':round(time.monotonic()-start,3),'log':str(log)})
 (run/'ui-followup.json').write_text(json.dumps({'schema':'airk.neo-ui-corrected-path/1','scope':'Actual installed Mac production controller/UI with nativeexecution disabled, plus separate fictional managed-entry bootstrap fixture; no Cargo/native service/window reload.','actual_code_executable':str(executable),'original_invocation':'Queued owner used absent Contents/MacOS/Electron; this follow-up uses exact CFBundleExecutable without modifying the application.','steps':rows,'target_remains_absent':not (run/'target').exists(),'status':'passed' if all(r['exit_code']==0 for r in rows) else 'failed'},indent=2)+'\n')
 print(json.dumps(rows[-1]),flush=True)
raise SystemExit(0 if all(r['exit_code']==0 for r in rows) else 1)
