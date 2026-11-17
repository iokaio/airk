# SPDX-License-Identifier: Apache-2.0
import json, re, subprocess, sys
from pathlib import Path
base=Path(__file__).resolve().parent
label=sys.argv[1]
assert re.fullmatch(r'r[1-9][0-9]*',label)
connection=json.loads((base/'neo-connection.local.json').read_text(encoding='utf-8-sig'))
remote='/Users/tylerjensen/airk-builds/p9-native-20261105-'+label
peer=connection['user']+'@'+connection['host']
ssh=['ssh','-i',connection['identity_path'],*connection['ssh_options'],peer]
scp=['scp','-i',connection['identity_path'],*connection['ssh_options']]
for name in ['qualification','cleanup','steps']:
    local=base/('p9-neo-'+label+'-'+name+'.json')
    assert not local.exists(),local
    subprocess.run([*scp,peer+':'+remote+'/'+name+'.json',str(local)],check=True)
subprocess.run([*ssh,'test ! -e '+remote+'/target'],check=True)
qualification=json.loads((base/('p9-neo-'+label+'-qualification.json')).read_text())
cleanup=json.loads((base/('p9-neo-'+label+'-cleanup.json')).read_text())
print(json.dumps({'run':label,'status':qualification['status'],'steps':len(qualification['steps']),
                  'native_source':qualification['native_source'],'cargo_clean_exit_code':cleanup['cargo_clean_exit_code'],
                  'target_removed':True}))
