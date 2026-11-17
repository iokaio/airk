# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,hashlib,subprocess,time,stat,importlib.util
assert os.getuid()==os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');S=B/'source';W=pathlib.Path('/mnt/d/code/Github/airk');P=W/'artifacts/v190/final-plan/exec-20261116T052942Z/linux-userspace-provisioning-plan-r1/aggregate-canonical-tool-proposal-r1'
assert B.resolve()==B and B.stat().st_uid==1000
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();manifest=json.loads((B/'source-transfer/source-manifest.json').read_text());assert manifest['commit']=='059a2a893069f89e8ea8aefec7f9bdb8bb6063b6'
for row in manifest['files']:
 p=S/row['path'];assert p.lstat().st_uid==1000 and sha(p)==row['sha256'] and p.stat().st_size==row['bytes'],row['path']
assert not subprocess.check_output(['git','-C',str(S),'status','--porcelain']).strip()
assert sha(P/'proposal.patch')=='8251ae643058dbd7264bab5de35fc20d2919dc11b224e8c990e4ae4035e637cb'
bridge={'schema':'airk.private.linux-reviewed-aggregate-source-overlay/1','base_commit':manifest['commit'],'base_tree':manifest['normalized_tree'],'base_raw_c7':manifest['raw_c7'],'base_files_reverified':len(manifest['files']),'patch_sha256':sha(P/'proposal.patch'),'files':[]}
for name in ['scripts/static-onnx-aggregate.py','scripts/tests/test_static_ort_aggregate.py']:
 old=S/name;new=P/name;assert new.read_bytes()==(W/name).read_bytes(),'Root-applied file differs from reviewed proposal: '+name
 st=old.stat();before={'sha256':sha(old),'inode':st.st_ino,'uid':st.st_uid,'mode':stat.S_IMODE(st.st_mode)}
 with old.open('r+b') as f:f.write(new.read_bytes());f.truncate()
 after=old.stat();assert after.st_ino==st.st_ino and after.st_uid==st.st_uid and stat.S_IMODE(after.st_mode)==stat.S_IMODE(st.st_mode)
 bridge['files'].append({'path':name,'before':before,'after_sha256':sha(old),'inode_preserved':True,'owner_mode_preserved':True})
changed=set(subprocess.check_output(['git','-C',str(S),'diff','--name-only'],text=True).splitlines());assert changed=={x['path'] for x in bridge['files']}
spec=importlib.util.spec_from_file_location('updated_linux_source_identity',S/'scripts/build-service-runtime.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);bridge['overlay_raw_c7']=m.source_identity();bridge['git_status']=subprocess.check_output(['git','-C',str(S),'status','--porcelain'],text=True);bridge['head_unmodified']=subprocess.check_output(['git','-C',str(S),'rev-parse','HEAD'],text=True).strip()==manifest['commit']
q=B/'evidence/reviewed-aggregate-overlay-r1.json';q.write_text(json.dumps(bridge,indent=2)+'\n');print(json.dumps(bridge));print('RECEIPT_SHA256',sha(q))
