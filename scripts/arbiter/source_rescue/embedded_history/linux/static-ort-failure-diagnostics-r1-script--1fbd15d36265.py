# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,hashlib
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler'
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');D=B/'inputs/static-ort';r={}
for name in ['build-receipt.json','build-attempt.json','static-aggregate-attempt.json','static-aggregate-receipt.json']:
 p=D/name
 if p.exists():
  d=json.loads(p.read_text());r[name]={'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'keys':list(d),'result':d.get('result'),'commands':d.get('commands'),'library_count':len(d.get('libraries',[]))}
p=D/'logs/build.log';r['build_tail']=p.read_text(errors='replace')[-1800:]
p=pathlib.Path('/usr/bin/ar');r['ar']={'is_symlink':p.is_symlink(),'target':os.readlink(p),'canonical':str(p.resolve()),'canonical_uid':p.resolve().stat().st_uid,'canonical_mode':oct(p.resolve().stat().st_mode),'canonical_sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
r['controller_pid376_exists']=pathlib.Path('/proc/376').exists();print(json.dumps(r))
