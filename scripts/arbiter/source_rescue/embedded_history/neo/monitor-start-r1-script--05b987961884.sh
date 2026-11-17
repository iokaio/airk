# SPDX-License-Identifier: Apache-2.0
set -eu
u=$(id -u); h=$(stat -f %u /dev/console)
[ "$u" = 501 ] && [ "$u" = "$h" ] && [ "$u" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'REMOTE'
from pathlib import Path
import json,os,re
p=Path.home()/'airk-builds/arbiter-final-20261116-r1/evidence/platform-integrated-dda749cb-r1'
print(json.dumps({'uid':os.getuid(),'console_uid':os.stat('/dev/console').st_uid,'output_exists':p.exists(),'json_files':[str(q.relative_to(p)) for q in p.rglob('*.json') if 'fixtures' not in q.parts] if p.exists() else [],'logs':[{'path':str(q.relative_to(p)),'bytes':q.stat().st_size,'tail':q.read_text(errors='replace')[-3000:]} for q in p.glob('logs/*.log')] if p.exists() else []},indent=2))
REMOTE
