# SPDX-License-Identifier: Apache-2.0
import pathlib,json,hashlib,subprocess,ast,shutil,datetime
root=pathlib.Path.cwd();base=pathlib.Path(r'D:/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z');out=base/'ui-native-r3-cards';runtime=pathlib.Path(r'D:/code/Github/airk/artifacts/v190/final-plan/candidate-worktree-r3/artifacts/windows-signed-native-r3/runtime')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()=='c547e6547b1312c2e113bd1ddc9ab84969c229c0';assert subprocess.check_output(['git','status','--porcelain'],text=True)==''
source=subprocess.check_output(['git','ls-files','-z']).decode('utf-8').split('\0')[:-1];assert len(source)==3508
outputs=list(p for p in (root/'packages/vscode/out').rglob('*') if p.is_file());assert len(outputs)==223
files={str(root/n):sha(root/n) for n in source};files.update({str(p):sha(p) for p in outputs});files.update({str(p):sha(p) for p in runtime.rglob('*') if p.is_file()})
for p in [pathlib.Path(r'D:/code/Github/airk/artifacts/tools/node-v24.15.0-win-x64/node.exe'),pathlib.Path(r'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'),base/'windows-runtime-debug-r3-build.json',base/'ui-title-build-r1/admin-assets.json']:files[str(p)]=sha(p)
expected={'airk.exe':'5fbadf9489ab385155d60a05bf89beff682871457f0528624dcb241036f74282','airkctl.exe':'5a11763598890f9fecba1a9899d4e02704da4da56f20c9f248da680d3308f98f','airk-build.json':'e47146c0b46acd8cb4147912e43c8cb0890c7d6f3e9f12ddb46027e1166ec0e8'}
for n,h in expected.items():assert sha(runtime/n)==h
build=json.loads((runtime/'airk-build.json').read_bytes());assert build['source']=={'sha256':'a8d7983b0f8bfdb701c7659f3b7a424505359a0dbe52446eb10c31de0b1a3dfe','files':1941}
assert build['admin_assets']['bundle_sha256']=='8e65febbf798774d0adbfe9ce8c757c7615390b9cd14fba8fa7cc1d987a4f7d1'
assert build['admin_assets']['inventory_sha256']=='b21f516d5fe8313e6df13249a1874200e078a4f7f80093b28cbde0401a9985c8'
shipping_manifest=pathlib.Path(r'D:/code/Github/airk/artifacts/arbiter-1.8.207/windows-candidate-r3/native/publish/manifest.json')
shipping=json.loads(shipping_manifest.read_bytes())
assert sha(shipping_manifest)=='9e1d4015a76e1f0627d2dfb41915e8dc0d97eba1cf6968a557c783c496a3f7d7'
assert shipping['commit']=='c547e6547b1312c2e113bd1ddc9ab84969c229c0' and shipping['source']==build['source']
assert {p.relative_to(runtime).as_posix():sha(p) for p in runtime.rglob('*') if p.is_file()}==shipping['payload']
for p in [shipping_manifest,runtime.parent/'signing.json',base/'windows-packaging-candidate-r3-plan/adoption/execution.json',pathlib.Path(r'D:/code/Github/airk/artifacts/arbiter-1.8.207/windows-candidate-r3/publish/manifest.json')]:files[str(p)]=sha(p)
binding={'schema':'airk.private.native-ui-frozen-binding/1','candidate':str(root),'commit':'c547e6547b1312c2e113bd1ddc9ab84969c229c0','tree':'b83e6757e1314ebd1109d37bac9cee8a33fb7ac4','source_C7':build['source'],'runtime':str(runtime),'files':files,'source_count':3508,'compiled_count':223,'native_builder_receipt_sha256':sha(base/'windows-runtime-debug-r3-build.json'),'shipped_vsix_sha256':'5053a93364b123b87bee006df7314d4abc3e03b3c2ea8a77d7f33885b46c1c56','actual_execution':False}
(out/'bindings.json').write_text(json.dumps(binding,indent=2)+'\n',encoding='utf-8')
guard='''import pathlib,json,hashlib,subprocess
BINDING=pathlib.Path(__file__).resolve().parent/'bindings.json'
def verify():
 d=json.loads(BINDING.read_bytes());root=pathlib.Path.cwd()
 assert root.samefile(d['candidate']),'Candidate cwd differs'
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True,encoding='utf-8').strip()==d['commit'],'Candidate HEAD drift'
 assert subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True,encoding='utf-8').strip()==d['tree'],'Candidate tree drift'
 assert subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True,encoding='utf-8')=='','Candidate tracked status changed'
 for name,want in d['files'].items():
  p=pathlib.Path(name)
  assert p.is_file() and hashlib.sha256(p.read_bytes()).hexdigest()==want,'Frozen input differs: '+name
 return {'binding_sha256':hashlib.sha256(BINDING.read_bytes()).hexdigest(),'files_verified':len(d['files']),'source_C7':d['source_C7']}
def settle(result):
 try:result['binding_post']=verify();return True
 except BaseException as error:result.update(passed=False,binding_failure={'type':type(error).__name__,'message':str(error)});return False
'''
admin=out/'admin-r4';work=out/'workbench-r3';admin.mkdir();work.mkdir()
for p in (admin,work):
 (p/'bindings.json').write_bytes((out/'bindings.json').read_bytes());(p/'frozen_binding.py').write_text(guard,encoding='utf-8')
