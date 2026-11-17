# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,sys
from pathlib import Path
r=Path.cwd();b=r/'artifacts/advanced-split/stage-service-preparation-r1/cal-execution-r102'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
packet=b/'fixture-packet-cooperative-r152.proposed.json';launcher=b/'run_fixture_started_r1.py'
assert sha(packet)=='5c84d41d1744fb9e8210eba6821f803f27ec4e0e64d8c1a44197333540b6e7f7'
assert sha(launcher)=='0c9bb06ffbba6174b4e31ee104ca5590597988ad28636d54e83db59835c3c8f8'
assert sha(b/'fixture_bound_support_r1.py')=='44dddfd8fcd65be4c08865eada3dc4083641c43d2d073a15689859974b0bb826'
old=json.loads((b/'fixture-packet-next-corrected-r141.proposed.json').read_bytes());new=json.loads(packet.read_bytes())
allowed={'work','image','cargo_json','build_receipt','native_source_group'}
assert all(old.get(k)==new.get(k)for k in old.keys()|new.keys() if k not in allowed)
decision=json.loads((b/'root-fixture-decision-next-corrected-r142.json').read_bytes())
decision.update(approved_fixture_packet_sha256=sha(packet),approved_launcher_sha256=sha(launcher),actual_native_source_group='13dc85349c783077aec1ad4eae194e17542a3205639c0fe7d927b165a6a0cf25',source_commit='3d5f8847',
 decision='Root reviewed current R148 full workspace Clippy, six focused purpose/authority regressions, passing original actual CLI/pipe/HTTPS combined transport case and successful actual immutable Kernel/Service Cargo provenance. Authorize NEW same-owner native fixture with unchanged original control/index/cache/configuration/raw model/data/CPU/memory inputs, actual prior natural closure3eaeb5c5049420529ddba644a01cc23f6d999d9a0b4822e441e6a5c7d98333ae and original dbd ownership anchor. New reviewed launcher writes immutable fsynced real entry started.json before copy/native entry; conservative bound is fixed to that start plus original10800 seconds, never extended. Start/bound support reviewedR2/5controls; Root combined harness review327119fb03cdc78cdfabc662f474eab79f8fcda9ebad8f1fc5809f15aee2af69. No Cargo or unrelated NN during this fixture/capture/model window.',
 authorization_scope='Startup, genuine restored Ready/warm/status/fullDiagnostics and native metadata cal-live bindings ONLY; no index regeneration or Configure. Require a separate exact Root common CAL release and exact13-phase controller decision before any task read. Same partially exposed100cohort, no refill/confirmation/model selection. Original1800 phase/10800 fixture/30s call bounds remain. No publication or model qualification granted.')
path=b/'root-fixture-decision-cooperative-r153.json'
with path.open('x',encoding='utf-8',newline='\n')as f:json.dump(decision,f,indent=2);f.write('\n')
print(json.dumps({'decision':str(path),'sha256':sha(path),'packet_sha256':sha(packet)}),flush=True)
result=subprocess.run([sys.executable,str(launcher),'--packet',str(packet),'--packet-sha256',sha(packet),'--decision',str(path),'--decision-sha256',sha(path),'--output',str(b/'fixture-cooperative-r152-owned-actual')])
raise SystemExit(result.returncode)
