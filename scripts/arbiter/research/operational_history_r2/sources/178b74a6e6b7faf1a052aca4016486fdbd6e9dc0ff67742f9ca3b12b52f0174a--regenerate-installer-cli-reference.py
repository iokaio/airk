# SPDX-License-Identifier: Apache-2.0
import difflib,json,os,subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[2];document=root/'docs/cli-reference.md';before=document.read_text()
(root/'artifacts/one-process/installer-cli-reference-before.md').write_text(before)
env=dict(os.environ,AIRK_WRITE_CLI_REFERENCE='1')
binary=root/'artifacts/v190/p10-target/debug/deps/airk-8f1150b8e588c5ea.exe'
result=subprocess.run([str(binary),'cli_reference_documents_every_public_command','--test-threads=1'],env=env,text=True,capture_output=True)
(root/'artifacts/one-process/installer-cli-reference-regenerate.log').write_text(result.stdout+result.stderr)
assert result.returncode==0,result.stdout+result.stderr
after=document.read_text();diff=''.join(difflib.unified_diff(before.splitlines(True),after.splitlines(True),fromfile='before',tofile='generated'))
(root/'artifacts/one-process/installer-cli-reference-generated.diff').write_text(diff)
print(diff)
