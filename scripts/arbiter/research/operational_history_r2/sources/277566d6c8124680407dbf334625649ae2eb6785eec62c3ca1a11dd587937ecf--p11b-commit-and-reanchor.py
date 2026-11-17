# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,subprocess
from pathlib import Path
root=Path('D:/code/Github/airk');base=root/'artifacts/v190';work=base/'p10-worktree'
manifest=json.loads((base/'p11b-integration-manifest-r1.json').read_text())
def git(where,*args):
    return subprocess.check_output(['git','-c','safe.directory='+where.as_posix(),'-c','core.autocrlf=true',
        '-c','core.safecrlf=false','-C',str(where),*args],stderr=subprocess.PIPE)
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def names(data):return set(data.decode().strip('\0').split('\0'))-{''}
assert git(root,'rev-parse','HEAD').decode().strip()==manifest['base']
assert not git(root,'diff','--cached','--name-only')
paths=[r['path'] for r in manifest['files']]
assert all(sha(root/r['path'])==r['tested']==sha(work/r['path']) for r in manifest['files'])
assert names(git(root,'diff','--name-only','-z'))|names(git(root,'ls-files','--others','--exclude-standard','-z'))==set(paths)
assert (root/'AGENTS.md').read_bytes()==(root/'CLAUDE.md').read_bytes()
neo=json.loads((base/'p11b-neo-combined-verification-r1.json').read_text())
assert neo['status']=='passed' and neo['passing_tests']==11
assert all(r['cleanup']['cargo_clean_exit_code']==0 and r['cleanup']['target_removed'] for r in neo['batches'])
assert json.loads((base/'p11b-final-qualification-r1.json').read_text())['status']=='passed'
git(root,'add','--',*paths)
assert names(git(root,'diff','--cached','--name-only','-z'))==set(paths)
git(root,'diff','--cached','--check')
print(git(root,'commit','-s','-m','Add reviewed file-aware Lean and Rich retained-profile migration').decode())
commit=git(root,'rev-parse','HEAD').decode().strip()
name=git(root,'config','user.name').decode().strip();email=git(root,'config','user.email').decode().strip()
assert f'Signed-off-by: {name} <{email}>' in git(root,'log','-1','--format=%B').decode()
assert not git(root,'status','--porcelain')
def inventory():return {p:sha(work/p) for p in names(git(work,'ls-files','--cached','--others','--exclude-standard','-z'))}
before=inventory();assert git(work,'branch','--show-current').decode().strip()!='feat/finetuning'
git(work,'reset','--mixed','--quiet',commit)
assert before==inventory();git(work,'diff','--quiet')
spec=importlib.util.spec_from_file_location('builder',work/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
assert builder.source_identity()==manifest['source']
record={'commit':commit,'files':len(paths),'phase_accepted':True,'source':manifest['source'],
    'compiled_runtime_source':manifest['compiled_runtime_source'],'mac_functional_source':manifest['mac_functional_source'],
    'dco_verified':True,'root_clean':True,'worktree_reanchored':True,'all_public_bytes_preserved':True,'public_files':len(before),
    'checks':'Primary combined diff/contracts/evidence review; independent implementation/correction/doc reviews;52 focused native plus2 current resource cases;11 release/fault/performance/repository gates with84unchanged limits;UI746/19;Mac11/two cleaned targets;root repo/version/whitespace.',
    'limits':'Scoped phase acceptance only; P14 still restores cumulative workspace,4routine campaigns,doctests and installed/platform matrix. No broad/cross-platform publication claim.',
    'remaining':'P12 through P14, deleted-source follow-up, local test installer, PR60 assessment; no push/publication.'}
output=base/'p11b-local-acceptance-commit-r1.json';assert not output.exists()
output.write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record))
