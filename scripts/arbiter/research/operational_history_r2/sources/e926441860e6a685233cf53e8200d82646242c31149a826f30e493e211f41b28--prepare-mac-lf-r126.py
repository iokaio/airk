# SPDX-License-Identifier: Apache-2.0
import ast,hashlib,re
from pathlib import Path
base=Path('artifacts/advanced-split/local-delivery-preparation-r1')
original=(base/'root_only_mac_r2_batch.sh').read_bytes()
assert hashlib.sha256(original).hexdigest()=='77cdf32fa9f22c810a8360fa4cfec0ff26d5730add8874c52f3e33854af4598f'
fixed=re.sub(b'\r+\n',b'\n',original)
assert b'\r' not in fixed
body=fixed.split(b"<<'AIRK_ROOT_BATCH'\n",1)[1].rsplit(b'\nAIRK_ROOT_BATCH',1)[0]
python=re.sub(b'\r+\n',b'\n',(base/'root_only_mac_r2_batch.py').read_bytes())
assert body.strip()==python.strip()
ast.parse(body.decode('utf-8'))
output=base/'root_only_mac_r2_batch.lf.sh'
assert not output.exists()
output.write_bytes(fixed)
script=Path('artifacts/advanced-split/run-mac-recovery-r125.py').read_text(encoding='utf-8')
script=script.replace('root_only_mac_r2_batch.sh','root_only_mac_r2_batch.lf.sh').replace('77cdf32fa9f22c810a8360fa4cfec0ff26d5730add8874c52f3e33854af4598f',hashlib.sha256(fixed).hexdigest()).replace('mac-recovery-r125','mac-recovery-r126')
Path('artifacts/advanced-split/run-mac-recovery-r126.py').write_text(script,encoding='utf-8')
print(hashlib.sha256(fixed).hexdigest())
