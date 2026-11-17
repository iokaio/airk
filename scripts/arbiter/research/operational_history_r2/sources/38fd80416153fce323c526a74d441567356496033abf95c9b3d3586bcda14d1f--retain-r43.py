# SPDX-License-Identifier: Apache-2.0
import json,subprocess,hashlib,sys
from pathlib import Path
r=Path.cwd(); logs=r/'artifacts/advanced-split/receipts/native-integration-r43'
rows=[]
for line in (logs/'capture-kernel-image.log').read_text().splitlines():
 try: x=json.loads(line)
 except ValueError: continue
 if x.get('reason')=='compiler-artifact' and x.get('target',{}).get('name')=='airk_kernel' and x.get('profile',{}).get('test') and x.get('executable'):rows.append(x)
assert len(rows)==1
binary=Path(rows[0]['executable']); dep=binary.with_suffix('.d')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
source=r/'crates/airk-kernel/src/inference_owner/evaluation_tests/train_units.rs'
receipt=json.loads((logs/'capture-kernel-image.json').read_text()); group=receipt['input_groups']['native'] if 'input_groups' in receipt else None
print(list(receipt.keys()))
packet={'binary':str(binary),'binary_sha256':sha(binary),'dep_info':str(dep),'dep_info_sha256':sha(dep),'native_source_sha256':sha(source),'source_group':'159bfdec0ceb8e05b7b4c347a6f0e53e50e6bbc32d49c10ed78ba0d1292879d9'}
(r/'artifacts/advanced-split/r43-image-private.json').write_text(json.dumps(packet,indent=2))
cmd=[sys.executable,str(r/'artifacts/advanced-split/models/native-parent-bag-proposal-r1/native_stage_r4.py'),'retain','--binary',str(binary),'--binary-sha256',packet['binary_sha256'],'--dep-info',str(dep),'--dep-info-sha256',packet['dep_info_sha256'],'--native-source-sha256',packet['native_source_sha256'],'--source-group',packet['source_group'],'--retained-dir',str(r/'artifacts/advanced-split/native-harnesses/context-integrated-r43')]
raise SystemExit(subprocess.run(cmd).returncode)

