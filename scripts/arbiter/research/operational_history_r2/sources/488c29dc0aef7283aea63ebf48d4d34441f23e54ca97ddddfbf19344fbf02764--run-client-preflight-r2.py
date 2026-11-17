# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,subprocess,sys,hashlib,shutil
root=Path.cwd(); path=root/'artifacts/advanced-split/receipts/native-answer-image-r44/compile-answer-validator.log';rows=[]
for line in path.read_text().splitlines():
 try:x=json.loads(line)
 except ValueError:continue
 if x.get('reason')=='compiler-artifact' and x.get('target',{}).get('name')=='combined_reads' and x.get('executable'):rows.append(x)
assert len(rows)==1
original=Path(rows[0]['executable']); out=root/'artifacts/advanced-split/native-harnesses/answer-validator-r44';out.mkdir();exe=out/'combined_reads.exe';shutil.copyfile(original,exe)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(original)==sha(exe)
packet={'compiler_artifact':rows[0],'original_sha256':sha(original),'retained':str(exe),'sha256':sha(exe),'compile_log_sha256':sha(path)}
(out/'retention.json').write_text(json.dumps(packet,indent=2)+'\n')
cmd=[sys.executable,'-B','scripts/check-service-clients.py','--runtime-dir','artifacts/advanced-split/runtime-current-context-r2','--files','16','--model','--answer-validator',str(exe),'--report','artifacts/advanced-split/receipts/current-context-pair-preflight-r2.json']
print('Running actual client preflight with retained native validator',packet['sha256'],flush=True)
with (root/'artifacts/advanced-split/receipts/current-context-pair-preflight-r2.log').open('wb') as log:result=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
print('client preflight exit',result.returncode)
raise SystemExit(result.returncode)
