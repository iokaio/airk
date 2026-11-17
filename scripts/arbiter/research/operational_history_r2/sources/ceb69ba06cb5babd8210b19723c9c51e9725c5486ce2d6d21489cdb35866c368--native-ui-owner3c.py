# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,re,signal,subprocess,tarfile,time,zipfile
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source3c6e81c';evidence=run/'evidence3c';assert json.loads((run/'notary3c-result.json').read_text())['status']=='passed'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip();assert commit=='3c6e81cf5911751aac8dbe6767ea77abab11d206'
names=['scripts/check-native-editor.cjs','scripts/editor-ui.cjs','scripts/native-editor-receipts.cjs','scripts/native-editor-idle.cjs','packages/vscode/src/test/installedEditor.ts','packages/vscode/src/test/installedEditorContracts.ts']
identity={n:sha(source/n) for n in names}
for n,h in identity.items():assert hashlib.sha256(subprocess.check_output(['git','show','HEAD:'+n],cwd=source)).hexdigest()==h
assert identity['scripts/check-native-editor.cjs']=='b13fc71cfd57f729b06a702888afcfec900dfde86545b8ced0c2bdf022bbfc1e' and identity['scripts/native-editor-idle.cjs']=='4ebbc88ec0e86fa3776f150641b8bc73d2b56be8c67bd0a39f83fa25fdc547c9'
overlay={'commit':commit,'source_sha256':identity,'scope':'Committed reviewed harness and host; no worktree test overlay'}
production=source/'packages/vscode/out'
def production_hashes():return {p.relative_to(production).as_posix():sha(p) for p in production.rglob('*') if p.is_file() and 'test' not in p.relative_to(production).parts}
before=production_hashes();tools=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools';nodebin=tools/'node-v24.15.0-darwin-arm64/bin';env=dict(os.environ,PATH=str(nodebin)+':'+str(tools)+':'+str(Path.home()/'.cargo/bin')+':'+os.environ.get('PATH',''),PYTHONDONTWRITEBYTECODE='1',AIRK_EDITOR_SCENARIOS='same')
report={'schema':'airk.neo-native-enabled-installed-editor/1','reviewed_test_source':overlay,'production_candidate_rebuilt':False,'native_compilation':False,'steps':[]};error=None
def step(name,command,timeout):
 start=time.monotonic();log=evidence/(name+'.log');(run/'native-ui3c-phase.txt').write_text(name)
 with log.open('x') as output:
  child=subprocess.Popen(command,cwd=source,env=env,stdout=output,stderr=subprocess.STDOUT,start_new_session=True)
  try:code=child.wait(timeout=timeout);timed=False
  except subprocess.TimeoutExpired:
   timed=True;os.killpg(child.pid,signal.SIGTERM)
   try:code=child.wait(timeout=5)
   except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);code=child.wait(timeout=5)
 row={'name':name,'command':command,'exit_code':code,'timed_out':timed,'elapsed_seconds':round(time.monotonic()-start,3)};report['steps'].append(row);print(json.dumps(row),flush=True)
 if code:raise RuntimeError(name+' failed')
try:
 # Exact committed tests already compiled by the complete preceding npm test batch.
 after=production_hashes();report['production_compile_outputs_unchanged']=before==after;assert before==after
 report['compiled_test_sha256']={n:sha(source/'packages/vscode/out/test'/n) for n in ['installedEditor.js','installedEditorContracts.js']}
 vsix=source/'artifacts/neo/vscode-notarized-3c/publish/airk-1.8.12-darwin-arm64.vsix';candidate=json.loads((vsix.parent/'manifest.json').read_text());assert candidate['commit']==commit and sha(vsix)==candidate['archive_sha256'];report['vsix_sha256']=sha(vsix)
 step('native-enabled-installed-same3c',[str(nodebin/'node'),'scripts/check-native-editor.cjs','/Applications/Visual Studio Code.app/Contents/MacOS/Code','/Applications/Visual Studio Code.app/Contents/Resources/app/out/cli.js',str(vsix)],780)
 report['status']='passed'
except BaseException as exc:report['status']='failed';error={'type':type(exc).__name__,'error':str(exc)}
finally:
 log=evidence/'native-enabled-installed-same3c.log'
 if log.exists():
  m=re.search(r'Native editor output: (.+)',log.read_text(errors='replace'))
  if m:
   owned=Path(m.group(1).strip()).resolve(strict=True);assert owned.is_relative_to(source/'artifacts/editor-qualification') and owned.name.startswith('native-installed-');report['editor_run']=str(owned)
   summary=owned/'summary.json'
   if summary.exists():report['summary']=json.loads(summary.read_text())
   launcher=owned/'same/native-service/airk-launcher'
   if launcher.is_file():
    observations=[]
    for cmd in [['/usr/bin/xattr','-p','com.apple.quarantine',str(launcher)],['/usr/bin/codesign','--verify','--strict','--check-notarization',str(launcher)],[str(launcher),'--version']]:
     try:
      p=subprocess.run(cmd,capture_output=True,text=True,timeout=45);observations.append({'command':cmd,'exit_code':p.returncode,'stdout':p.stdout[-4096:],'stderr':p.stderr[-4096:]})
     except subprocess.TimeoutExpired:observations.append({'command':cmd,'timed_out':True})
    report['actual_installed_launcher']={'path':str(launcher),'sha256':sha(launcher),'observations':observations}
 report.update({'error':error,'target_absent':not (source/'target').exists() and not (run/'target').exists()})
 (evidence/'native-ui3c-result.json').write_text(json.dumps(report,indent=2)+'\n');(run/'native-ui3c-phase.txt').write_text('complete');print(json.dumps({k:v for k,v in report.items() if k not in ['summary','reviewed_test_source']}),flush=True)
