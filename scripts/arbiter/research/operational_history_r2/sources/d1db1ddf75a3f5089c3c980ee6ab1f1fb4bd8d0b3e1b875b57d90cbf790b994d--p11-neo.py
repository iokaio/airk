# SPDX-License-Identifier: Apache-2.0
"""Prepare/verify P11a's bounded Neo batch; launch/collect require explicit action."""
import ast, hashlib, importlib.util, json, os, subprocess, sys, tarfile
from pathlib import Path
B=Path(__file__).resolve().parent
ROOT=B/'p10-worktree'
LABEL='p11-native-20261106-r1'
EXPECTED={'sha256':'33e6df135da90839f409dfbba6dd2ce338af31675875e99f20d0b94af212f3f7','files':948}
def read(p): return json.loads(p.read_text(encoding='utf-8'))
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def save(p,v):
 data=(json.dumps(v,indent=2)+'\n').encode()
 with p.open('xb') as f: f.write(data)
def identity():
 spec=importlib.util.spec_from_file_location('p11_builder',ROOT/'scripts/build-service-runtime.py')
 module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
 return module.source_identity()
def local_proofs():
 proofs=[]
 for stem,count in [('p11a-core-families-r1',3),('p11a-ingest-producer-r1',1),('p11a-kernel-families-r1',2),('p11a-kernel-scopes-r1',10),('p11a-mcp-display-r1',1),('p11a-cli-info-r1',1),('p11a-scoped-clippy-r1',0)]:
  receipt=B/(stem+'.receipt.json');log=B/(stem+'.log');r=read(receipt)
  assert r['source_before']==r['source_after']==EXPECTED and r['source_unchanged']
  assert r['actual_exit_code']==r['exit_code']==0 and r['passing_tests']==count
  assert sha(log)==r['log_sha256']
  proofs.append({'receipt':receipt.name,'sha256':sha(receipt),'log':log.name,'log_sha256':sha(log),'passing_tests':count})
 return proofs
def plan():
 old=read(B/'p10-native-20261106-r4-plan.json')
 value={k:old[k] for k in ['working_version','runner_arguments','environment']}
 value.update(phase='P11a',draft=False,pending=[],native_source=EXPECTED,steps=[{'name':'fmt','command':['cargo','fmt','--all','--check']}],not_run=[
  'Targeted arm64 macOS source tests only; no full Mac workspace, runtime pair, actual CLI/MCP process parity, performance, retained-profile migration, semantic quality, OCR/model qualification, editor, signing, Linux or Intel qualification.',
  'Windows native focused 18-test proof and full UI proof are separately retained; do not relabel them as Mac observations.'])
 value['environment'].update(CARGO_BUILD_JOBS='1',CARGO_NET_OFFLINE='true',CARGO_INCREMENTAL='0',TMPDIR='Canonical existing macOS temporary directory resolved strictly by runner',RLIMIT_NOFILE=4096)
 def step(name,pkg,pattern,count,features=None,target=None):
  value['steps'].append({'name':name,'expected_passing_tests':count,'command':['cargo','test','-p',pkg,*(target or ['--lib']),'--locked','--offline','--timings',*(features or []),pattern,'--','--test-threads=1']})
 step('core-profile-families','airk-core','profile_families',3)
 step('ingest-effective-producer','airk-ingest','metadata_support_requires_complete_producer_without_source_io',1)
 for suffix,features in [('lightweight',['--no-default-features']),('helpers',['--all-features'])]:
  step('kernel-profile-families-'+suffix,'airk-kernel','profile_families',2,features)
  step('mcp-family-display-'+suffix,'airk-mcp','conditional_file_family_rows_and_unknown_fields_survive_display_modes',1,features)
  step('cli-info-family-'+suffix,'airk-cli','info_family_flag_forwards_only_explicit_opt_in',1,features,['--bin','airk'])
 assert len(value['steps'])==9 and sum(x.get('expected_passing_tests',0) for x in value['steps'])==12
 assert '--fresh' in value['runner_arguments'] and '--cache' not in value['runner_arguments']
 return value
def verify():
 p=read(B/(LABEL+'-prepared.json'));assert p['native_source']==EXPECTED
 for name,digest in p['artifacts'].items(): assert sha(B/name)==digest,name
 assert read(B/(LABEL+'-plan.json'))==plan()
 assert read(B/(LABEL+'-snapshot.json'))['native_source']==EXPECTED
 assert p['local_proofs']==local_proofs()
 return p
