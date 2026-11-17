# SPDX-License-Identifier: Apache-2.0
set -eu
e=$(id -u); r=$(id -ru); h=$(stat -f %u /dev/console)
[ "$e" = 501 ] && [ "$r" = 501 ] && [ "$e" = "$h" ] && [ "$e" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'REMOTE'
from pathlib import Path
import json,os
p=Path.home()/'airk-builds/arbiter-final-20261116-r1/evidence/platform-admin-repair-r1'
files=[]
for q in [p/'batch-start.json',*sorted(p.glob('*-outer-start.json'))]:
 if q.is_file():
  try:d=json.loads(q.read_text())
  except (OSError,ValueError):continue
  files.append({'path':q.name,'document':d})
logs=[]
for q in [*sorted((p/'logs').glob('*.log')),*sorted((p/'receipts').glob('*.log'))]:
 with q.open('rb') as f:f.seek(max(0,q.stat().st_size-2200));tail=f.read().decode('utf-8',errors='replace')
 logs.append({'path':str(q.relative_to(p)),'bytes':q.stat().st_size,'tail':tail})
print(json.dumps({'uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid,'output_exists':p.exists(),'started':files,'logs':logs},separators=(',',':')))
REMOTE
