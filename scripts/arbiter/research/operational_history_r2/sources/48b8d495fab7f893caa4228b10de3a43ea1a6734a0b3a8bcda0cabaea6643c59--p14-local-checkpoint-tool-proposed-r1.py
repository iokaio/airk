# SPDX-License-Identifier: Apache-2.0
"""Root-owned exact-manifest integration and local DCO checkpoint. No push or publication.
Requires a final acceptance manifest and explicit SHA supplied after all actual outcomes.
"""
import argparse,hashlib,importlib.util,json,os,re,subprocess
from pathlib import Path
ROOT=Path('D:/code/Github/airk');BASE=ROOT/'artifacts/v190';WORK=BASE/'p10-worktree'
HEAD='e2d65ed4c2d44ca9f00c91c4c1e9261ab04a1780'
NATIVE=None  # Filled only from Root's explicit actual qualified native identity arguments.
CODE=['crates/airk-cli/tests/combined_reads/split.rs', 'crates/airk-cli/tests/mcp.rs', 'crates/airk-kernel/src/retrieval_retry.rs', 'crates/airk-kernel/src/split_execution.rs', 'packages/vscode/src/test/installedEditor.ts', 'packages/vscode/src/test/installedEditorContracts.test.ts', 'packages/vscode/src/test/installedEditorContracts.ts', 'scripts/benchmark-scale.py', 'scripts/check-native-editor.cjs', 'scripts/check-runtime-upgrade.py', 'scripts/check-usage-inputs.py', 'scripts/editor-ui.cjs', 'scripts/tests/editor-ui.test.cjs', 'scripts/tests/test_runtime_upgrade.py', 'scripts/tests/test_scale_measurement.py', 'scripts/tests/test_usage_stable_publication.py', 'crates/airk-cli/src/runtime_lifecycle.rs', 'crates/airk-cli/src/runtime_install_tests.rs', 'crates/airk-cli/src/runtime_install_native_tests.rs']
DOCS=['README.md', 'docs/README.md', 'docs/releases/release-1.8.8.md', 'docs/releases/release-1.9.0.md', 'docs/research/implementation-v190.md', 'docs/research/plan-v190.md', 'docs/architecture.md', 'docs/index-splitting.md', 'packages/vscode/README.md', 'docs/vscode-extension.md', 'docs/mcp.md', 'docs/unified-mcp.md', 'docs/mcp-developer-guide.md', 'docs/service-retrieval.md', 'docs/index-self-healing.md']
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest() if Path(p).is_file() else None
read=lambda p:json.loads(Path(p).read_text(encoding='utf-8'))
def git(where,*args):
 env=os.environ.copy()
 # Fixture checks use an owned empty global file. Human commits must use the actual configured identity.
 env.pop('GIT_CONFIG_GLOBAL',None)
 for key in ['GIT_AUTHOR_NAME','GIT_AUTHOR_EMAIL','GIT_COMMITTER_NAME','GIT_COMMITTER_EMAIL']:env.pop(key,None)
 return subprocess.check_output(['git','-c','safe.directory='+where.as_posix(),'-c','core.autocrlf=true','-c','core.safecrlf=false','-C',str(where),*args],env=env,stderr=subprocess.PIPE)
def names(data):return set(data.decode().strip('\0').split('\0'))-{''}
def source(where):
 spec=importlib.util.spec_from_file_location('checkpoint_builder_'+where.name,where/'scripts/build-service-runtime.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m.source_identity()
def public(where):return names(git(where,'ls-files','--cached','--others','--exclude-standard','-z'))
def inventory(where):
 result={}
 for name in public(where):
  p=where/name;assert p.is_file() and not p.is_symlink(),name
  assert not name.startswith(('artifacts/','temp/','.git/','.airk/')),name
  result[name]=sha(p)
 return result
def clean(where):
 assert not git(where,'diff','--name-only') and not git(where,'diff','--cached','--name-only')
 assert not git(where,'ls-files','--others','--exclude-standard')
def write_new(path,value):
 with path.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2);f.write('\n')