def prepare():
 assert identity()==EXPECTED
 connection=json.loads((B/'neo-connection.local.json').read_text(encoding='utf-8-sig'));assert connection['host'].endswith('.61')
 for suffix in ['.tar.gz','-snapshot.json','-plan.json','-prepared.json','-active.json']:assert not (B/(LABEL+suffix)).exists()
 proofs=local_proofs()
 git=['git','-c','safe.directory='+ROOT.as_posix()]
 names=subprocess.check_output([*git,'ls-files','-z','--cached','--others','--exclude-standard'],cwd=ROOT).split(b'\0')
 excluded={'.git','.codex','.agents','.aws','.mcp.json','.ssh','temp','target','artifacts','node_modules','__pycache__'}
 files=sorted({n.decode() for n in names if n and not(set(Path(n.decode()).parts)&excluded)})
 manifest={};archive=B/(LABEL+'.tar.gz')
 with tarfile.open(archive,'w:gz') as tar:
  for name in files:
   path=ROOT/name
   assert path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(ROOT.resolve()),name
   manifest[name]=sha(path);tar.add(path,arcname=name,recursive=False)
 assert identity()==EXPECTED and all(sha(ROOT/n)==d for n,d in manifest.items())
 save(B/(LABEL+'-snapshot.json'),{'entry_commit':subprocess.check_output([*git,'rev-parse','HEAD'],cwd=ROOT).decode().strip(),'native_source':EXPECTED,'files':manifest,'archive_sha256':sha(archive)})
 save(B/(LABEL+'-plan.json'),plan())
 scripts=['p11-neo.py','p11-neo-owner.py','p11-neo-launch.py',LABEL+'-runner.py']
 for name in scripts:ast.parse((B/name).read_text(encoding='utf-8'),filename=name)
 assert (B/'p11-neo-owner.py').read_text()==(B/'p10-neo-owner.py').read_text().replace('p10-native-','p11-native-')
 artifacts=[LABEL+'.tar.gz',LABEL+'-snapshot.json',LABEL+'-plan.json',*scripts]
 save(B/(LABEL+'-prepared.json'),{'phase':'P11a','label':LABEL,'native_source':EXPECTED,'public_files':len(files),'steps':9,'expected_passing_tests':12,'prepared_only':True,'ssh_launched':False,'saved_host_is_61':True,'script_ast':'passed','owner_change':'prefix only; unconditional finally cargo clean and target absence retained','local_proofs':proofs,'templates':{n:sha(B/n) for n in ['p10-neo-owner.py','p10-neo-launch.py','p10-native-20261106-r4-runner.py','p10-native-20261106-r4-plan.json']},'artifacts':{n:sha(B/n) for n in artifacts}})
 print(json.dumps({'prepared_only':True,'public_files':len(files),'steps':9,'tests':12,'native_source':EXPECTED}))
def collect():
 verify();subprocess.run([sys.executable,str(B/'p11-neo-launch.py'),'collect',LABEL],check=True)
 q=read(B/(LABEL+'-qualification.json'));steps=read(B/(LABEL+'-steps.json'));cleanup=read(B/(LABEL+'-cleanup.json'))
 assert q['phase']=='P11a' and q['status']=='passed' and q['native_source']==EXPECTED
 assert len(steps)==9
 for row,expected in zip(steps,plan()['steps']):
  assert row['name']==expected['name'] and row['actual_exit_code']==row['exit_code']==0
  if 'expected_passing_tests' in expected:assert row['passing_tests']==expected['expected_passing_tests']
 assert cleanup['cargo_clean_exit_code']==0 and cleanup['target_removed']
 save(B/(LABEL+'-verified.json'),{'phase':'P11a','native_source':EXPECTED,'steps':9,'passing_tests':12,'cleanup':cleanup,'qualification_sha256':sha(B/(LABEL+'-qualification.json'))})
if __name__=='__main__':
 action=sys.argv[1];assert action in ['prepare','verify','launch','collect']
 if action=='prepare':prepare()
 elif action=='verify':verify();print('prepared artifacts verified; no SSH or Cargo launched')
 elif action=='launch':verify();subprocess.run([sys.executable,str(B/'p11-neo-launch.py'),'launch',LABEL],check=True)
 else:collect()