# Preserve all current Admin assertions, including exact12titles, full manual/clipboard/resource details and request terminal counts.
for n in ['admin-live.mjs','browser-harness.mjs']:shutil.copyfile(base/'ui-live-admin-r3'/n,admin/n)
a=(base/'ui-live-admin-r3/run.py').read_text(encoding='utf-8');a=a.replace('root=pathlib.Path.cwd();out=pathlib.Path(__file__).resolve().parent','root=pathlib.Path.cwd();out=pathlib.Path(__file__).resolve().parent\nfrom frozen_binding import verify,settle\nbinding_before=verify()')
a=a.replace("text=True,timeout=240","text=True,encoding='utf-8',timeout=240")
a=a.replace("result.update(elapsed_seconds=time.monotonic()-started,native_processes=witness);", "binding_ok=settle(result);result.update(binding_before=binding_before,elapsed_seconds=time.monotonic()-started,native_processes=witness);")
a+='\nassert binding_ok, "Frozen Admin inputs changed; execution receipt remains nonpassing"\n'
w=(base/'ui-live-workbench-r1/run.py').read_text(encoding='utf-8');w=w.replace('root=pathlib.Path.cwd();out=pathlib.Path(__file__).resolve().parent','root=pathlib.Path.cwd();out=pathlib.Path(__file__).resolve().parent\nfrom frozen_binding import verify,settle\nbinding_before=verify()\nassert not (out/"execution.json").exists(), "Fresh run required"')
w=w.replace('D:/code/Github/airk/artifacts/arbiter-1.8.207/windows-runtime-debug-r1',str(runtime).replace('\\','/'))
w=w.replace("result.update(elapsed_seconds=time.monotonic()-started,native_processes=witness);", "binding_ok=settle(result);result.update(binding_before=binding_before,elapsed_seconds=time.monotonic()-started,native_processes=witness);")
w+='\nassert binding_ok, "Frozen Workbench inputs changed; execution receipt remains nonpassing"\n'
for p,s in [(admin/'run.py',a),(work/'run.py',w)]:ast.parse(s);p.write_text(s,encoding='utf-8')
# Preparation parses Python only; no Node/browser/native launch or test execution.
ast.parse(guard)
cards={'schema':'airk.private.native-ui-r3-cards/1','status':'prepared-not-executed-awaiting-root-review-signed-R3','candidate':str(root),'runtime':str(runtime),'binding_sha256':sha(out/'bindings.json'),'private_inputs':{p.relative_to(out).as_posix():sha(p) for p in out.rglob('*') if p.is_file()},'environment':{'AIRK_PRIVATE_CENSUS_BASE':r'C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/temp','PYTHONDONTWRITEBYTECODE':'1'},'admin':{'argv':[r'C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe','-B',str(admin/'run.py'),'--runtime-dir',str(runtime),'--expected-head',binding['commit']],'scope':'Actual embedded assets and I3 in private Edge; all24navigation/12titles/12icons/wide+narrow/fullnativeResources+status+3manualWindowsclipboard comparisons; exact issued/completed/failed/aborted/missing-terminal bucket counts, no count target; original native Stop and retained kernel process handle exit proof; clipboard restored in memory only if still owned.'},'workbench':{'argv':[r'C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe','-B',str(work/'run.py')],'scope':'Actual native I2+compiled production Workbench/WindowScope/adapter/session callbacks, injected VSCode host (not installed/DOM). Full-result comparisons, both Answer routes/source opens, source readonly, native Stop and original kernel handle exit proof. Compact actualDOM separately passed65 on same bytes.'},'signed_shipping_bytes':{'native_manifest_sha256':sha(shipping_manifest),'runtime_payload_files':len(shipping['payload']),'current_VSIX_sha256':'5053a93364b123b87bee006df7314d4abc3e03b3c2ea8a77d7f33885b46c1c56'},'unchanged_prior_evidence':'Historical r1/r2 receipts remain under original identities. No optional model or installed-native Workbench qualification claimed.'}
(out/'cards.json').write_text(json.dumps(cards,indent=2)+'\n',encoding='utf-8');print(json.dumps({'cards':str(out/'cards.json'),'sha256':sha(out/'cards.json'),'binding_sha256':sha(out/'bindings.json'),'frozen_files':len(files),'executed':False},indent=2))
