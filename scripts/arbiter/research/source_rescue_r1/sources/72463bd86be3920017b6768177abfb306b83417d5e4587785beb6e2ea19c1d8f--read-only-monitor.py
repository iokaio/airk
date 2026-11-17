# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import os,json,stat,base64
assert os.getuid()==os.geteuid()==501==os.stat('/dev/console').st_uid and os.getuid()!=0
b=Path.home()/'airk-builds/arbiter-final-20261116-r1';out=b/'evidence/platform-process-observer-repair-r1'
result={'uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid,'out_exists':out.exists(),'target_exists':(b/'source/target').exists(),'files':{},'log_tails':{}}
if out.exists():
 assert out.is_dir() and out.lstat().st_uid==501 and not out.is_symlink()
 names=['batch-start.json','batch-result.json','cargo-clean-result.json']
 for phase in ('installer-clippy','legacy-process-tests','exact-cli-replacement','workspace-clippy','workspace-tests','doctests'):
  names.extend([phase+'-outer-start.json',phase+'-outer-result.json','receipts/neo-process-observer-'+phase+'-r1.json'])
 for name in names:
  p=out/name
  if not p.exists():continue
  st=p.lstat();assert stat.S_ISREG(st.st_mode) and st.st_uid==501 and st.st_nlink==1 and st.st_size<=8*1024*1024
  result['files'][name]=base64.b64encode(p.read_bytes()).decode()
 for phase in ('installer-clippy','legacy-process-tests','exact-cli-replacement','workspace-clippy','workspace-tests','doctests','cargo-clean'):
  p=out/'logs'/(phase+'.log')
  if p.exists():
   st=p.lstat();assert stat.S_ISREG(st.st_mode) and st.st_uid==501 and st.st_nlink==1
   with p.open('rb') as f:f.seek(max(0,st.st_size-12000));data=f.read(12000)
   result['log_tails'][phase]={'bytes_total':st.st_size,'tail':data.decode(errors='replace')}
print(json.dumps(result))