def verify_acceptance(path,digest):
 assert re.fullmatch('[a-f0-9]{64}',digest) and sha(path)==digest
 a=read(path)
 assert a['phase']=='P14' and a['status']=='accepted_selected_local_qualification' and a['base']==HEAD and a['native_source']==NATIVE
 assert a['complete_diff_reviewed'] is True and a['documentation_consistency_verified'] is True
 assert a['corpus_campaign']=='failed_budget_preserved' and a['release_promotion'] is False
 assert a['deleted_source_followup']=='required_after_P14_checkpoint' and a['version']=='1.8.8'
 assert set(a['evidence'])=={'final_qualification','broader_supplement','neo','editor_outcomes','late_checks','final_docs','primary_review'}
 for kind,row in a['evidence'].items():
  p=Path(row['path']);assert p.resolve().is_relative_to(BASE.resolve()) and sha(p)==row['sha256'],kind
  v=read(p)
  for key,value in row['required_values'].items():assert v[key]==value,(kind,key)
  assert row['required_values'],kind
 rows=a['public_files'];assert len(rows)==34 and {r['path'] for r in rows}==set(CODE+DOCS)
 assert len(CODE)==19 and len(DOCS)==15
 assert all(re.fullmatch('[a-f0-9]{64}',r['tested_sha256']) and (r['base_sha256'] is None or re.fullmatch('[a-f0-9]{64}',r['base_sha256'])) for r in rows)
 assert (WORK/'AGENTS.md').read_bytes()==(WORK/'CLAUDE.md').read_bytes()
 assert all(sha(WORK/r['path'])==r['tested_sha256'] for r in rows)
 assert source(WORK)==NATIVE
 return a

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('action',choices=['integrate','commit']);p.add_argument('--native-source-sha256',required=True);p.add_argument('--native-source-files',type=int,required=True);p.add_argument('--acceptance',type=Path,required=True);p.add_argument('--acceptance-sha256',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--integration',type=Path);p.add_argument('--integration-sha256');args=p.parse_args()
 assert args.output.resolve().is_relative_to(BASE.resolve()) and not args.output.exists()
 global NATIVE
 assert re.fullmatch('[a-f0-9]{64}',args.native_source_sha256) and args.native_source_files==960
 NATIVE={'sha256':args.native_source_sha256,'files':args.native_source_files}
 a=verify_acceptance(args.acceptance,args.acceptance_sha256);rows=a['public_files'];paths=sorted(r['path'] for r in rows)
 assert git(ROOT,'rev-parse','HEAD').decode().strip()==HEAD
 assert git(WORK,'rev-parse','HEAD').decode().strip()==HEAD
 if args.action=='integrate':
  clean(ROOT)
  assert {r['path'] for r in rows if r['new']}==names(git(WORK,'ls-files','--others','--exclude-standard','-z'))
  assert names(git(WORK,'diff','--name-only','-z',HEAD))=={r['path'] for r in rows if not r['new']}
  before=inventory(ROOT);after=inventory(WORK)
  assert set(after)-set(before)=={r['path'] for r in rows if r['new']} and not(set(before)-set(after))
  # Exact byte checks avoid dropping native source-identity changes while ignoring unrelated historical EOL differences.
  assert all(sha(ROOT/r['path'])==r['base_sha256'] for r in rows)
  for r in rows:
   s,d=WORK/r['path'],ROOT/r['path'];assert s.is_file() and not s.is_symlink() and sha(s)==r['tested_sha256']
   assert d.resolve().is_relative_to(ROOT.resolve()) and not d.is_symlink()
   d.parent.mkdir(parents=True,exist_ok=True);d.write_bytes(s.read_bytes())
  assert source(ROOT)==NATIVE
  assert all(sha(ROOT/r['path'])==r['tested_sha256'] for r in rows)
  assert names(git(ROOT,'diff','--name-only','-z'))|names(git(ROOT,'ls-files','--others','--exclude-standard','-z'))==set(paths)
  git(ROOT,'diff','--check')
  write_new(args.output,{'status':'applied_not_committed','phase':'P14','base':HEAD,'native_source':NATIVE,'acceptance_sha256':args.acceptance_sha256,'public_files':rows,'private_state_copied':False,'deletions':False})
 else:
  assert args.integration and args.integration_sha256 and sha(args.integration)==args.integration_sha256
  integrated=read(args.integration);assert integrated['status']=='applied_not_committed' and integrated['acceptance_sha256']==args.acceptance_sha256 and integrated['public_files']==rows
  assert not git(ROOT,'diff','--cached','--name-only')
  assert names(git(ROOT,'diff','--name-only','-z'))|names(git(ROOT,'ls-files','--others','--exclude-standard','-z'))==set(paths)
  assert all(sha(ROOT/r['path'])==r['tested_sha256'] for r in rows) and source(ROOT)==NATIVE
  assert (ROOT/'AGENTS.md').read_bytes()==(ROOT/'CLAUDE.md').read_bytes()
  name=git(ROOT,'config','user.name').decode().strip();email=git(ROOT,'config','user.email').decode().strip()
  assert name=='Tyler Jensen' and email=='tyler@tsjensen.com','Authorized configured human identity required'
  git(ROOT,'add','--',*paths);assert names(git(ROOT,'diff','--cached','--name-only','-z'))==set(paths)
  git(ROOT,'diff','--cached','--check');before=inventory(WORK)
  print(git(ROOT,'commit','-s','-m','Complete selected local P14 qualification and shared read recovery checks').decode())
  commit=git(ROOT,'rev-parse','HEAD').decode().strip();message=git(ROOT,'log','-1','--format=%B').decode()
  assert 'Signed-off-by: '+name+' <'+email+'>' in message.splitlines()
  clean(ROOT)
  assert names(git(ROOT,'diff-tree','--no-commit-id','--name-only','-r','-z',commit))==set(paths)
  git(WORK,'reset','--mixed','--quiet',commit)
  assert before==inventory(WORK),'Cache mixed reanchor changed public file bytes'
  git(WORK,'diff','--quiet');assert not git(WORK,'ls-files','--others','--exclude-standard')
  assert source(WORK)==source(ROOT)==NATIVE
  write_new(args.output,{'status':'accepted','accepted':True,'phase':'P14','commit':commit,'base':HEAD,'native_source':NATIVE,'acceptance_sha256':args.acceptance_sha256,'integration_sha256':args.integration_sha256,'dco_verified':True,'root_clean':True,'all_work_public_bytes_preserved':True,'all_public_bytes_preserved':True,'target_cleaned':False,'release_promotion':False})
 print(json.dumps({'action':args.action,'output':str(args.output),'sha256':sha(args.output)}))
if __name__=='__main__':main()
