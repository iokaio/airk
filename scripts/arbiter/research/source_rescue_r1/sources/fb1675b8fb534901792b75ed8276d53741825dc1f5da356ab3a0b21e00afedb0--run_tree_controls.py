# SPDX-License-Identifier: Apache-2.0
"""Root executes two bounded real process-tree witnesses before compiler action."""
import argparse,concurrent.futures,ctypes,datetime,hashlib,importlib.util,json,os,sys,time,uuid
from pathlib import Path
HERE=Path(__file__).resolve().parent
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
def digest(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load(p,n):
 s=importlib.util.spec_from_file_location(n,p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
class Events:
 def __init__(self):
  self.k=ctypes.WinDLL('kernel32',use_last_error=True);k=self.k;k.CreateEventW.argtypes=[ctypes.c_void_p,ctypes.c_bool,ctypes.c_bool,ctypes.c_wchar_p];k.CreateEventW.restype=ctypes.c_void_p;k.WaitForSingleObject.argtypes=[ctypes.c_void_p,ctypes.c_uint32];k.SetEvent.argtypes=[ctypes.c_void_p];k.CloseHandle.argtypes=[ctypes.c_void_p];k.OpenProcess.argtypes=[ctypes.c_uint32,ctypes.c_bool,ctypes.c_uint32];k.OpenProcess.restype=ctypes.c_void_p;k.GetExitCodeProcess.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_uint32)];k.QueryFullProcessImageNameW.argtypes=[ctypes.c_void_p,ctypes.c_uint32,ctypes.c_wchar_p,ctypes.POINTER(ctypes.c_uint32)]
  self.names={n:'Local\\AIRK-tree-r11-'+str(uuid.uuid4())+'-'+n for n in ['ready','release','parent_exit','started','done']};self.handles={};self.process_handles=[]
  for n,name in self.names.items():
   h=k.CreateEventW(None,True,False,name)
   if not h or ctypes.get_last_error()==183:raise ValueError('fresh owned witness event unavailable')
   self.handles[n]=h
 def wait(self,name,ms):return self.k.WaitForSingleObject(self.handles[name],ms)
 def set(self,name):
  if not self.k.SetEvent(self.handles[name]):raise ctypes.WinError(ctypes.get_last_error())
 def process(self,pid):
  h=self.k.OpenProcess(0x100000|0x1000,False,pid)
  if not h:raise ctypes.WinError(ctypes.get_last_error())
  self.process_handles.append(h);buf=ctypes.create_unicode_buffer(32768);size=ctypes.c_uint32(32768)
  if not self.k.QueryFullProcessImageNameW(h,0,buf,ctypes.byref(size)) or os.path.normcase(buf.value)!=os.path.normcase(sys.executable):raise ValueError('actual witness process image differs')
  adv=ctypes.WinDLL('advapi32',use_last_error=True);adv.OpenProcessToken.argtypes=[ctypes.c_void_p,ctypes.c_uint32,ctypes.POINTER(ctypes.c_void_p)];adv.GetTokenInformation.argtypes=[ctypes.c_void_p,ctypes.c_int,ctypes.c_void_p,ctypes.c_uint32,ctypes.POINTER(ctypes.c_uint32)];adv.ConvertSidToStringSidW.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_void_p)];token=ctypes.c_void_p();needed=ctypes.c_uint32();sid_pointer=ctypes.c_void_p();self.k.LocalFree.argtypes=[ctypes.c_void_p]
  if not adv.OpenProcessToken(h,8,ctypes.byref(token)):raise ctypes.WinError(ctypes.get_last_error())
  try:
   adv.GetTokenInformation(token,1,None,0,ctypes.byref(needed));data=ctypes.create_string_buffer(needed.value)
   if not adv.GetTokenInformation(token,1,data,needed,ctypes.byref(needed)):raise ctypes.WinError(ctypes.get_last_error())
   sid=ctypes.cast(data,ctypes.POINTER(ctypes.c_void_p)).contents.value
   if not adv.ConvertSidToStringSidW(sid,ctypes.byref(sid_pointer)):raise ctypes.WinError(ctypes.get_last_error())
   actual=ctypes.wstring_at(sid_pointer)
   if actual!=SID:raise ValueError('actual witness token SID differs')
  finally:
   if sid_pointer:self.k.LocalFree(sid_pointer)
   self.k.CloseHandle(token)
  return h,{'pid':pid,'image':buf.value,'sid':actual}
 def exited0(self,h,ms):
  if self.k.WaitForSingleObject(h,ms)!=0:raise ValueError('original witness HANDLE did not exit')
  code=ctypes.c_uint32()
  if not self.k.GetExitCodeProcess(h,ctypes.byref(code)) or code.value!=0:raise ValueError('actual witness exit nonzero')
 def close(self):
  for h in self.process_handles:self.k.CloseHandle(h)
  for h in self.handles.values():self.k.CloseHandle(h)
