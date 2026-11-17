# SPDX-License-Identifier: Apache-2.0
set -eu
e=$(id -u); r=$(id -ru); h=$(stat -f %u /dev/console)
[ "$e" = 501 ] && [ "$r" = 501 ] && [ "$e" = "$h" ] && [ "$e" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'REMOTE'
from pathlib import Path
import json,os,base64,hashlib,stat
p=Path.home()/'airk-builds/arbiter-final-20261116-r1/evidence/platform-admin-repair-r1';copied=[]
summary=[];files=[];errors=[]
def retain(q,phase):
 s=q.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and s.st_size<=64*1024*1024
 data=q.read_bytes();files.append({'phase':phase,'path':q.relative_to(p).as_posix(),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'base64':base64.b64encode(data).decode()})
for phase in ('scoped-clippy','exact-admin-tls','workspace-tests','doctests'):
 q=p/(phase+'-outer-result.json')
 if q.exists():
  try:d=json.loads(q.read_text())
  except (ValueError,OSError) as ex:errors.append({'path':str(q),'error':type(ex).__name__});continue
  native=p/'receipts'/('neo-admin-repair-'+phase+'-r1.json');receipt=json.loads(native.read_text()) if native.exists() else {}
  summary.append({'phase':phase,'outer_passed':d.get('passed'),'result':d.get('result'),'native_receipt_passed':receipt.get('passed'),'selected_tests':receipt.get('selected_tests'),'native_exit_code':receipt.get('exit_code'),'inputs_unchanged':receipt.get('inputs_unchanged')})
  if phase not in copied:
   chosen=[p/'batch-start.json',p/(phase+'-outer-start.json'),q,*sorted((p/'receipts').glob('neo-admin-repair-'+phase+'-r1*'))]
   log=p/'logs'/(phase+'.log')
   if log.exists():chosen.append(log)
   for item in chosen:
    if item.is_file():retain(item,phase)
batch=p/'batch-result.json';final=json.loads(batch.read_text()) if batch.exists() else None
if final is not None and 'batch' not in copied:
 for q in [batch,p/'cargo-clean-result.json',p/'logs/cargo-clean.log']:
  if q.is_file():retain(q,'batch')
logs=[]
for q in [*sorted((p/'logs').glob('*.log')),*sorted((p/'receipts').glob('*.log'))]:
 with q.open('rb') as f:f.seek(max(0,q.stat().st_size-4000));tail=f.read().decode('utf-8',errors='replace')
 logs.append({'path':q.relative_to(p).as_posix(),'bytes':q.stat().st_size,'tail':tail})
print(json.dumps({'uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid,'completed_phases':summary,'batch_result':final,'logs':logs,'read_errors':errors,'files':files},separators=(',',':')))
REMOTE
