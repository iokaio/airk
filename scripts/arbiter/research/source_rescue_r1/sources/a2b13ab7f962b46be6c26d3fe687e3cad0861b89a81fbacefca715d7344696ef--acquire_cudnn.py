# SPDX-License-Identifier: Apache-2.0
"""Approved exact cuDNN developer archive acquisition; no extraction/install/build."""
from pathlib import Path
import datetime,hashlib,importlib.util,json,os,stat,sys,time,urllib.request,urllib.parse,zipfile,shutil
ROOT=Path('D:/code/Github/airk');PROPOSAL=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z/gpu-runtime-strategy-proposal-r1'
DEST=Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/gpu-runtime-inputs-r1')
URL='https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-9.10.2.21_cuda12-archive.zip';SIZE=683336095;SHA='c1a4567d822ebda7373fa1f19255dff4942302de741f830160b6c7d1fb31af23'
spec=importlib.util.spec_from_file_location('cpu_owner',ROOT/'scripts/provision-static-onnx-runtime.py');owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
class NoRedirect(urllib.request.HTTPRedirectHandler):
 def redirect_request(self,*args,**kwargs):raise ValueError('exact official acquisition forbids redirects')
def get_small(url,maximum):
 with urllib.request.build_opener(NoRedirect()).open(url,timeout=30) as r:
  if r.geturl()!=url:raise ValueError('crossed metadata URL')
  b=r.read(maximum+1)
  if len(b)>maximum:raise ValueError('metadata exceeds bound')
  return b
def main():
 owner.plain(DEST)
 if DEST.exists():raise ValueError('fresh acquisition destination required')
 if shutil.disk_usage(DEST.parent).free<4*1024**3:raise ValueError('insufficient free private storage')
 meta=get_small('https://developer.download.nvidia.com/compute/cudnn/redist/redistrib_9.10.2.json',65536);lic=get_small('https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/LICENSE.txt',100000)
 if hashlib.sha256(meta).hexdigest()!='73a33a12bbb8eb12b105a515b5921db2e328b3ca679f92b6184c7f32fe94a8b0' or hashlib.sha256(lic).hexdigest()!='4fce5a3b1ecaf4d7d202478e969e4563c7220db2718a76067e286c0202e1195f':raise ValueError('official metadata/license drift')
 row=json.loads(meta)['cudnn']['windows-x86_64']['cuda12']
 if int(row['size'])!=SIZE or row['sha256']!=SHA or 'https://developer.download.nvidia.com/compute/cudnn/redist/'+row['relative_path']!=URL:raise ValueError('crossed official archive identity')
 DEST.mkdir();(DEST/'redistrib_9.10.2.json').write_bytes(meta);(DEST/'redist-LICENSE.txt').write_bytes(lic)
 path=DEST/Path(urllib.parse.urlsplit(URL).path).name;partial=path.with_suffix(path.suffix+'.partial');start=time.monotonic();receipt={'schema':'airk.private.cudnn-acquisition/1','started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':URL,'expected_bytes':SIZE,'expected_sha256':SHA,'byte_cap':SIZE,'deadline_seconds':600,'root':str(DEST),'canonical_owner_sha256':owner.digest(ROOT/'scripts/provision-static-onnx-runtime.py'),'status':'running'}
 try:
  h=hashlib.sha256();n=0
  with urllib.request.build_opener(NoRedirect()).open(urllib.request.Request(URL,headers={'User-Agent':'AIRK-private-runtime-audit/1'}),timeout=30) as r,partial.open('xb') as f:
   if r.geturl()!=URL:raise ValueError('archive origin changed')
   if r.headers.get('Content-Length') and int(r.headers['Content-Length'])!=SIZE:raise ValueError('archive content length differs')
   while True:
    b=r.read(4*1024*1024)
    if not b:break
    n+=len(b)
    if n>SIZE or time.monotonic()-start>=600:raise ValueError('archive size or wall deadline exceeded')
    h.update(b);f.write(b)
   f.flush();os.fsync(f.fileno())
  if n!=SIZE or h.hexdigest()!=SHA:raise ValueError('archive length/hash differs')
  owner.verify_file(partial,{'bytes':SIZE,'sha256':SHA});partial.rename(path)
  seen=set();entries=[];expanded=0;prefix='cudnn-windows-x86_64-9.10.2.21_cuda12-archive'
  with zipfile.ZipFile(path) as z:
   for info in z.infolist():
    name=info.filename;parts=owner.relative_name(name.rstrip('/')).parts
    if parts[0]!=prefix or len(entries)>=4096 or info.flag_bits&1:raise ValueError('archive root/count/encryption differs')
    if any(x.endswith((' ','.')) or x.split('.')[0].upper() in {'CON','PRN','AUX','NUL',*[f'COM{i}' for i in range(1,10)],*[f'LPT{i}' for i in range(1,10)]} for x in parts):raise ValueError('unsafe Windows archive component')
    key=name.rstrip('/').casefold()
    if key in seen:raise ValueError('duplicate case-insensitive archive path')
    seen.add(key);mode=(info.external_attr>>16)&0xffff;kind=stat.S_IFMT(mode)
    if kind not in (0,stat.S_IFREG,stat.S_IFDIR):raise ValueError('archive link/special member')
    if info.file_size<0 or info.file_size>1024**3:raise ValueError('member size exceeds bound')
    expanded+=info.file_size
    if expanded>2*1024**3:raise ValueError('archive expansion exceeds2GiB')
    entries.append({'name':name,'directory':info.is_dir(),'bytes':info.file_size,'compressed_bytes':info.compress_size,'crc32':info.CRC,'mode':mode})
  receipt.update(status='PASS_ARCHIVE_AND_FULL_PREFLIGHT',archive={'path':str(path),'bytes':n,'sha256':h.hexdigest()},elapsed_seconds=time.monotonic()-start,expanded_bytes=expanded,entries=len(entries),extracted=False)
  (DEST/'archive-preflight.json').write_text(json.dumps({'archive':receipt['archive'],'entries':entries,'expanded_bytes':expanded},indent=2)+'\n',encoding='utf8')
 except BaseException as e:
  receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(e).__name__+': '+str(e),elapsed_seconds=time.monotonic()-start);raise
 finally:
  (DEST/'acquisition.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf8');print(json.dumps(receipt),flush=True)
if __name__=='__main__':main()
