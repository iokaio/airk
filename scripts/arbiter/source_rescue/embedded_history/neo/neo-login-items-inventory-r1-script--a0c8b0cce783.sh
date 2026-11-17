# SPDX-License-Identifier: Apache-2.0
set -eu
effective_uid=$(id -u)
human_uid=$(stat -f %u /dev/console)
printf 'effective_name=%s effective_uid=%s logged_in_name=%s logged_in_uid=%s\n' "$(id -un)" "$effective_uid" "$(stat -f %Su /dev/console)" "$human_uid"
[ "$effective_uid" = "$human_uid" ] && [ "$effective_uid" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'PY'
import json,os,re,subprocess
from pathlib import Path
p=Path('/usr/bin/sfltool');row={'tool':str(p),'exists':p.is_file(),'command':['/usr/bin/sfltool','dumpbtm']}
if p.is_file():
 try:
  done=subprocess.run([str(p),'dumpbtm'],capture_output=True,text=True,timeout=20)
  lines=done.stdout.splitlines();selected=set()
  for i,line in enumerate(lines):
   if 'airk' in line.lower() or 'iokaio' in line.lower():
    selected.update(range(max(0,i-4),min(len(lines),i+7)))
  row.update(exit_code=done.returncode,airk_blocks=[{'line':i+1,'text':lines[i]} for i in sorted(selected)],stderr=done.stderr.strip()[:2000],total_output_lines=len(lines),limits='Only AIRK/Iokaio-related lines and adjacent BTM registration facts retained. No BTM mutation command used.')
 except subprocess.TimeoutExpired:row.update(error='read-only dumpbtm deadline exceeded; no retry')
print(json.dumps(row,indent=2))
PY
