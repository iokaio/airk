# SPDX-License-Identifier: Apache-2.0
set -eu
u=$(id -u); h=$(stat -f %u /dev/console)
[ "$u" = 501 ] && [ "$u" = "$h" ] && [ "$u" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'REMOTE'
from pathlib import Path
import json,os,base64,hashlib,stat,subprocess
b=Path.home()/'airk-builds/arbiter-final-20261116-r1';o=b/'inputs/static-ort130-aarch64';e=b/'evidence'
files=[*sorted(e.glob('*-r1.json')),*sorted(e.glob('*-r1.jsonl')),*sorted(e.glob('prerequisite-command-*-r1.log'))]
files += [o/name for name in ('owner.json','source-files.json','build-attempt.json','build-receipt.json','static-aggregate-receipt.json')]
files += sorted((o/'logs').glob('*.log'))
files += sorted((o/'aggregate').glob('*.json'))
rows=[]
for q in files:
 s=q.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and s.st_size<16*1024*1024
 data=q.read_bytes();rows.append({'path':q.relative_to(b).as_posix(),'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data),'base64':base64.b64encode(data).decode()})
links=[]
for folder in (b/'source',b/'inputs',e,b/'transfer'):
 for parent,dirs,names in os.walk(folder,followlinks=False):
  for name in [*dirs,*names]:
   q=Path(parent)/name
   if q.is_symlink():links.append({'path':str(q.relative_to(b)),'uid':q.lstat().st_uid,'target':os.readlink(q),'resolved':str(q.resolve()),'inside_fresh_root':q.resolve().is_relative_to(b)})
processes=subprocess.check_output(['/bin/ps','-axo','pid=,ppid=,uid=,comm='],text=True)
active=[]
for line in processes.splitlines():
 fields=line.strip().split(None,3)
 if len(fields)==4 and fields[2]=='501' and (fields[3].startswith(str(o)) or fields[0]=='92832'):active.append(line)
print(json.dumps({'uid':os.getuid(),'console_uid':os.stat('/dev/console').st_uid,'files':rows,'links':links,'active_static_input_processes':active,'source_target_exists':(b/'source/target').exists()},separators=(',',':')))
REMOTE
