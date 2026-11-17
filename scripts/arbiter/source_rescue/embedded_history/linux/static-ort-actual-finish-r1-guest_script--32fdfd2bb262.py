# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,hashlib,subprocess,time,datetime,stat
assert os.getuid()==os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
CARD={'schema': 'airk.private.linux-static-ort-build-card/1', 'status': 'PREP ONLY Root separate actual build release required; not executed', 'identity': 'Explicit Ubuntu GUID/user tyler; uid1000 vs registered DefaultUid plus WindowsSID1002 checked before launch', 'guest_cwd': '/home/tyler/airk-builds/arbiter-final-linux-r1/source', 'guest_argv': ['/usr/bin/python3', '-I', '-B', 'scripts/provision-static-onnx-runtime.py', '--target', 'x86_64-unknown-linux-gnu', '--destination', '/home/tyler/airk-builds/arbiter-final-linux-r1/inputs/static-ort', '--download-tools', '--build', '--parallel', '2', '--cc', '/home/tyler/airk-builds/arbiter-final-linux-r1/toolchain/bin/cc', '--cxx', '/home/tyler/airk-builds/arbiter-final-linux-r1/toolchain/bin/cxx', '--patch', '/usr/bin/patch', '--environment-file', '/home/tyler/airk-builds/arbiter-final-linux-r1/inputs/static-environment.json'], 'child_environment_override': {'PATH': '/home/tyler/airk-builds/arbiter-final-linux-r1/rustup/toolchains/1.98.0-x86_64-unknown-linux-gnu/bin:/home/tyler/airk-builds/arbiter-final-linux-r1/nextest:/home/tyler/airk-builds/arbiter-final-linux-r1/toolchain/bin:/home/tyler/airk-builds/arbiter-final-linux-r1/node/node-v24.15.0-linux-x64/bin:/usr/bin:/bin', 'CARGO_HOME': '/home/tyler/airk-builds/arbiter-final-linux-r1/cargo', 'RUSTUP_HOME': '/home/tyler/airk-builds/arbiter-final-linux-r1/rustup', 'TMPDIR': '/home/tyler/airk-builds/arbiter-final-linux-r1/tmp', 'AIRK_BUNDLED_MODEL_DIR': '/home/tyler/airk-builds/arbiter-final-linux-r1/inputs/bundled-model', 'CC': '/home/tyler/airk-builds/arbiter-final-linux-r1/toolchain/bin/cc', 'CXX': '/home/tyler/airk-builds/arbiter-final-linux-r1/toolchain/bin/cxx', 'AR': '/usr/bin/ar', 'PKG_CONFIG': '/home/tyler/airk-builds/arbiter-final-linux-r1/toolchain/bin/pkg-config', 'PYTHONDONTWRITEBYTECODE': '1'}, 'source_head': '059a2a893069f89e8ea8aefec7f9bdb8bb6063b6', 'source_tree': '92f38579cbd9a4650466cc839e34bde1b8574973', 'source_raw_c7': {'sha256': '06c740ee904ba9ccc3aec6a8359bff2b6e1f76bd061aa26e41e3c55a2bfd910e', 'files': 1944}, 'guest_readiness_path': '/home/tyler/airk-builds/arbiter-final-linux-r1/evidence/userspace-handoff-r1.json', 'guest_readiness_sha256': '2e1ca2ff4932d7966c3834474827e90c9c591ee265a1b79c590802865e5a4ef2', 'windows_readiness_mirror': 'D:\\code\\Github\\airk\\artifacts\\v190\\final-plan\\exec-20261116T052942Z\\linux-userspace-provisioning-plan-r1\\actual-userspace-handoff-r1.json', 'windows_readiness_mirror_sha256': '316dbb6f0bd39e52459de25861bc832f8740ab296b10a5cd2ad9c96d40dbee0f', 'native_source_archives_already_verified': 17, 'missing_download': 'Pinned Linux CMake3.31.6 archive55,010,959B SHA5a1133ff103c71eb5120e2cc3de922733e7d8a26a98ae716397e8676adb367bf; --download-tools owner downloads/verifies this exact archive and builds pinned Ninja1.12.1', 'unchanged_existing_owner_limits': {'CMake_and_Ninja_configure_each_seconds': 600, 'ORT_configure_seconds': 600, 'ORT_build_seconds': 10800, 'logs_bytes': 67108864, 'cleanup_seconds': 5, 'parallel': 2}, 'root_only_Cargo': True, 'no_product_contract_policy_or_threshold_change': True, 'success_scope': 'Only genuine Linux static-ORT tool/source/build/aggregate/handoff; final Cargo link/model/product/platform checks still follow'}
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');S=B/'source';assert B.resolve()==B and B.stat().st_uid==1000 and stat.S_IMODE(B.stat().st_mode)==0o700
readiness=pathlib.Path(CARD['guest_readiness_path']);assert hashlib.sha256(readiness.read_bytes()).hexdigest()==CARD['guest_readiness_sha256'];h=json.loads(readiness.read_text());assert h['passed']
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
source_manifest=B/'source-transfer/source-manifest.json';m=json.loads(source_manifest.read_text());assert m['commit']==CARD['source_head']
for row in m['files']:
 p=S/row['path'];assert p.lstat().st_uid==1000 and p.stat().st_size==row['bytes'] and sha(p)==row['sha256'],row['path']
