# SPDX-License-Identifier: Apache-2.0
import json,shlex,subprocess
from pathlib import Path
base=Path(__file__).resolve().parent
connection=json.loads((base.parent/'v190/neo-connection.local.json').read_text())
script="""from pathlib import Path
p=Path('/Users/tylerjensen/airk-builds/branch-review-native-r4')
print((p/'phase.txt').read_text() if (p/'phase.txt').exists() else 'preparing')
print((p/'owner.log').read_text(errors='replace')[-1500:])
print((p/'native-list.log').read_text(errors='replace')[-900:] if (p/'native-list.log').exists() else '')
print((p/'cleanup.json').read_text() if (p/'cleanup.json').exists() else 'cleanup pending')
"""
command='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/python3.13 -c '+shlex.quote(script)
subprocess.run(['ssh','-i',connection['identity_path'],*connection['ssh_options'],connection['user']+'@'+connection['host'],command],check=True)
