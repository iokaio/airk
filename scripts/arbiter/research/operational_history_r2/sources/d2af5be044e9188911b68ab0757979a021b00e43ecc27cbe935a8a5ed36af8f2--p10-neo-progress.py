# SPDX-License-Identifier: Apache-2.0
"""Read bounded progress from one owned Mac batch; never changes remote state."""
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys

label = sys.argv[1]
assert re.fullmatch(r'p10-native-[a-z0-9-]+', label)
base = Path(__file__).resolve().parent
connection = json.loads((base / 'neo-connection.local.json').read_text(encoding='utf-8-sig'))
remote = '/Users/tylerjensen/airk-builds/' + label
code = '''import json,pathlib,shutil
p=pathlib.Path(REMOTE)
s=json.loads((p/'steps.json').read_text()) if (p/'steps.json').exists() else []
c=json.loads((p/'cleanup.json').read_text()) if (p/'cleanup.json').exists() else None
q=json.loads((p/'qualification.json').read_text()) if (p/'qualification.json').exists() else None
print(json.dumps({'completed_steps':len(s),'steps':[{'name':r['name'],'exit_code':r['exit_code'],'passing_tests':r.get('passing_tests')} for r in s],'free_disk_bytes':shutil.disk_usage(p).free,'qualification_status':q.get('status') if q else None,'cleanup':c}))
'''.replace('REMOTE', repr(remote))
command = '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/python3.13 -c ' + shlex.quote(code)
sys.exit(subprocess.run(['ssh', '-i', connection['identity_path'], *connection['ssh_options'],
                        connection['user'] + '@' + connection['host'], command]).returncode)
