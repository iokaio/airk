# SPDX-License-Identifier: Apache-2.0
set -eu
u=$(id -u); h=$(stat -f %u /dev/console)
[ "$u" = 501 ] && [ "$u" = "$h" ] && [ "$u" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'REMOTE'
from pathlib import Path
import json,os,subprocess
p=Path.home()/'airk-builds/arbiter-final-20261116-r1';e=p/'evidence'
events=e/'prerequisite-events-r1.jsonl'
rows=[json.loads(s) for s in events.read_text().splitlines()] if events.exists() else []
print(json.dumps({'uid':os.getuid(),'console_uid':os.stat('/dev/console').st_uid,'events':rows,'static_inputs_present':(p/'inputs/static-ort130-aarch64').exists(),'result_present':(e/'prerequisite-result-r1.json').exists()},indent=2))
for log in sorted(e.glob('prerequisite-command-*-r1.log')):
 print('LOG',log.name,log.stat().st_size);print(log.read_text(errors='replace')[-1800:])
for log in sorted((p/'inputs/static-ort130-aarch64/logs').glob('*.log')):
 print('STATIC LOG',log.name,log.stat().st_size);print(log.read_text(errors='replace')[-1000:])
REMOTE
