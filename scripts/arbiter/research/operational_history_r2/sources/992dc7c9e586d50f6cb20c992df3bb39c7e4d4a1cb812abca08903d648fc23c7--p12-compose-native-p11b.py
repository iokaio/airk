# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,re,subprocess
root=Path(r'D:/code/Github/airk');a=root/'artifacts/v190';live=a/'p10-worktree';out=a/'p12-native-composed-p11b-r1';out.mkdir(exist_ok=True)
patch=(a/'p12-native-guard-control-baseline-r3.proposed.patch').read_text(encoding='utf-8');m=json.loads((a/'p12-native-guard-control-baseline-r3.proposed.patch.manifest.json').read_text(encoding='utf-8'))
parts={};lines=patch.splitlines(True);i=0
while i<len(lines):
 if not lines[i].startswith('--- '):i+=1;continue
 old=lines[i].strip()[4:];new=lines[i+1].strip()[4:];rel=new.removeprefix('b/');i+=2;hunks=[]
 while i<len(lines) and not lines[i].startswith('--- '):
  if not lines[i].startswith('@@ '):i+=1;continue
  hdr=lines[i];match=re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@',hdr);assert match,hdr
  i+=1;content=[]
  while i<len(lines) and not lines[i].startswith(('@@ ','--- ')):
   if lines[i].startswith((' ','+','-')):content.append(lines[i])
   elif lines[i].startswith('\\'):pass
   else:assert False,lines[i]
   i+=1
  hunks.append((int(match[3]),content))
 parts[rel]=(old,hunks)
records=[];conflicts=[]
for row in m['files']:
 rel=row['path'];proposed_path=a/'p12-integrated-proposal'/rel;raw=proposed_path.read_bytes();assert hashlib.sha256(raw).hexdigest()==row['proposal_sha256'],rel
 proposed=raw.decode('utf-8').replace('\r\n','\n');oldpath,hunks=parts[rel]
 if oldpath=='/dev/null':ancestor=''
 else:
  incoming=proposed.splitlines(True);output=[];cursor=0
  for start,content in hunks:
   start=max(0,start-1);expected=[line[1:] for line in content if line[0] in ' +'];replaced=[line[1:] for line in content if line[0] in ' -'];assert incoming[start:start+len(expected)]==expected,(rel,start)
   output.extend(incoming[cursor:start]);output.extend(replaced);cursor=start+len(expected)
  output.extend(incoming[cursor:]);ancestor=''.join(output)
  digest=hashlib.sha256(ancestor.encode()).hexdigest();windows=hashlib.sha256(ancestor.replace('\n','\r\n').encode()).hexdigest();assert row['before_sha256'] in [digest,windows],(rel,'old-base hash mismatch',row['before_sha256'],digest,windows)
 current=(live/rel).read_bytes() if (live/rel).exists() else b''
 for folder,data in [('base',current),('ancestor',ancestor.encode()),('old-proposal',proposed.encode())]:
  p=out/folder/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
 target=out/'tree'/rel;target.parent.mkdir(parents=True,exist_ok=True)
 if not ancestor:
  assert not current,('new target exists',rel);merged=proposed.encode();status=0
 else:
  cur=out/'normalized-current'/rel;cur.parent.mkdir(parents=True,exist_ok=True);cur.write_bytes(current.replace(b'\r\n',b'\n'))
  process=subprocess.run(['git','-c','safe.directory='+root.as_posix(),'merge-file','--stdout','--diff3','-L','current-P11b','-L','saved-P10-ancestor','-L','P12-r3','--',str(cur),str(out/'ancestor'/rel),str(out/'old-proposal'/rel)],cwd=root,capture_output=True)
  assert process.returncode<128,(rel,process.stderr.decode(errors='replace'));status=process.returncode;merged=process.stdout
 target.write_bytes(merged)
 if status:conflicts.append({'path':rel,'conflict_count':status})
 records.append({'path':rel,'base_sha256':hashlib.sha256(current).hexdigest() if current else None,'ancestor_sha256':hashlib.sha256(ancestor.encode()).hexdigest() if ancestor else None,'old_proposal_sha256':row['proposal_sha256'],'merge_status':status})
(out/'merge-records.json').write_text(json.dumps(records,indent=2)+'\n',encoding='utf-8');(out/'conflicts.json').write_text(json.dumps(conflicts,indent=2)+'\n',encoding='utf-8');print(json.dumps({'files':len(records),'conflicts':conflicts}))
