# SPDX-License-Identifier: Apache-2.0
import os,stat,json,hashlib,subprocess,base64
from pathlib import Path
assert os.getuid()==501==os.geteuid()==os.stat('/dev/console').st_uid and os.getuid()!=0
base=Path('/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1');source=base/'source'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def git(*args,cwd=source):return subprocess.check_output(['/usr/bin/git','--no-optional-locks',*args],cwd=cwd,timeout=20)
def guard():
 manifest=base/'transfer/bridge-059a2a89-r1/source-bridge-manifest.json';m=json.loads(manifest.read_bytes());assert m['commit']=='059a2a893069f89e8ea8aefec7f9bdb8bb6063b6'
 for r in m['files']:
  q=source/r['path'];s=q.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and sha(q)==r['sha256']
 d={'head':git('rev-parse','HEAD').decode().strip(),'clean':not git('status','--porcelain=v1','--untracked-files=all'),'source_files':len(m['files']),'manifest_sha256':sha(manifest),'raw_c7':m['raw_c7'],'old_dirty_sha256':hashlib.sha256(git('status','--porcelain=v1','-z','--untracked-files=normal',cwd=Path('/Users/tylerjensen/code/airk'))).hexdigest()}
 assert d['head']==m['commit'] and d['clean'] and d['old_dirty_sha256']=='943936de2803601aa34eeb0d5c974bbb94261dc762496bd44196177755734b06';return d
def metadata(s):return {'device':s.st_dev,'inode':s.st_ino,'uid':s.st_uid,'gid':s.st_gid,'mode':oct(s.st_mode),'permissions':oct(stat.S_IMODE(s.st_mode)),'nlink':s.st_nlink,'bytes':s.st_size,'flags':s.st_flags,'mtime_ns':s.st_mtime_ns,'ctime_ns':s.st_ctime_ns,'birthtime':s.st_birthtime,'atime_ns':s.st_atime_ns,'symlink':stat.S_ISLNK(s.st_mode),'directory':stat.S_ISDIR(s.st_mode)}
def stable(s):return tuple(getattr(s,n) for n in ('st_dev','st_ino','st_uid','st_gid','st_mode','st_nlink','st_size','st_flags','st_mtime_ns','st_ctime_ns'))
before=guard();rows=[]
for path in ('/','/etc','/private','/private/etc'):
 first=os.lstat(path);followed=os.stat(path);link=os.readlink(path) if stat.S_ISLNK(first.st_mode) else None
 command=['/bin/ls','-ldeO@',path];r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=20);assert len(r.stdout)+len(r.stderr)<=16384 and r.returncode==0
 attrs=[];total=len(r.stdout);assert total<=16384
 last=os.lstat(path);followed_after=os.stat(path);assert stable(first)==stable(last) and stable(followed)==stable(followed_after)
 rows.append({'path':path,'lstat':metadata(first),'stat':metadata(followed),'readlink':link,'ls_argv':command,'ls_exit_code':r.returncode,'ls_stdout':r.stdout.decode('utf-8'),'ls_stderr':r.stderr.decode('utf-8'),'xattrs_metadata_source':'native ls -ldeO@ nofollow listing; names and sizes only, no values read','xattr_listing_bytes_bound':total,'metadata_unchanged':True,'atime_unchanged':first.st_atime_ns==last.st_atime_ns})
after=guard();assert after==before
print(json.dumps({'actor':{'real':os.getuid(),'effective':os.geteuid(),'console':os.stat('/dev/console').st_uid},'scope':'ONE Root-authorized read-only OS alias metadata census of exactly /,/etc,/private,/private/etc; no follow-open payload bytes, no Cargo/native/service/target/write/cleanup/alias behavior change. Source hashed only for guards; no source/private file content exported. No analysis document contents read.','before':before,'paths':rows,'after':after},indent=2))
