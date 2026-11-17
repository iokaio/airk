# SPDX-License-Identifier: Apache-2.0
"""Root-only six-layer adoption; requires a real accepted P14 checkpoint first."""
import argparse
import difflib
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import subprocess

ROOT=Path('D:/code/Github/airk')
BASE=ROOT/'artifacts/v190'
WORK=BASE/'p10-worktree'
MAP=BASE/'post-p14-recovery-composition-order-r4.json'
CATALOG=BASE/'post-p14-first-pass-qualification-r4/command-catalog.json'
MAP_SHA='35a9ea25a8c7125061b7b30883e75a631d830ae57695a769df53a3426eebbd69'
CATALOG_SHA='46ade342d47d078fff4e640c9c8cd637e4cce29b9ced1c7cb50331ed0bd18317'
MANIFESTS={
 'normal15':'b53e4d8698c56970432ec114551a462f517f0c3ed232909905535c2ad428070f',
 'union2':'6a0a2c1b6bc9bb2b4c3d2ad19d5efe94658b3213211db17bda9b6e677894728e',
 'core_store2':'44921c37972ff296c17c6bddba76d40a567090e32f41275f4bf6f0420c1f4c85',
 'adapter4':'a3adefe518165ebd1351bfeed491a94732e89958c419af8261aed85a5023784c',
 'actual187_harness3':'200d3aa947d72a606bba0d197b51b16bc01575453c0d7035132fc547d0c09220',
 'retirement_references2':'6028681205e4576c018b69edbfa48762d51f67e5564a0ce469a7361d3b951113'}

def digest(data):return hashlib.sha256(data).hexdigest()
def sha(p):return digest(p.read_bytes()) if p.exists() else None
def git(where,*args):
 return subprocess.check_output(['git','-c','safe.directory='+where.as_posix(),'-c','core.autocrlf=true','-c','core.safecrlf=false','-C',str(where),*args],stderr=subprocess.PIPE)
def public_inventory(where):
 names=set(git(where,'ls-files','--cached','--others','--exclude-standard','-z').decode().split('\0'))-{''}
 assert all(not (where/n).is_symlink() and (where/n).is_file() for n in names)
 return {n:sha(where/n) for n in sorted(names)}
def source_identity():
 spec=importlib.util.spec_from_file_location('builder',WORK/'scripts/build-service-runtime.py')
 builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
 return builder.source_identity()
def checked_json(path,expected):
 path=Path(path).resolve(strict=True)
 assert path.is_relative_to(BASE.resolve()) and sha(path)==expected
 return path,json.loads(path.read_text(encoding='utf-8-sig'))

