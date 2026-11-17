# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,subprocess,sys,hashlib
r=Path.cwd();out=r/'artifacts/advanced-split/native-harnesses/context-integrated-r51';packet=r/'artifacts/advanced-split/models/ann-regeneration-preparation-r1/prepared-packet-r2.json';runner=packet.parent/'run_reviewed.py'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
cmd=[sys.executable,'-I','-B',str(runner),'--packet-sha256',sha(packet),'--reviewed-runner-sha256',sha(runner),'--binary',str(out/'airk-kernel.exe'),'--binary-sha256',sha(out/'airk-kernel.exe'),'--compile-receipt',str(out/'compile.json'),'--compile-receipt-sha256',sha(out/'compile.json'),'--artifact-lock',str(out/'artifact-lock.json'),'--artifact-lock-sha256',sha(out/'artifact-lock.json')]
(r/'artifacts/advanced-split/ann-regeneration-r51-command.json').write_text(json.dumps(cmd,indent=2)+'\n')
assert sha(packet)=='d426e1902e77d38971e31da363dc5152bacffe92af769059ebd953de5d06be7b'
assert sha(runner)=='4bf63c0f091565aafbcb727c64bb02edd82c71fb2c7d022c21a54d652cb26f9a'
raise SystemExit(subprocess.run(cmd).returncode)
