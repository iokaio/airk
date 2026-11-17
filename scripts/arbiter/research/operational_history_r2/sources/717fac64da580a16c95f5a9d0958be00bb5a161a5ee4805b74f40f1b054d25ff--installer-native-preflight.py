# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,subprocess,sys
from pathlib import Path
root=Path(__file__).resolve().parents[2]
commands=[
 [sys.executable,'scripts/package-native.py','--target','x86_64-pc-windows-msvc','--runtime-dir','artifacts/one-process/debug-runtime-1823','--output','artifacts/one-process/debug-native-1823'],
 [sys.executable,'scripts/package-extension.py','--target','x86_64-pc-windows-msvc','--native-dir','artifacts/one-process/debug-native-1823/publish','--output','artifacts/one-process/debug-extension-1823','--stage-only'],
]
for command in commands:
 print('COMMAND',command,flush=True)
 result=subprocess.run(command,cwd=root)
 if result.returncode:sys.exit(result.returncode)
stage=root/'artifacts/one-process/debug-extension-1823/stage'
def identity():
 return {p.relative_to(stage).as_posix():hashlib.file_digest(p.open('rb'),'sha256').hexdigest() for p in sorted(stage.rglob('*')) if p.is_file()}
before=identity()
(root/'artifacts/one-process/installer-1823-debug-stage-identity.json').write_text(json.dumps(before,indent=2)+'\n')
env=dict(os.environ,AIRK_TEST_EXTENSION_ROOT=str(stage))
command=['node','--test','--test-concurrency=1','--test-reporter=tap','packages/vscode/out/test/*.test.js']
print('COMMAND',command,'AIRK_TEST_EXTENSION_ROOT',str(stage),flush=True)
result=subprocess.run(command,cwd=root,env=env)
assert before==identity(),'Consumed extension stage changed during native qualification'
sys.exit(result.returncode)
