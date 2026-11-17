# SPDX-License-Identifier: Apache-2.0
"""Read only the exact launched R37 campaign's stage/receipt summary."""
import json
from pathlib import Path
import subprocess

base = Path(__file__).resolve().parent
active = json.loads((base / 'neo-active-r35.json').read_text())
campaign = active['campaign']
assert campaign.startswith('/Users/tylerjensen/airk-builds/airk-clean-break-r35-final-')
assert active['launch']['native_source']['sha256'] == 'e62c0e91183d74677065bd647d222f6ff72086ee274ad7135ab58e14f6e66d20'
code = '''from pathlib import Path
import json,re
p = Path(CAMPAIGN)
phase = p / 'phase.txt'
receipt = p / 'evidence/batch-receipt.json'
state = json.loads(receipt.read_text()) if receipt.is_file() else {}
for row in state.get('commands',[]):
    if isinstance(row.get('test_counts'),dict):
        row['test_counts'] = {k:v for k,v in row['test_counts'].items() if isinstance(v,(int,float,str,bool,type(None)))}
progress = None
log = p / 'evidence/focused-clean-break-native-tests.log'
if log.is_file() and not log.is_symlink():
    with log.open('rb') as stream:
        stream.seek(max(0,log.stat().st_size-1048576))
        lines = stream.read(1048576).decode('utf-8','replace')
    counts = re.findall(r'\\(\\s*(\\d+)/(\\d+)\\)',lines)
    known = [line.strip()[:200] for line in lines.splitlines() if re.match(r'^\\s*(Compiling|Finished|PASS|Starting|Summary)\\b',line)]
    progress = {'completed':int(counts[-1][0]),'selected':int(counts[-1][1])} if counts else {'last_public_build_or_test_line':known[-1] if known else None}
print(json.dumps({'campaign':str(p),'phase':phase.read_text() if phase.is_file() else 'not yet published','native_progress':progress,'result':state.get('result'),'cleanup':state.get('cargo_cleanup'),'commands':[{k:r.get(k) for k in ('stage','result','elapsed_seconds','test_counts','error')} for r in state.get('commands',[])],'error':state.get('error')}))
'''.replace('CAMPAIGN', repr(campaign))
subprocess.run(['ssh','-i','C:/Users/tdotn/.ssh/airk_mac_build_ed25519',
    '-oBatchMode=yes','-oStrictHostKeyChecking=yes','-oConnectTimeout=10',
    'tylerjensen@192.168.68.61',
    '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13',
    '-I','-'], input=code, text=True, check=True, timeout=30)
