# SPDX-License-Identifier: Apache-2.0
"""Future root-owned single-test-driver emit with exact product/VSIX preservation proof."""
import argparse,hashlib,importlib.util,json,re,subprocess,sys,zipfile
from pathlib import Path
A=Path(__file__).parent;W=A/'p10-worktree';PKG=W/'packages/vscode'
EXPECTED={'sha256':'5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11','files':960}
h=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
read=lambda p:json.loads(Path(p).read_text(encoding='utf-8'))
TARGETS={'packages/vscode/src/test/'+name for name in ['installedEditor.ts','installedEditorContracts.ts','installedEditorContracts.test.ts']}
def load(name,p):
 spec=importlib.util.spec_from_file_location(name,p);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
def census(root,skip_test=False):
 result={}
 for p in root.rglob('*'):
  if p.is_file() and not(skip_test and p.relative_to(root).parts[0]=='test'):
   assert not p.is_symlink();result[p.relative_to(root).as_posix()]=h(p)
 return result
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--label',required=True);p.add_argument('--proposal-manifest',type=Path,required=True);p.add_argument('--proposal-manifest-sha256',required=True);args=p.parse_args()
 assert re.fullmatch('p14-installed-editor-[a-z0-9-]+',args.label)
 assert h(args.proposal_manifest)==args.proposal_manifest_sha256
 proposal=read(args.proposal_manifest);changed={row['path']:row for row in proposal['files'] if row['path'].startswith('packages/vscode/')}
 assert set(changed)==TARGETS
 assert all(h(W/name)==row['proposed_sha256'] for name,row in changed.items())
 prior=load('p14_original_editor_inputs',A/'p14-package-editor-owner-r3.py')
 original=read(A/'p14-package-editor-owner-r3-inputs.json');current=prior.extension_inputs()
 delta={name for name in set(original)|set(current) if original.get(name)!=current.get(name)}
 assert delta==TARGETS,'Only three external test inputs may differ from original qualified UI source'
 builder=load('p14_external_driver_builder',W/'scripts/build-service-runtime.py');assert builder.source_identity()==EXPECTED
 vsix=Path(read(A/'p14-package-editor-owner-r2.json')['vsix']['path'])
 assert h(vsix)=='49170b51765048e438dfd9516a35e4884d7667efe5ad2652d02303caf22f4c90'
 product_before=census(PKG/'out',True);source_before=census(PKG/'src',True)
 with zipfile.ZipFile(vsix) as zipped:
  names=zipped.namelist();assert not any(name.startswith('extension/out/test/') for name in names)
  packaged={name[len('extension/out/'):]:hashlib.sha256(zipped.read(name)).hexdigest() for name in names if name.startswith('extension/out/') and not name.endswith('/')}
 assert packaged and all(product_before.get(name)==value for name,value in packaged.items()),'Current product output must equal exact existing VSIX'
 prefix=A/args.label;record=Path(str(prefix)+'.json');emit=Path(str(prefix)+'-emit.json');log=Path(str(prefix)+'.log');before_path=Path(str(prefix)+'-before.json')
 assert not any(p.exists() for p in [record,emit,log,before_path])
 before={'source':EXPECTED,'ui_delta':sorted(delta),'production_sources':source_before,'production_outputs':product_before,'packaged_outputs':packaged,'vsix_sha256':h(vsix),'proposal_manifest_sha256':args.proposal_manifest_sha256}
 with before_path.open('x',encoding='utf-8') as output:json.dump(before,output,indent=2);output.write('\n')
 code=1;error=None
 try:
  with log.open('x',encoding='utf-8') as output:
   code=subprocess.run(['node',str(A/'p14-installed-editor-target-compile-r2.cjs'),str(W),str(emit)],cwd=W,stdout=output,stderr=subprocess.STDOUT,timeout=180).returncode
  assert code==0 and read(emit)['typecheck']=='passed'
  assert builder.source_identity()==EXPECTED
  assert census(PKG/'src',True)==source_before and census(PKG/'out',True)==product_before
  assert prior.extension_inputs()==current and h(vsix)==before['vsix_sha256']
 except BaseException as failure:error=repr(failure);raise
 finally:
  result={'status':'passed' if code==0 and error is None else 'failed','actual_exit_code':code,'error':error,
   'native_source':EXPECTED,'production_source_unchanged':census(PKG/'src',True)==source_before,'production_out_unchanged':census(PKG/'out',True)==product_before,
   'vsix_sha256':h(vsix),'vsix_contains_test_outputs':False,'ui_input_hashes':current,'external_test_delta':sorted(delta),
   'emission':read(emit) if emit.exists() else None,'log_sha256':h(log) if log.exists() else None,
   'limits':'External test code only. No native/npm/package/VSIX rebuild or actual installed-editor acceptance inferred.'}
  with record.open('x',encoding='utf-8') as output:json.dump(result,output,indent=2);output.write('\n')
if __name__=='__main__':main()
