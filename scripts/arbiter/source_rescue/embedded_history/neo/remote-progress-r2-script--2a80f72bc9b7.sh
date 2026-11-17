# SPDX-License-Identifier: Apache-2.0
set -eu
u=$(id -u); h=$(stat -f %u /dev/console)
[ "$u" = 501 ] && [ "$u" = "$h" ] && [ "$u" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'REMOTE'
from pathlib import Path
import json,subprocess,os
p=Path.home()/'airk-builds/arbiter-final-20261116-r1';o=p/'inputs/static-ort130-aarch64';e=p/'evidence'
events=[json.loads(x) for x in (e/'prerequisite-events-r1.jsonl').read_text().splitlines()]
print(json.dumps({'uid':os.getuid(),'console_uid':os.stat('/dev/console').st_uid,'last_event':events[-1],'downloads':[{'name':q.name,'bytes':q.stat().st_size} for q in sorted((o/'downloads').glob('*')) if q.is_file()],'source_inventory':(o/'source-files.json').exists(),'build_attempt':(o/'build-attempt.json').exists(),'build_receipt':(o/'build-receipt.json').exists(),'result_present':(e/'prerequisite-result-r1.json').exists()},indent=2))
for q in [e/'prerequisite-command-03-r1.log',*(o/'logs').glob('*.log')]:
 if q.exists():print('LOG',q.name,q.stat().st_size,q.read_text(errors='replace')[-1400:])
print(subprocess.check_output(['/bin/ps','-p','92832','-o','pid=,ppid=,uid=,etime=,comm='],text=True))
REMOTE
