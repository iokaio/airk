# SPDX-License-Identifier: Apache-2.0
import os,stat,json,hashlib,subprocess,time
from pathlib import Path
assert os.getuid()==501==os.geteuid()==os.stat('/dev/console').st_uid and os.getuid()!=0
base=Path('/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1');source=base/'source';out=base/'evidence/platform-native-name-diagnostic-r1'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def git(*args,cwd=source):return subprocess.check_output(['/usr/bin/git','--no-optional-locks',*args],cwd=cwd,timeout=20)
def guard():
 m=base/'transfer/bridge-059a2a89-r1/source-bridge-manifest.json';d=json.loads(m.read_bytes())
 for r in d['files']:
  p=source/r['path'];s=p.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and sha(p)==r['sha256']
 old=hashlib.sha256(git('status','--porcelain=v1','-z','--untracked-files=normal',cwd=Path('/Users/tylerjensen/code/airk'))).hexdigest();assert old=='943936de2803601aa34eeb0d5c974bbb94261dc762496bd44196177755734b06'
 return {'head':git('rev-parse','HEAD').decode().strip(),'clean':not git('status','--porcelain=v1','--untracked-files=all'),'manifest_sha256':sha(m),'raw_c7':d['raw_c7'],'source_files':len(d['files']),'old_dirty_sha256':old}
def plain(p):
 assert p.is_relative_to(base)
 for q in [base,*[base.joinpath(*p.relative_to(base).parts[:i]) for i in range(1,len(p.relative_to(base).parts))]]:
  s=q.lstat();assert stat.S_ISDIR(s.st_mode) and s.st_uid==501 and q.resolve()==q
 s=p.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and not p.is_symlink();return s
entries=[];total=0
def read(p,cap):
 global total
 if not os.path.lexists(p):return {'path':str(p.relative_to(out)),'exists':False}
 before=plain(p);assert before.st_size<=cap;total+=before.st_size;assert total<=24*1024*1024
 raw=p.read_bytes();after=p.lstat();assert (before.st_dev,before.st_ino,before.st_size,before.st_mtime_ns,before.st_ctime_ns)==(after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns,after.st_ctime_ns)
 row={'path':str(p.relative_to(out)),'exists':True,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'content':raw.decode('utf-8'),'atime_unchanged':before.st_atime_ns==after.st_atime_ns};entries.append(row);return row
before=guard();assert before['head']=='059a2a893069f89e8ea8aefec7f9bdb8bb6063b6' and before['clean']
phases=[]
for name in ('scoped-clippy','name-boundary-controls','private-log-controls','exact-reference-refresh'):
 start=read(out/(name+'-outer-start.json'),65536);result=read(out/(name+'-outer-result.json'),65536)
 phases.append({'phase':name,'started':start['exists'],'finished':result['exists'],'result':json.loads(result['content']) if result['exists'] else None})
for name in ('batch-start.json','batch-result.json','cargo-clean-result.json','refresh-final-capture.json','refresh-final-capture-error.json'):read(out/name,256*1024)
# Only completed exact diagnostic receives complete log/detail reads. Active native logs are not sampled.
if phases[-1]['finished']:
 read(out/'receipts/neo-native-name-exact-reference-refresh-r1.json',1024*1024)
 read(out/'receipts/neo-native-name-exact-reference-refresh-r1.log',8*1024*1024)
 retained=out/'retained-reference-refresh-final'
 if retained.is_dir() and not retained.is_symlink():
  for rel in ('reference-refresh-failure/observations.json','reference-refresh-failure/raw-export.json','reference-refresh-failure/raw-errors.jsonl','control/job-errors-1.owner.json','control/job-errors-1/current.jsonl'):
   read(retained/rel,10*1024*1024)
ps=subprocess.run(['/bin/ps','-axo','pid=,ppid=,pgid=,uid=,state=,comm='],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=20);assert ps.returncode==0 and len(ps.stdout)<=1024*1024
processes=[]
for line in ps.stdout.decode().splitlines():
 fields=line.split(None,5)
 if len(fields)!=6:continue
 pid,ppid,pgid,uid,state,comm=fields
 if uid=='501' and any(x in Path(comm).name for x in ('cargo','rustc','clippy','python','airk','clang','cc','ld','ninja')):processes.append({'pid':int(pid),'ppid':int(ppid),'pgid':int(pgid),'uid':int(uid),'state':state,'executable_basename':Path(comm).name})
assert len(processes)<=256
after=guard();assert after==before
print(json.dumps({'scope':'One bounded readonly progress observation; no native/control/signal/cleanup/retry commands. No PID exit or gracefulStop inferred.','actor':{'real':os.getuid(),'effective':os.geteuid(),'console':os.stat('/dev/console').st_uid},'before':before,'phases':phases,'files':entries,'processes':processes,'after':after,'target_exists':(source/'target').exists()},indent=2))