def main():
 ap=argparse.ArgumentParser();ap.add_argument('--execute-witness',action='store_true');ap.add_argument('--approved-script-sha256',required=True);ap.add_argument('--approved-input-sha256',required=True);a=ap.parse_args()
 if not a.execute_witness or digest(__file__)!=a.approved_script_sha256 or digest(HERE/'input-pins.json')!=a.approved_input_sha256:raise ValueError('explicit reviewed witness source required')
 req=json.loads((HERE/'continuation-request.json').read_bytes());recipe=load(req['recipe']['path'],'reviewed_r8_tree');lock=json.loads(Path(req['recipe_lock']['path']).read_bytes());o=recipe.cpu_owner(lock);pins=o.read_json(HERE/'input-pins.json')
 for row in pins:o.verify_file(o.plain(row['path'],regular=True),row)
 wall=o.read_json(HERE/'wall-origin.json');o.verify_file(o.plain(wall['start_file']['path'],regular=True),wall['start_file']);cutoff=datetime.datetime.fromisoformat(wall['absolute_cutoff_utc']);out=recipe.PARENT/'capi-tree-controls-r11';idlock=dict(lock);idlock['build_path']={'fresh_child':out.name};policy=recipe.identity_and_parent(o,out,idlock);out.mkdir();env=load(HERE/'producer_environment.py','exact_env_tree').producer_environment(out,recipe.CUDA);tree=load(HERE/'process_tree.py','tree_observer');receipt={'status':'RUNNING','script_sha256':a.approved_script_sha256,'input_pins_sha256':a.approved_input_sha256,'policy':policy,'cases':[],'absolute_original_cutoff_utc':cutoff.isoformat()};start=time.monotonic()
 try:
  for mode in ['normal','linger']:
   case=out/mode;case.mkdir();events=Events();record={'mode':mode};receipt['cases'].append(record)
   payload={'mode':mode,'events':events.names,'parent_marker':str(case/'parent.json'),'child_marker':str(case/'child.json')};recipe.write(case/'worker-request.json',payload);argv=[sys.executable,'-S','-B',str(HERE/'tree_witness_worker.py'),'--request',str(case/'worker-request.json')]
   cap=min(20,(cutoff-datetime.datetime.now(datetime.timezone.utc)).total_seconds())
   if cap<=0:raise TimeoutError('original wall cutoff expired')
   with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
    future=pool.submit(tree.run_tree,o,argv,HERE,case,'tree-'+mode,cap,env,events.names)
    try:
     if mode=='linger':
      if events.wait('started',5000)!=0 or events.wait('ready',5000)!=0:raise ValueError('actual leader/child readiness not observed')
      parent=o.read_json(case/'parent.json');child=o.read_json(case/'child.json');ph,pp=events.process(parent['pid']);ch,cp=events.process(child['pid']);record['actual_processes']=[pp,cp];events.set('parent_exit');events.exited0(ph,2000)
      # Event-conditioned noncompletion check, no fixed sleep and no cleanup extension.
      if events.wait('done',500)!=258 or events.k.WaitForSingleObject(ch,0)!=258:raise ValueError('tree leader returned while actual child remained gated')
      record['parent_exit_observed_while_child_alive_and_wrapper_pending']=True;events.set('release');events.exited0(ch,2000)
     result=future.result(timeout=20);record['owner']=result
     if not o.successful(result):raise ValueError('actual tree witness not naturally settled')
     record['passed']=True
    finally:
     events.set('release');events.set('parent_exit');events.close()
  for row in pins:o.verify_file(o.plain(row['path'],regular=True),row)
  if datetime.datetime.now(datetime.timezone.utc)>=cutoff:raise TimeoutError('original absolute cutoff expired during witness closure')
  receipt['status']='PASS_PROCESS_TREE_WITNESSES_ONLY'
 except BaseException as exc:receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:receipt['elapsed_seconds']=time.monotonic()-start;recipe.write(out/'receipt.json',receipt)
 print(json.dumps({'status':receipt['status'],'output':str(out)}))
if __name__=='__main__':main()
