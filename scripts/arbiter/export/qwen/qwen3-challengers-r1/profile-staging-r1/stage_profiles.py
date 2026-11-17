# SPDX-License-Identifier: Apache-2.0
"""Copy actual Rust-emitted asset overlays byte-for-byte; never constructs Profiles."""
import argparse,hashlib,json,os,stat,time
from pathlib import Path
CHUNK=4*1024*1024

def digest(path):
 h=hashlib.sha256()
 with Path(path).open('rb') as f:
  while block:=f.read(CHUNK):h.update(block)
 return h.hexdigest()
def no_alias(path,anchor):
 path=Path(path).absolute();anchor=Path(anchor).absolute()
 if '..' in path.parts or not path.is_relative_to(anchor):raise ValueError('path outside named private artifacts anchor')
 for item in [path,*path.parents]:
  metadata=item.lstat()
  if stat.S_ISLNK(metadata.st_mode) or getattr(metadata,'st_file_attributes',0)&0x400:raise ValueError('alias/reparse path refused')
  if item==anchor:break
 return path

def small(path,maximum):
 with Path(path).open('rb') as f:raw=f.read(maximum+1)
 if len(raw)>maximum:raise ValueError('metadata byte bound exceeded')
 return raw

def copy_exact(source,target,size,pin):
 if type(size) is not int or not 0<size<=16*1024**3 or source.stat().st_size!=size or target.exists():raise ValueError('source length or fresh destination differs')
 temporary=target.with_name(target.name+'.partial')
 if temporary.exists():raise ValueError('retained partial destination refused')
 h=hashlib.sha256();total=0
 with source.open('rb') as f,temporary.open('xb') as out:
  remaining=size+1
  while remaining:
   block=f.read(min(CHUNK,remaining))
   if not block:break
   total+=len(block);remaining-=len(block);h.update(block);out.write(block)
 if total!=size or h.hexdigest()!=pin or source.stat().st_size!=size:raise ValueError('copied bytes differ from original Core asset identity')
 if digest(temporary)!=pin:raise ValueError('destination independent hash differs')
 os.rename(temporary,target)

def stage(masters,output,anchor):
 output=Path(output).absolute();anchor=Path(anchor).absolute()
 if output.exists() or not output.is_relative_to(anchor):raise ValueError('fresh private staging output required')
 no_alias(output.parent,anchor)
 prepared=[];models=set()
 for path,pin in masters:
  path=no_alias(path,anchor);raw=small(path,1<<20)
  if hashlib.sha256(raw).hexdigest()!=pin:raise ValueError('reviewed actual native master changed')
  master=json.loads(raw)
  if master.get('schema')!='airk.private.selected-native-profile-master/1':raise ValueError('actual Rust-emitted master required')
  for row in master['profiles']:
   profile_path=no_alias(row['profile'],anchor);profile_raw=small(profile_path,1<<20)
   if hashlib.sha256(profile_raw).hexdigest()!=row['profile_sha256']:raise ValueError('actual native Profile bytes changed')
   profile=json.loads(profile_raw);names={asset['name'] for asset in profile['assets']}
   if len(names)!=len(profile['assets']) or names!=set(row['files']) or profile['model'] in models:raise ValueError('duplicate model or incomplete exact asset overlay')
   models.add(profile['model']);fixtures=[a['name'] for a in profile['assets'] if a['role']=='fixture']
   if len(fixtures)!=1:raise ValueError('existing native Input requires one actual Fixture asset')
   for asset in profile['assets']:
    name=asset['name']
    if Path(name).name!=name or name in ['.','..'] or len(name)>128 or not all(c.isascii() and (c.isalnum() or c in '._-')for c in name):raise ValueError('unsafe logical Core asset name')
    no_alias(row['files'][name],anchor)
   prepared.append((profile_raw,profile,row['files'],fixtures[0],pin))
 if len(prepared)!=3:raise ValueError('existing selected service fixture requires exactly three actual native profiles')
 output.mkdir();inputs=[];proof=[];started=time.monotonic()
 for index,(raw,profile,files,fixture,masterpin) in enumerate(prepared):
  root=output/('%02d'%index);root.mkdir();assets=root/'assets';assets.mkdir();profile_path=root/'profile.json';profile_path.write_bytes(raw)
  rows=[]
  for asset in profile['assets']:
   src=no_alias(files[asset['name']],anchor);dst=assets/asset['name'];copy_exact(src,dst,asset['bytes'],asset['sha256']);rows.append({'name':asset['name'],'bytes':asset['bytes'],'sha256':asset['sha256']})
  pin=hashlib.sha256(raw).hexdigest();inputs.append({'profile':str(profile_path),'profile_sha256':pin,'assets':str(assets),'fixtures':str(assets/fixture)})
  proof.append({'model':profile['model'],'source_native_master_sha256':masterpin,'original_and_staged_profile_sha256':pin,'assets':rows})
 (output/'inputs.private.json').write_text(json.dumps(inputs,indent=2)+'\n',encoding='utf-8')
 (output/'staging-receipt.private.json').write_text(json.dumps({'schema':'airk.private.native-profile-flat-staging/1','source_script_sha256':digest(__file__),'profiles':proof,'elapsed_seconds':time.monotonic()-started,'no_hardlinks':True,'no_profile_or_calibration_mutation':True,'scope':'Private evaluation copies only; no NN/qualification/promotion or distributability claim. Later fitted calibration requires a fresh complete directory.'},indent=2)+'\n',encoding='utf-8')
 return inputs

def main():
 p=argparse.ArgumentParser();p.add_argument('--master',type=Path,action='append',required=True);p.add_argument('--master-sha256',action='append',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 if len(a.master)!=len(a.master_sha256):raise ValueError('one reviewed SHA for each actual master required')
 anchor=Path(__file__).resolve().parents[4];inputs=stage(list(zip(a.master,a.master_sha256)),a.output,anchor);print(json.dumps({'inputs':str(a.output/'inputs.private.json'),'profiles':len(inputs),'input_sha256':digest(a.output/'inputs.private.json'),'NN_executed':False}))
if __name__=='__main__':main()
