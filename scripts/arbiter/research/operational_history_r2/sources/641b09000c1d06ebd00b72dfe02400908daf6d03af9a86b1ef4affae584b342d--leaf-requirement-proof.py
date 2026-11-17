# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
run=Path(__file__).resolve().parent;runtime=run/'source/artifacts/neo/signed-gui1/runtime'
requirement='=anchor apple generic and certificate leaf[subject.OU] = "5PX7D99PXA" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf = H"4d8496dcab73794378194d040d9177fb28a0f982"'
rows=[]
for name in ['AIRK.app','AIRK.app/Contents/MacOS/airk','AIRK.app/Contents/MacOS/airk-embed','AIRK.app/Contents/MacOS/airk-launcher-image','AIRK.app/Contents/MacOS/libonnxruntime.dylib']:
 command=['/usr/bin/codesign','--verify','--strict','--verbose=2','-R',requirement,str(runtime/name)]
 result=subprocess.run(command,capture_output=True,text=True,timeout=30)
 rows.append({'path':name,'command':command,'exit_code':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
report={'schema':'airk.neo-exact-leaf-requirement-proof/1','status':'passed' if all(row['exit_code']==0 for row in rows) else 'failed','read_only':True,'requirement':requirement,'checks':rows,'target_absent':not (run/'target').exists()}
(run/'evidence/exact-leaf-requirement-proof.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'status':report['status'],'checks':[{k:row[k] for k in ['path','exit_code']} for row in rows],'target_absent':report['target_absent']},indent=2))
