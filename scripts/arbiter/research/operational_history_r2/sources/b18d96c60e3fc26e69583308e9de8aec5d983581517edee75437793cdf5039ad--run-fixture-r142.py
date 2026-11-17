# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,sys
from pathlib import Path
r=Path.cwd();b=r/'artifacts/advanced-split/stage-service-preparation-r1/cal-execution-r102'
packet=b/'fixture-packet-next-corrected-r141.proposed.json';launcher=b/'run_fixture_restart_r4.py'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(packet)=='163f3d194bafe827e25cf279f785298013c81541feefc87378323520a513d036'
assert sha(launcher)=='8e3135f10ec415b8bce76d5cf3fbda094d5942b5e9f025cac33bb50f64b71afe'
decision=json.loads((b/'root-fixture-decision-restart-r136.json').read_bytes())
decision.update(approved_fixture_packet_sha256=sha(packet),actual_native_source_group='40c2871bc63b3f74ae67b1974aa6501bda1f04430fd8922f5b601a13aa5e6de5',
    decision='Root reviewed exact R141 successful unchanged Cargo/source/image closure and canonical-purpose correction. Full workspace Clippy R140 passed; nine focused controls passed there and all four MCP Focus controls passed after a test-only valid Help baseline correction in R141. Purpose guards validate original binding and retain exact approved Search/Answer matrix, original source/scope/budget/deadline fences; independent FocusContract review passed. Authorize this NEW private same-owner fixture under original descriptor dbd5d257937c48fdc3fcdaf0ad9b7d09a30d70d4e50e3d0d4d9e53c9c7f9c84f and actual latest naturally settled fixture closure 725efbac88d564af77476b73bb77b5d87b3633ab4df6a78cae8b72032d0be4b6. Root packet comparison found only actual image/build/source pins and fresh metadata work changed. Original control/index/cache and fixed model/scientific inputs remain unchanged. No Cargo or unrelated NN until fixture is closed.',
    source_commit='ccbe5096',qualification_granted=False)
path=b/'root-fixture-decision-next-corrected-r142.json'
with path.open('x',encoding='utf-8',newline='\n') as f:json.dump(decision,f,indent=2);f.write('\n')
print(json.dumps({'decision':str(path),'sha256':sha(path),'packet_sha256':sha(packet)}),flush=True)
result=subprocess.run([sys.executable,str(launcher),'--packet',str(packet),'--packet-sha256',sha(packet),'--decision',str(path),'--decision-sha256',sha(path),'--output',str(b/'fixture-next-corrected-r141-owned-actual')])
raise SystemExit(result.returncode)