def main():
 parser=argparse.ArgumentParser()
 parser.add_argument('--p14-checkpoint',required=True)
 parser.add_argument('--p14-checkpoint-sha256',required=True)
 parser.add_argument('--p14-commit',required=True)
 parser.add_argument('--accepted-native-sha256',required=True)
 parser.add_argument('--accepted-native-files',required=True,type=int)
 parser.add_argument('--independent-review',required=True)
 parser.add_argument('--independent-review-sha256',required=True)
 args=parser.parse_args()
 checkpoint_path,checkpoint=checked_json(args.p14_checkpoint,args.p14_checkpoint_sha256)
 review_path,review=checked_json(args.independent_review,args.independent_review_sha256)
 assert checkpoint['phase']=='P14' and checkpoint['accepted'] is True
 assert all(checkpoint[k] is True for k in ['dco_verified','root_clean','all_public_bytes_preserved'])
 assert checkpoint['commit']==args.p14_commit and len(args.p14_commit)==40
 assert review['approved'] is True and review['application_tool_reviewed'] is True
 assert review['application_tool_sha256']==sha(Path(__file__))
 assert review['composition_map_sha256']==MAP_SHA and review['qualification_catalog_sha256']==CATALOG_SHA
 assert sha(MAP)==MAP_SHA and sha(CATALOG)==CATALOG_SHA
 for where in [ROOT,WORK]:
  assert git(where,'rev-parse','HEAD').decode().strip()==args.p14_commit
  assert not git(where,'diff','--name-only'),str(where)+' has semantic working changes'
  assert not git(where,'diff','--cached','--name-only'),str(where)+' has staged changes'
  assert not git(where,'ls-files','--others','--exclude-standard'),str(where)+' has untracked public files'
 message=git(ROOT,'log','-1','--format=%B').decode()
 human=git(ROOT,'config','user.name').decode().strip();email=git(ROOT,'config','user.email').decode().strip()
 assert f'Signed-off-by: {human} <{email}>' in message
 initial=public_inventory(WORK)
 root_initial=public_inventory(ROOT)
 eol_path='packages/vscode/src/explorerHelp/indexing-library.md'
 differences={n for n in set(root_initial)|set(initial) if root_initial.get(n)!=initial.get(n)}
 assert differences=={eol_path}
 assert root_initial[eol_path]=='8a604e75f4dfcdef8ad33955dd64cbfef5dae962dedbe04a49177844b0d1ccc3'
 assert initial[eol_path]=='9151a36dd4b2b41393be997db43868ed2df06d0435d79a5a16e341871755f23b'
 assert (ROOT/eol_path).read_bytes().replace(b'\r\n',b'\n')==(WORK/eol_path).read_bytes().replace(b'\r\n',b'\n')
 eol_baseline={'path':eol_path,'root_sha256':root_initial[eol_path],'work_sha256':initial[eol_path],'normalized_lf_equal':True,'preserved':True}
 before_native=source_identity()
 assert before_native==checkpoint['native_source']=={'sha256':args.accepted_native_sha256,'files':args.accepted_native_files}
 assert len(args.accepted_native_sha256)==64 and all(c in '0123456789abcdef' for c in args.accepted_native_sha256)
 assert args.accepted_native_files==960
 admission_paths={'crates/airk-cli/src/runtime_lifecycle.rs','crates/airk-cli/src/runtime_install_tests.rs','crates/airk-cli/src/runtime_install_native_tests.rs'}
 accepted_admission={n:initial[n] for n in sorted(admission_paths)}
 composition=json.loads(MAP.read_text())
 assert [l['name'] for l in composition['layers']]==list(MANIFESTS)
 virtual={};original={};steps=[]
 for layer in composition['layers']:
  patch=Path(layer['patch']).resolve(strict=True);directory=patch.parent
  assert directory.is_relative_to(BASE.resolve()) and sha(patch)==layer['patch_sha256']
  manifest=directory/'manifest.json';assert sha(manifest)==MANIFESTS[layer['name']]
  raw=json.loads(manifest.read_text());files=raw.get('files',raw)
  indexed={r['path']:r for r in files} if isinstance(files,list) else files
  assert set(indexed)==set(layer['files'])
  for name,row in layer['files'].items():
   path=PurePosixPath(name)
   assert not path.is_absolute() and '..' not in path.parts and path.parts[0] in {'crates','packages','scripts'}
   declared=indexed[name]
   declared_base=declared.get('base_sha256',declared.get('current_p14_base_sha256'))
   assert declared_base==row['base_sha256'] and declared['proposed_sha256']==row['proposed_sha256']
   if name not in original:original[name]=(WORK/name).read_bytes() if (WORK/name).exists() else None
   current=virtual.get(name,original[name])
   assert (digest(current) if current is not None else None)==row['base_sha256'],layer['name']+':'+name
   assert (row['base_kind']=='previous_layer')==(name in virtual)
   source=directory/'source'/name
   assert source.is_file() and not source.is_symlink() and sha(source)==row['proposed_sha256']
   virtual[name]=source.read_bytes()
  steps.append({'name':layer['name'],'manifest_sha256':sha(manifest),'patch_sha256':sha(patch),'files':layer['files']})
 assert not admission_paths.intersection(virtual), 'Frozen recovery layers must not overwrite the accepted lifecycle admission fix'
 # Remove exactly one obsolete inert-only test-module header, after every frozen layer.
 target='crates/airk-cli/tests/combined_reads/deleted_source_recovery.rs'
 frozen_final=virtual[target]
 line=b'//! Post-P14 acceptance regressions. Prepared inertly; not compiled or executed.'
 lines=frozen_final.splitlines(keepends=True)
 assert sum(x.rstrip(b'\r\n')==line for x in lines)==1
 virtual[target]=b''.join(x for x in lines if x.rstrip(b'\r\n')!=line)
 comment_delta={'path':target,'before_sha256':digest(frozen_final),'after_sha256':digest(virtual[target]),'removed_exact_line':line.decode(),'behavior_change':False}
 final={n:digest(data) for n,data in virtual.items()}
 patch=''
 for name in sorted(virtual):
  old=original[name]
  patch+=''.join(difflib.unified_diff((old or b'').decode().replace('\r\n','\n').splitlines(True),virtual[name].decode().replace('\r\n','\n').splitlines(True),fromfile='a/'+name if old is not None else '/dev/null',tofile='b/'+name))
 output=BASE/'post-p14-six-layer-live-application-r4'
 output.mkdir(exist_ok=False)
 (output/'integrated-review.patch').write_bytes(patch.encode())
 state={'status':'PREFLIGHT_VERIFIED_APPLY_STARTED_NOT_QUALIFIED','checkpoint':{'path':str(checkpoint_path),'sha256':sha(checkpoint_path),'commit':args.p14_commit},'independent_review':{'path':str(review_path),'sha256':sha(review_path)},'composition_map_sha256':sha(MAP),'qualification_catalog_sha256':sha(CATALOG),'native_before':before_native,'accepted_lifecycle_admission_files':accepted_admission,'root_public_before':root_initial,'historical_eol_baseline':eol_baseline,'public_before':initial,'layers':steps,'inert_header_removal':comment_delta,'expected_final_affected':final,'integrated_diff_sha256':digest(patch.encode()),'qualification':'31 new Rust Windows/30 portable; 37 Windows focused/32 Mac selected per pinned catalog; not executed'}
 receipt=output/'receipt.json'
 receipt.write_text(json.dumps(state,indent=2)+'\n',encoding='utf-8')
 for layer in composition['layers']:
  directory=Path(layer['patch']).parent
  for name,row in layer['files'].items():
   destination=WORK/name
   assert sha(destination)==row['base_sha256']
   data=(directory/'source'/name).read_bytes()
   assert digest(data)==row['proposed_sha256']
   destination.parent.mkdir(parents=True,exist_ok=True)
   destination.write_bytes(data)
   assert sha(destination)==row['proposed_sha256']
 assert sha(WORK/target)==comment_delta['before_sha256']
 (WORK/target).write_bytes(virtual[target])
 assert all(sha(WORK/n)==value for n,value in final.items())
 after=public_inventory(WORK)
 assert set(after)==set(initial)|set(final)
 assert all(after[n]==value for n,value in initial.items() if n not in final)
 assert public_inventory(ROOT)==root_initial and not git(ROOT,'status','--porcelain')
 observed_names=set(git(WORK,'diff','--name-only','-z').decode().split('\0'))-{''}
 observed_names|=set(git(WORK,'ls-files','--others','--exclude-standard','-z').decode().split('\0'))-{''}
 assert observed_names==set(final)
 state.update(status='SIX_FROZEN_LAYERS_APPLIED_WITH_EXACT_HEADER_REMOVAL_QUALIFICATION_PENDING',native_after=source_identity(),public_after=after,affected_count=len(final),root_unchanged=True)
 receipt.write_text(json.dumps(state,indent=2)+'\n',encoding='utf-8')
 print(json.dumps({'receipt':str(receipt),'sha256':sha(receipt),'status':state['status'],'native_after':state['native_after'],'affected_count':len(final)}))

if __name__=='__main__':main()
