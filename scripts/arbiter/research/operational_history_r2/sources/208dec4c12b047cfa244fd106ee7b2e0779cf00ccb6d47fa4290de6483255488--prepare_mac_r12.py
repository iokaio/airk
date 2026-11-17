# SPDX-License-Identifier: Apache-2.0
import ast,base64,copy,hashlib,json,re
from pathlib import Path
R=Path.cwd(); old=R/'artifacts/advanced-split/mac-cli-await-r11'; out=R/'artifacts/advanced-split/mac-cli-await-r12';out.mkdir(exist_ok=False)
def sha(x):return hashlib.sha256(x).hexdigest()
def write(n,s):(out/n).write_text(s,encoding='utf-8',newline='\n')
base=json.loads((old/'source.json').read_bytes());oldsha=sha((old/'source.json').read_bytes());assert oldsha=='b844d15f9aa0f7c53e623caa2b8bf64ea1bc6543219bed0e9f4484da14ae3759'
name='scripts/dev/receipt_runner.py';raw=(R/name).read_bytes();assert sha(raw)=='5d760f8e43c9c101725d8f39c80bad6b829ff63358addac7cb58825d73264865'
v=copy.deepcopy(base);v['previous_source_seal']=base['source_set_sha256'];v['correction_paths']=[name];v['scope']='R12 harness-only receipt runner correction; all native bytes and other 1609 source rows unchanged from applied R11. Reuse R11 passed Clippy; test only.';v['files'][name]={'bytes':len(raw),'sha256':sha(raw)}
v['source_set_sha256']=sha(json.dumps(v['files'],sort_keys=True,separators=(',',':')).encode());manifest=(json.dumps(v,sort_keys=True,indent=2)+'\n').encode();(out/'source.json').write_bytes(manifest);msha=sha(manifest);seal=v['source_set_sha256']
payload={'base_manifest_path':'artifacts/advanced-split/receipts/mac-cli-await-source-r11.json','base_manifest_sha256':oldsha,'before_sha256':base['files'][name]['sha256'],'after_sha256':sha(raw),'content':base64.b64encode(raw).decode(),'manifest':v,'manifest_sha256':msha}
prefix="""#!/bin/sh
# Root-only single public harness correction; no native source or model transfer.
set -eu
umask 077
cd "$HOME/code/airk"
"$PWD/artifacts/tools/neo-python-3.13.18/bin/python3.13" -I -B - <<'PY'
import base64,hashlib,json,os,subprocess,sys
from pathlib import Path
assert sys.version_info[:3]==(3,13,18)
"""
apply=prefix+'payload=json.loads(base64.b64decode('+repr(base64.b64encode(json.dumps(payload,separators=(',',':')).encode()).decode())+'))\n'+"""root=Path.cwd().resolve();assert os.path.samefile(root,Path.home()/'code/airk')
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def safe(p):
 for q in (p,*p.parents):
  assert not q.is_symlink(),q
  if q==root:return p
 raise ValueError('Outside checkout')
active=[p for p in subprocess.check_output(['/bin/ps','-axo','comm='],text=True).splitlines() if Path(p.strip()).name in ('cargo','cargo-nextest','rustc','clippy-driver','rustfmt')];assert not active,active
bp=safe(root/payload['base_manifest_path']);assert digest(bp)==payload['base_manifest_sha256'];base=json.loads(bp.read_bytes())
for name,row in base['files'].items():
 p=safe(root/name);assert p.is_file() and digest(p)==row['sha256'],name
for name in base['retired_auto_discovery']:assert not (root/name).exists(),name
name='scripts/dev/receipt_runner.py';target=safe(root/name);assert digest(target)==payload['before_sha256']
content=base64.b64decode(payload['content']);assert hashlib.sha256(content).hexdigest()==payload['after_sha256']
stage=safe(root/'artifacts/advanced-split/receipts/mac-cli-await-transfer-r12');stage.mkdir(mode=0o700,exist_ok=False)
original=target.read_bytes();backup=stage/'receipt_runner.before.py'
with backup.open('xb') as f:f.write(original);f.flush();os.fsync(f.fileno())
assert digest(backup)==payload['before_sha256']
recovery={'base_native_seal':base['source_set_sha256'],'path':name,'before_sha256':payload['before_sha256'],'after_sha256':payload['after_sha256'],'backup':str(backup),'native_files_changed':0}
(stage/'recovery.json').write_text(json.dumps(recovery,indent=2)+'\\n')
assert digest(target)==payload['before_sha256']
# Open the existing proven regular source without replacing its ownership/mode.
with target.open('r+b') as f:f.write(content);f.truncate();f.flush();os.fsync(f.fileno())
assert digest(target)==payload['after_sha256']
manifest=(json.dumps(payload['manifest'],sort_keys=True,indent=2)+'\\n').encode();assert hashlib.sha256(manifest).hexdigest()==payload['manifest_sha256']
for name,row in payload['manifest']['files'].items():assert digest(safe(root/name))==row['sha256'],name
mp=safe(root/'artifacts/advanced-split/receipts/mac-cli-await-source-r12.json')
with mp.open('xb') as f:f.write(manifest);f.flush();os.fsync(f.fileno())
print(json.dumps({'source_set_sha256':payload['manifest']['source_set_sha256'],'native_base_seal':base['source_set_sha256'],'changed_files':1,'native_files_changed':0,'recovery':str(stage/'recovery.json')}))
PY
"""
write('apply-source.sh',apply);write('apply-source.review.txt','\n'.join(x if len(x)<500 else '[sealed base64 payload omitted; exactly one public script plus source-hash manifest]' for x in apply.splitlines())+'\n')
for n in ['run.sh','observe.sh','fetch.sh','invoke.py','transport_worker.py']:
 s=(old/n).read_text().replace('mac-cli-await-r11','mac-cli-await-r12').replace('mac-cli-await-source-r11','mac-cli-await-source-r12').replace(oldsha,msha).replace(base['source_set_sha256'],seal)
 if n=='run.sh':
  s='\n'.join(x for x in s.splitlines() if not ('mac-cli-await-clippy ' in x))+'\n'
  s=s.replace('# Fresh RUN-ONLY correction: applied source-r2.json is verified, never reapplied.','# R12 TEST-ONLY; R11 eight-crate Clippy already passed on identical native bytes.')
 if n=='invoke.py':
  s=s.replace("for name,row in manifest['files'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==row['sha256'],('Local public source changed',name)","for name,row in manifest['files'].items():\n  expected='c26935eac28d12ec74037fb95f4ad0c1cc944f741b8c8515b9a86e8c86aab4a7' if name=='scripts/tests/test_receipt_runner.py' else row['sha256']\n  assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==expected,('Local public source changed',name)")
 write(n,s)
