# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import subprocess,os,json,hashlib,shutil,ctypes,time
root=Path.cwd();out=root/'artifacts/v190/final-plan/exec-20261116T052942Z/ui-workbench-build-r1';destination=out/'source';destination.mkdir();started=time.monotonic();warm=root/'artifacts/v190/final-plan/exec-20261116T052942Z/ui-title-build-r1/source';rows=[]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z']).decode().split(chr(0));paths=sorted(set(x for x in paths if x))
for name in paths:
 parts=Path(name).parts
 if parts[0] in ['artifacts','temp','.airk','.git','.codex','.agents','.aws'] or name=='.mcp.json' or any(x in parts for x in ['node_modules','out','dist','target','.cache']):continue
 if any(x=='.npmrc' or x=='.env' or x.startswith('.env.') for x in parts):continue
 src=root/name
 if src.suffix.lower() in ['.exe','.dll','.dylib','.so','.a','.lib']:raise ValueError('Native binary forbidden: '+name)
 if not src.is_file():continue
 if any(p.is_symlink() or getattr(p.lstat(),'st_file_attributes',0)&0x400 for p in [src,*src.parents] if p!=root.parent):raise ValueError('Reparse input '+name)
 dst=destination/name;dst.parent.mkdir(parents=True,exist_ok=True);raw=src.read_bytes();digest=hashlib.sha256(raw).hexdigest()
 with dst.open('xb') as f:f.write(raw)
 assert sha(src)==digest,'Input changed during copy '+name
 rows.append({'path':name,'bytes':len(raw),'sha256':digest})
# Warm dependency/output input objects must belong to the verified human SID.
adv=ctypes.WinDLL('advapi32',use_last_error=True);kernel=ctypes.WinDLL('kernel32',use_last_error=True)
adv.GetNamedSecurityInfoW.argtypes=[ctypes.c_wchar_p,ctypes.c_uint,ctypes.c_uint,ctypes.POINTER(ctypes.c_void_p),ctypes.c_void_p,ctypes.c_void_p,ctypes.c_void_p,ctypes.POINTER(ctypes.c_void_p)];adv.ConvertSidToStringSidW.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_void_p)];kernel.LocalFree.argtypes=[ctypes.c_void_p]
def verify_owner(path):
 owner=ctypes.c_void_p();descriptor=ctypes.c_void_p();status=adv.GetNamedSecurityInfoW(str(path),1,1,ctypes.byref(owner),None,None,None,ctypes.byref(descriptor));assert status==0,(path,status)
 text=ctypes.c_void_p()
 try:
  assert adv.ConvertSidToStringSidW(owner,ctypes.byref(text));assert ctypes.wstring_at(text)=='S-1-5-21-3678370870-3858642417-826968105-1002','Foreign warm input '+str(path)
 finally:
  if text:kernel.LocalFree(text)
  if descriptor:kernel.LocalFree(descriptor)
transferred=[]
for directory in ['packages/admin/node_modules','packages/vscode/node_modules','packages/vscode/out']:
 source=warm/directory;verify_owner(source)
 for src in sorted(source.rglob('*')):
  if src.is_dir():continue
  assert not src.is_symlink() and not getattr(src.lstat(),'st_file_attributes',0)&0x400,'Redirected warm input'
  verify_owner(src);relative=src.relative_to(warm);dst=destination/relative;assert not dst.exists();dst.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dst);assert sha(src)==sha(dst)
  transferred.append({'path':relative.as_posix(),'bytes':src.stat().st_size,'sha256':sha(dst)})
relevant=[row for row in rows if row['path'].startswith(('packages/admin/','packages/ui/','packages/contracts-gen/','packages/vscode/','fixtures/split/','crates/airk-cli/admin-assets/','crates/airk-mcp/src/help/'))]
for row in relevant:assert sha(root/row['path'])==row['sha256'],'Consumed public source changed during copy '+row['path']
receipt={'schema':'airk.private.ui-title-source-snapshot/1','source_root':str(root),'source_head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'snapshot':str(destination),'snapshot_has_no_git_metadata':True,'source_files':rows,'frozen_relevant_inputs':relevant,'warm_originals_preserved':True,'warm_transfer':transferred,'elapsed_seconds':time.monotonic()-started}
(out/'source-manifest.json').write_text(json.dumps(receipt,indent=2)+chr(10),encoding='utf-8');print(json.dumps({'public_files':len(rows),'relevant_frozen':len(relevant),'warm_files':len(transferred),'warm_bytes':sum(x['bytes'] for x in transferred),'elapsed':receipt['elapsed_seconds']},indent=2))
