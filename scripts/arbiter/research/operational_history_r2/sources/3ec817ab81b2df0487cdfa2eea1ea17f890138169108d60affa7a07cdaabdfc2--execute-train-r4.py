# SPDX-License-Identifier: Apache-2.0
import json,subprocess,sys
from pathlib import Path
r=Path.cwd();p=json.loads((r/'artifacts/advanced-split/r41-image-private.json').read_text())
cmd=[sys.executable,str(r/'artifacts/advanced-split/models/native-parent-bag-proposal-r1/native_stage_r4.py'),'execute','--binary',p['binary'],'--binary-sha256',p['binary_sha256'],'--dep-info',p['dep_info'],'--dep-info-sha256',p['dep_info_sha256'],'--native-source-sha256',p['native_source_sha256'],'--source-group',p['source_group'],'--retained-dir',str(r/'artifacts/advanced-split/native-harnesses/context-integrated-r41')]
with (r/'artifacts/advanced-split/receipts/native-train-r4-root.log').open('wb') as log: result=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
print('native TRAIN R4 exit',result.returncode)
raise SystemExit(result.returncode)