assert subprocess.check_output(['/usr/bin/git','-C',str(S),'status','--porcelain']).strip()==b''
for receipt in h['readiness_receipts']:assert sha(pathlib.Path(receipt['path']))==receipt['sha256'],receipt['path']
for pin in h['tool_pins']:assert sha(pathlib.Path(pin['path']))==pin['sha256']
inputs=json.loads((B/'evidence/source-and-native-inputs-r3.json').read_text());assert inputs['passed']
for row in inputs['static_archives']:assert sha(pathlib.Path(row['path']))==row['sha256']
for profile in inputs['models']:
 for row in profile['assets']:assert sha(pathlib.Path(row['path']))==row['sha256']
run=B/'evidence/static-ort-execution-r1';run.mkdir(mode=0o700)
env={**os.environ,**CARD['child_environment_override']};env['PYTHONNOUSERSITE']='1'
record={'schema':'airk.private.linux-static-ort-execution/1','passed':False,'uid':os.getuid(),'user':pwd.getpwuid(1000).pw_name,'start_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'argv':CARD['guest_argv'],'cwd':str(S),'environment_override':CARD['child_environment_override'],'source_head':m['commit'],'source_manifest_sha256':sha(source_manifest),'readiness_sha256':sha(readiness),'source_files_verified':len(m['files']),'static_archive_pins':inputs['static_archives'],'tool_pins':h['tool_pins'],'concurrent_work':'Root Windows4825 Nextest active on same physical host; Linux C++ parallel2 separate target; no latency/resource-p95 claim','Cargo_invoked_by_this_task':False,'native_model_invoked':False,'existing_owner_policy':CARD['unchanged_existing_owner_limits']}
out=run/'execution.json'
def save():out.write_text(json.dumps(record,indent=2)+'\n')
began=time.monotonic();child=None
try:
 with (run/'stdout.log').open('xb') as stdout,(run/'stderr.log').open('xb') as stderr:
  child=subprocess.Popen(CARD['guest_argv'],cwd=S,env=env,stdout=stdout,stderr=stderr)
  record['controller_pid']=child.pid;record['controller_uid']=os.stat('/proc/'+str(child.pid)).st_uid;assert record['controller_uid']==1000;save()
  rc=child.wait();record['exit_code']=rc;record['controller_return_observed']=True
 record['final_stdout_sha256']=sha(run/'stdout.log');record['final_stderr_sha256']=sha(run/'stderr.log')
 record['passed']=rc==0
 if rc!=0:
  record['failure']={'type':'ExistingOwnerRefusal','stdout_tail':(run/'stdout.log').read_text(errors='replace')[-6000:],'stderr_tail':(run/'stderr.log').read_text(errors='replace')[-6000:]}
except BaseException as e:record['failure']={'type':type(e).__name__,'message':str(e)};raise
finally:
 record['seconds']=time.monotonic()-began;record['finish_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();print(json.dumps(record));print('RECEIPT_SHA256',sha(out))
