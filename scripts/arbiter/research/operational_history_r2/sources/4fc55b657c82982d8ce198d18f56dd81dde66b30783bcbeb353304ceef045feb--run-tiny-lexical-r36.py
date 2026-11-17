# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,shutil,types
from pathlib import Path
root=Path.cwd();out=root/'artifacts/advanced-split/models/native-parent-bag-proposal-r1/tiny-lexical-r36';out.mkdir(exist_ok=False)
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
source=root/'crates/airk-kernel/src/inference_owner/evaluation_tests/train_units.rs';assert sha(source)=='d821fb6614516b1ddd87ae516d3f15434d60aec5190e8ea71c68769882042048'
original=root/'artifacts/v190/p10-target/debug/deps/airk_kernel-57b62dce9e600d19.exe';dep=original.with_suffix('.d');assert 'train_units.rs' in dep.read_text();image=out/'airk-kernel.exe';image_sha=sha(original);shutil.copyfile(original,image);assert sha(image)==image_sha
helper=root/'tools/retrieval-train/training_execution.py';raw=helper.read_bytes();assert hashlib.sha256(raw).hexdigest()=='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975';owner=types.ModuleType('retained_tiny_owner');owner.__file__=str(helper);exec(compile(raw,str(helper),'exec'),owner.__dict__)
selector='inference_owner::evaluation_tests::train_units::native_train_tiny_lexical_setup_uses_same_owner_flow';argv=[str(image),'--exact',selector,'--nocapture','--test-threads=1'];(out/'stdin').write_bytes(b'')
with (out/'stdout.log').open('wb') as log,(out/'stdin').open('rb') as stdin: result=owner.run_windows_owned_training(argv,root,log,5,dict(os.environ),stdin,1024**3)
receipt={'argv':argv,'native_image_sha256':image_sha,'rustc_dep_sha256':sha(dep),'native_source_sha256':sha(source),'source_unchanged':sha(source)=='d821fb6614516b1ddd87ae516d3f15434d60aec5190e8ea71c68769882042048','owned':result,'purpose':'Tiny actual ownership/setup flow diagnostic, no model forwards','limits':{'seconds':5,'job_committed_bytes':1024**3,'slots':2}};(out/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt));print((out/'stdout.log').read_text());raise SystemExit(0 if result['exit_code']==0 and not result['timed_out'] and not result['owned_job_forced_termination'] else 1)
