# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import subprocess,json,os,time,hashlib,re,traceback
out=Path(__file__).resolve().parent;root=out/'source';node=Path(r'D:/code/Github/airk/artifacts/tools/node-v24.15.0-win-x64/node.exe');npm=node.parent/'node_modules/npm/bin/npm-cli.js';manifest=json.loads((out/'source-manifest.json').read_bytes());result={'schema':'airk.private.ui-title-build/1','passed':False,'runs':[],'source_manifest_sha256':hashlib.sha256((out/'source-manifest.json').read_bytes()).hexdigest()}
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
env={**os.environ,'PATH':str(node.parent)+os.pathsep+os.environ['PATH'],'TEMP':str(out/'test-temp'),'TMP':str(out/'test-temp'),'npm_config_cache':str(out/'npm-cache'),'PYTHONDONTWRITEBYTECODE':'1'}
for name in ['test-temp','npm-cache']:(out/name).mkdir()
result['node']=subprocess.check_output([str(node),'--version'],text=True).strip();result['npm']=subprocess.check_output([str(node),str(npm),'--version'],env=env,text=True).strip();assert result['node']=='v24.15.0'
inputs=[row for row in manifest['frozen_relevant_inputs'] if not row['path'].startswith('crates/airk-cli/admin-assets/')]
def verify_inputs():
 for row in inputs:assert sha(root/row['path'])==row['sha256'],'Frozen consumed input changed '+row['path']
selected=['packages/admin/test/navigation.test.mjs','packages/ui/test/pageTitles.dom.test.mjs','packages/ui/test/navigation.test.mjs','packages/ui/test/managementNavigation.test.mjs','packages/ui/test/management.dom.test.mjs','packages/ui/test/outerManagement.dom.test.mjs','packages/ui/test/help.dom.test.mjs','packages/ui/test/generatedHelp.dom.test.mjs','packages/ui/test/evidenceTools.dom.test.mjs']
batches=[('admin-build',[str(node),str(npm),'run','build','--prefix','packages/admin']),('admin-check',[str(node),str(npm),'run','check','--prefix','packages/admin']),('explorer-browser-bundles',[str(node),'packages/vscode/scripts/build-explorer.cjs']),('focused-behavior',[str(node),'--test','--test-reporter=tap','--test-concurrency=2',*selected])]
try:
 verify_inputs()
 for name,command in batches:
  started=time.monotonic();log=out/(name+'.log')
  with log.open('x',encoding='utf-8') as stream:completed=subprocess.run(command,cwd=root,env=env,stdout=stream,stderr=subprocess.STDOUT,text=True,timeout=180,creationflags=subprocess.CREATE_NO_WINDOW)
  row={'name':name,'argv':command,'cwd':str(root),'elapsed_seconds':time.monotonic()-started,'exit_code':completed.returncode,'log':str(log),'log_sha256':sha(log)}
  if name=='focused-behavior':
   text=log.read_text();row['counts']={key:int(re.search('^# '+key+' ([0-9]+)',text,re.MULTILINE).group(1)) for key in ['tests','pass','fail','cancelled','skipped']};assert row['counts']['tests']>0
  result['runs'].append(row);(out/'result.json').write_text(json.dumps(result,indent=2)+chr(10));verify_inputs()
  print(json.dumps(row),flush=True)
  assert completed.returncode==0,'Stop failed batch; diagnostics '+str(log)
 before={r['path'].removeprefix('packages/vscode/out/'):r for r in manifest['warm_transfer'] if r['path'].startswith('packages/vscode/out/')};after={p.relative_to(root/'packages/vscode/out').as_posix():{'bytes':p.stat().st_size,'sha256':sha(p)} for p in (root/'packages/vscode/out').rglob('*') if p.is_file()}
 result['editor_outputs']=[{'path':name,**row} for name,row in sorted(after.items())];result['editor_output_count']=len(after);result['editor_output_delta']=[{'path':name,'before':before.get(name),'after':after.get(name)} for name in sorted(set(before)|set(after)) if not before.get(name) or not after.get(name) or before[name]['sha256']!=after[name]['sha256']]
 result['admin_assets']=[{'path':p.name,'bytes':p.stat().st_size,'sha256':sha(p)} for p in sorted((root/'crates/airk-cli/admin-assets').iterdir()) if p.is_file()];assert len(result['admin_assets'])==5
 assert [x['path'] for x in result['editor_output_delta']]==['explorerWebview.js'],'Unexpected changed browser/host output inventory'
 result['passed']=True
except BaseException as error:result['failure']={'type':type(error).__name__,'message':str(error)};traceback.print_exc();raise
finally:(out/'result.json').write_text(json.dumps(result,indent=2)+chr(10),encoding='utf-8')
