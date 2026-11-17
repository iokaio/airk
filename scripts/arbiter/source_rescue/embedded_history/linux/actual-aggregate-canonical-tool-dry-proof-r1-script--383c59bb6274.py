# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,importlib.util,hashlib,stat,subprocess,time
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');S=B/'source';D=B/'inputs/static-ort';T='x86_64-unknown-linux-gnu'
def load(name,p):
 spec=importlib.util.spec_from_file_location(name,p);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
core=load('original_frozen_static_owner',S/'scripts/provision-static-onnx-runtime.py');old=load('original_frozen_aggregate',S/'scripts/static-onnx-aggregate.py');p=pathlib.Path('/mnt/d/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z/linux-userspace-provisioning-plan-r1/aggregate-canonical-tool-proposal-r1/scripts/static-onnx-aggregate.py');proposal=load('private_proposed_aggregate',p)
began=time.monotonic();original=core.verify_original_build(D,T);selected=proposal.plan(D,T,original,core);oldrequest=old.invocation(D,T,original,selected,core)
try:core.plain(oldrequest['tool'],regular=True);raise AssertionError('actual alias refusal absent')
except ValueError as e:refusal=str(e);assert 'Linked static-runtime path: /usr/bin/ar' in refusal
request=proposal.invocation(D,T,original,selected,core);tool=core.plain(request['tool'],regular=True);assert tool==pathlib.Path('/usr/bin/x86_64-linux-gnu-ar') and tool.stat().st_nlink==1 and tool.stat().st_mode&0o111
version=subprocess.run([str(tool),'--version'],capture_output=True,text=True,timeout=10);assert version.returncode==0 and 'GNU ar (GNU Binutils for Ubuntu)' in version.stdout
assert not (D/'aggregate').exists() and not (D/'static-aggregate-attempt.json').exists()
r={'schema':'airk.private.actual-linux-aggregate-tool-boundary-proof/1','passed':True,'uid':os.getuid(),'original_completed_cpp_reverified_by_native_owner':True,'original_build_receipt_sha256':sha(D/'build-receipt.json'),'compiled_library_count':len(original['receipt']['libraries']),'phases':original['receipt']['commands'],'runtime_closure_count':len(selected['inputs']),'runtime_closure_bytes':sum(row['bytes'] for row in selected['inputs']),'old_actual_refusal':refusal,'canonical_tool':{'path':str(tool),'sha256':sha(tool),'uid':tool.stat().st_uid,'nlink':tool.stat().st_nlink,'version':version.stdout},'new_proposal_sha256':sha(p),'proposed_aggregation_argv':request['argv'],'fixed_literal_MRI_shell_unchanged':request['argv'][2]=='exec "$1" -M < "$2"','artifact_plain_guard_not_changed':True,'original_cpp_receipts_unmodified':True,'aggregation_or_CPP_not_run':True,'no_aggregate_output_or_attempt_created':True,'seconds':time.monotonic()-began}
q=B/'evidence/aggregate-canonical-tool-dry-proof-r1.json';q.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r));print('RECEIPT_SHA256',sha(q))
