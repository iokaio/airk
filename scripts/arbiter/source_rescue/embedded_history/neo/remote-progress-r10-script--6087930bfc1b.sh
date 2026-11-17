# SPDX-License-Identifier: Apache-2.0
set -eu
u=$(id -u); h=$(stat -f %u /dev/console)
[ "$u" = 501 ] && [ "$u" = "$h" ] && [ "$u" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'REMOTE'
from pathlib import Path
import json,os,re
p=Path.home()/'airk-builds/arbiter-final-20261116-r1';e=p/'evidence';o=p/'inputs/static-ort130-aarch64';log=(o/'logs/build.log').read_text(errors='replace')
steps=re.findall(r'^\[\d+/\d+\].*',log,re.M)
events=[json.loads(x) for x in (e/'prerequisite-events-r1.jsonl').read_text().splitlines()]
print(json.dumps({'uid':os.getuid(),'console_uid':os.stat('/dev/console').st_uid,'last_step':steps[-1] if steps else None,'log_tail':log[-1600:],'build_receipt':(o/'build-receipt.json').exists(),'aggregate_receipt':(o/'static-aggregate-receipt.json').exists(),'last_event':{k:v for k,v in events[-1].items() if k!='argv'},'result_present':(e/'prerequisite-result-r1.json').exists()},indent=2))
REMOTE
