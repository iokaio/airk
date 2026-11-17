# SPDX-License-Identifier: Apache-2.0
set -eu
effective_uid=$(id -u)
human_uid=$(stat -f %u /dev/console)
printf 'effective_name=%s effective_uid=%s logged_in_name=%s logged_in_uid=%s\n' "$(id -un)" "$effective_uid" "$(stat -f %Su /dev/console)" "$human_uid"
[ "$effective_uid" = "$human_uid" ] && [ "$effective_uid" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'PY'
import glob,hashlib,json,os,plistlib,pwd,re,stat,subprocess
from pathlib import Path
uid=os.getuid();home=Path(pwd.getpwuid(uid).pw_dir);support=home/'Library/Application Support/AIRK'
def observe(p):
 row={'path':str(p),'exists':os.path.lexists(p)}
 if row['exists']:
  s=p.lstat();row.update(uid=s.st_uid,mode=oct(stat.S_IMODE(s.st_mode)),kind='symlink' if p.is_symlink() else 'directory' if p.is_dir() else 'file',bytes=s.st_size)
  if p.is_symlink():row['target']=os.readlink(p)
 return row
def bounded_json(p,limit):
 row=observe(p)
 if row.get('kind')!='file':return row,None
 if row['bytes']>limit:row['error']='exceeds_bound';return row,None
 raw=p.read_bytes();row['sha256']=hashlib.sha256(raw).hexdigest()
 try:return row,json.loads(raw)
 except ValueError:row['error']='invalid_json';return row,None
pointer_row,pointer=bounded_json(support/'install-pointer/current.json',4096)
if isinstance(pointer,dict):pointer_row['pointer']={k:pointer.get(k) for k in ('schema','install_id','program','record','record_sha256','version')}
record=None;record_row=None
if isinstance(pointer,dict) and isinstance(pointer.get('program'),str) and Path(pointer['program']).is_absolute():
 p=Path(pointer['program'])/'install-record.json';record_row,record=bounded_json(p,65536)
 if isinstance(record,dict):record_row['record']={k:record.get(k) for k in ('schema','install_id','version','channel','locations','executables','installed_by','signing')}
paths=[support,support/'install-pointer',support/'install-pointer/replacing.json',support/'install-pointer/active.json',support/'program',support/'data/indexes',support/'state/service',home/'Library/Caches/AIRK/service',home/'Library/Logs/AIRK/service',home/'Applications/AIRK.app',Path('/Applications/AIRK.app'),home/'.local/bin',home/'.local/bin/airkctl',home/'.local/bin/airk',support/'runtime',support/'runtime-preview',support/'service-1.4',support/'service-1.4-preview',support/'launcher',support/'launcher-preview']
if isinstance(record,dict):
 paths += [Path(v) for v in record.get('locations',{}).values() if isinstance(v,str) and Path(v).is_absolute()]
locations=[observe(p) for p in dict.fromkeys(paths)]
installs=[]
for p in [support/'program',support/'runtime',support/'runtime-preview',support/'service-1.4',support/'service-1.4-preview',home/'Applications',Path('/Applications')]:
 if p.is_dir() and not p.is_symlink():
  # Names and bundle facts only, never source/index content.
  children=[q for q in p.iterdir() if 'airk' in q.name.lower() or q.name in ('install-record.json','runtime-layout.json','active.json','current.json','airkctl-link-owner.txt')]
  for q in children[:30]:
   row=observe(q)
   info=q/'Contents/Info.plist'
   if q.suffix=='.app' and info.is_file() and not info.is_symlink() and info.stat().st_size<65536:
    try:
     d=plistlib.loads(info.read_bytes());row['bundle']={k:d.get(k) for k in ('CFBundleIdentifier','CFBundleShortVersionString','CFBundleVersion','CFBundleExecutable')}
    except (ValueError,plistlib.InvalidFileException):row['bundle_error']='invalid_plist'
   installs.append(row)
proc=subprocess.run(['/bin/ps','-axo','pid=,uid=,comm='],capture_output=True,text=True,timeout=15)
processes=[line.strip() for line in proc.stdout.splitlines() if re.search(r'(^|[/ -])airk(?:ctl|-|\.|$)',line,re.I)]
listed=subprocess.run(['/bin/launchctl','list'],capture_output=True,text=True,timeout=15)
matching=[line.strip() for line in listed.stdout.splitlines() if 'airk' in line.lower()]
labels={'io.iokaio.airk.service.stable','io.iokaio.airk.service.preview'}
labels.update(line.split()[-1] for line in matching if line.split())
jobs=[]
for label in sorted(labels):
 if not re.fullmatch(r'[A-Za-z0-9_.-]{1,200}',label):continue
 done=subprocess.run(['/bin/launchctl','print',f'gui/{uid}/{label}'],capture_output=True,text=True,timeout=15)
 facts=[line.strip() for line in done.stdout.splitlines() if re.match(r'^\s*(state|pid|path|program) = ',line)]
 jobs.append({'label':label,'exit_code':done.returncode,'facts':facts,'stderr':done.stderr.strip()[:400]})
plists=[]
for directory in [home/'Library/LaunchAgents',Path('/Library/LaunchAgents'),Path('/Library/LaunchDaemons')]:
 if directory.is_dir():
  for p in directory.iterdir():
   if 'airk' in p.name.lower():
    row=observe(p)
    if row.get('kind')=='file' and row['bytes']<65536:
     try:
      d=plistlib.loads(p.read_bytes());row['definition']={k:d.get(k) for k in ('Label','Program','ProgramArguments','RunAtLoad','KeepAlive')}
     except (ValueError,plistlib.InvalidFileException):row['error']='invalid_plist'
    plists.append(row)
profile_matches=[]
for name in ('.zprofile','.zshrc','.profile','.bash_profile','.bashrc'):
 p=home/name
 if p.is_file() and not p.is_symlink() and p.stat().st_size<262144:
  found=[{'line':i,'mentions_airk':True,'mentions_path':bool(re.search(r'PATH|path=',line))} for i,line in enumerate(p.read_text(errors='replace').splitlines(),1) if 'airk' in line.lower()]
  if found:profile_matches.append({'path':str(p),'matches':found})
print(json.dumps({'native_home':str(home),'pointer':pointer_row,'record':record_row,'locations':locations,'installation_entries':installs,'processes':{'command':'ps -axo pid=,uid=,comm=','exit_code':proc.returncode,'airk_matches':processes},'launchctl_list':{'exit_code':listed.returncode,'airk_matches':matching},'launchctl_jobs':jobs,'airk_launch_plists':plists,'ssh_path':os.environ.get('PATH','').split(':'),'shell_profile_airk_mentions':profile_matches,'limits':'Launchctl absence alone does not prove disabled/pending SMAppService registration absent. No product executable, service start/status RPC, app launch or login mutation executed.'},indent=2))
PY