plan={'status':'Frozen R12 root-only one public runner update then TEST-ONLY; no SSH/Cargo executed by preparer','base_source_set_sha256':base['source_set_sha256'],'base_remote_manifest':payload['base_manifest_path'],'base_manifest_sha256':oldsha,'current_source_set_sha256':seal,'current_manifest_sha256':msha,'transfer_files':{name:{'before_sha256':payload['before_sha256'],'after_sha256':sha(raw),'bytes':len(raw)}},'native_files_changed':0,'expected_selected':47,'exact_required_cases':34,'passed_clippy_reused':{'directory':'mac-cli-await-r11','elapsed_seconds':86.51354,'crates':8,'native_source_set_sha256':base['source_set_sha256']},'local_only_runner_test_sha256':'c26935eac28d12ec74037fb95f4ad0c1cc944f741b8c8515b9a86e8c86aab4a7','remote_runner_test':'unchanged R11 SHA5b2194; not transferred or invoked','runner_review':'artifacts/advanced-split/receipts/receipt-runner-long-argv-review.json','prepared_scripts':{n:sha((out/n).read_bytes()) for n in ['apply-source.sh','run.sh','fetch.sh','observe.sh','invoke.py','transport_worker.py']}}
# Python syntax only, no shell, Cargo, SSH, or compiled commands.
checks=[]
for n in ['apply-source.sh','run.sh','fetch.sh','observe.sh']:
 fragments=re.findall(r"<<'PY'\n(.*?)\nPY(?:\n|$)",(out/n).read_text(),re.S)
 for i,fragment in enumerate(fragments):ast.parse(fragment,filename=f'{n}:{i}')
 checks.append({'script':n,'python_fragments':len(fragments)})
for n in ['invoke.py','transport_worker.py']:ast.parse((out/n).read_text())
# Verify selector/independent literal assertions are preserved byte for byte.
oldrun=(old/'run.sh').read_text();newrun=(out/'run.sh').read_text();assert oldrun[oldrun.index("# Complete lib execution"):].replace('mac-cli-await-r11','mac-cli-await-r12')==newrun[newrun.index('# Complete lib execution'):]
assert 'mac-cli-await-clippy ' not in newrun and newrun.count('cargo nextest run --profile ci')==1
mismatch=[]
for path,row in v['files'].items():
 expected=plan['local_only_runner_test_sha256'] if path=='scripts/tests/test_receipt_runner.py' else row['sha256']
 if sha((R/path).read_bytes())!=expected:mismatch.append(path)
assert not mismatch,mismatch
plan['local_checks']={'python_syntax':checks,'all_1610_local_rows_verified':True,'only_local_runner_test_override':True,'nextest_command_count':1,'clippy_command_count':0,'unchanged_exact_47_population_and_34_names':True,'remote_execution':False}
write('plan.json',json.dumps(plan,indent=2)+'\n')
write('README.txt',f'''ROOT-ONLY R12: one public script update then TEST-ONLY. No SSH/Cargo/model bytes executed/transferred by preparer.
Applied R11 native/base seal {base['source_set_sha256']}; manifest {oldsha}. Native bytes and 1609 other rows unchanged.
Single runner preimage {payload['before_sha256']} -> {sha(raw)} ({len(raw)} bytes), backed up and fsynced before write. Fresh harness-inclusive seal {seal}; manifest {msha}. Root-reviewed long-argv correction passed 15 focused tests/2.198s after retained genuine ENAMETOOLONG baseline.
DO NOT rerun passed R11 eight-crate Clippy86.51354s. One compatible Nextest batch:47(35CLI+2C9+4IPC+6Kernel) with34 exact independently listed JUnit cases and matching UUID. Same physical root, pinned Python/Nextest, private TMPDIR, actual Cargo compiler evidence, observed compiler exit, retained-before-clean and exact-target cleanup on failure. No inference/model/installed qualification claim.
The updated Python test source is checked locally at its frozen SHA but is NOT transferred; Mac test script remains its exact R11 preimage. Only receipt_runner.py transfers. Source mode is this bounded harness update, never native source replay. Old build.rs remains absent.
Root commands (in order):
 python artifacts/advanced-split/mac-cli-await-r12/invoke.py source --expected-seal {seal}
 python artifacts/advanced-split/mac-cli-await-r12/invoke.py observe --expected-seal {seal}
 python artifacts/advanced-split/mac-cli-await-r12/invoke.py run --expected-seal {seal}
 python artifacts/advanced-split/mac-cli-await-r12/invoke.py fetch --expected-seal {seal}
''')
print(json.dumps({'directory':str(out),'seal':seal,'manifest_sha256':msha,'plan_sha256':sha((out/'plan.json').read_bytes()),'checks':plan['local_checks']},indent=2))
