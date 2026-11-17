# SPDX-License-Identifier: Apache-2.0
import pathlib,sys,json,hashlib,importlib.util,subprocess,time,traceback
root=pathlib.Path.cwd();out=pathlib.Path(__file__).resolve().parent
node=pathlib.Path('D:/code/Github/airk/artifacts/tools/node-v24.15.0-win-x64/node.exe');editor=pathlib.Path('C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe');app=editor.parent/'69047191fc/resources/app';cli=app/'out/cli.js'
publish=root/'artifacts/windows-milestone-vsix-r1/publish';manifest=json.loads((publish/'manifest.json').read_bytes());vsix=publish/manifest['archive']
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert json.loads((app/'package.json').read_bytes())['version']=='1.146.0'
assert json.loads((app/'product.json').read_bytes())['commit']=='69047191fc427f2f92828279aae1d90d8eeda7ab'
assert subprocess.check_output([str(node),'--version'],text=True).strip()=='v24.15.0'
assert sha(vsix)==manifest['archive_sha256']
assert manifest['commit']==subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
spec=importlib.util.spec_from_file_location('actual_vsix_owner',root/'scripts/package-extension.py');package=importlib.util.module_from_spec(spec);spec.loader.exec_module(package)
package.receipts.validate_vsix(manifest)
package.verify_vsix(vsix,manifest['payload'],version=manifest['version'],pre_release=manifest['pre_release'],display_name=manifest['display_name'])
argv=[str(node),str(root/'scripts/check-installed-explorer.cjs'),str(editor),str(cli),str(vsix)]
paths=[node,editor,cli,app/'package.json',app/'product.json',vsix,publish/'manifest.json',root/'scripts/check-installed-explorer.cjs',root/'scripts/editor-ui.cjs']
result={'schema':'airk.private.installed-explorer-execution/1','argv':argv,'cwd':str(root),'inputs':{str(p):{'sha256':sha(p),'bytes':p.stat().st_size} for p in paths},'source_commit':manifest['commit'],'vsix_verified':True,'editor_version':'1.146.0','editor_commit':'69047191fc427f2f92828279aae1d90d8eeda7ab','native_execution':False,'auto_install':False,'passed':False}
start=time.monotonic()
try:
 with (out/'run.log').open('x',encoding='utf-8') as log:
  child=subprocess.run(argv,cwd=root,stdout=log,stderr=subprocess.STDOUT,check=False)
 result['exit_code']=child.returncode;result['passed']=child.returncode==0
 for path,pin in result['inputs'].items():assert sha(pathlib.Path(path))==pin['sha256'],'input drifted: '+path
 if child.returncode:raise RuntimeError('Actual private Explorer check failed; see run.log')
except BaseException as error:
 result['failure']={'type':type(error).__name__,'message':str(error)};traceback.print_exc();raise
finally:
 result['elapsed_seconds']=time.monotonic()-start;(out/'execution.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'passed':result['passed'],'elapsed_seconds':result['elapsed_seconds'],'evidence':str(out)},ensure_ascii=True))