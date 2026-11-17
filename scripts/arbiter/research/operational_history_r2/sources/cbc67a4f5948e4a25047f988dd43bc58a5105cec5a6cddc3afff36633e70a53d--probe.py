# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import ctypes,importlib.util,json,sys,tempfile,hashlib
from unittest.mock import patch
root=Path.cwd();out=root/'artifacts/advanced-split/process-observer-exception-private'
source=root/'scripts/provision-static-onnx-runtime.py';before=source.read_bytes()
spec=importlib.util.spec_from_file_location('observer_probe_owner',source);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
fixed=(out/'run_windows_owned.fixed.py').read_text(encoding='utf-8');exec(compile(fixed,str(out/'run_windows_owned.fixed.py'),'exec'),m.__dict__)
original=ctypes.create_unicode_buffer;observed=[]
def allocate(value,*args,**kwargs):
 if value==32768:
  observed.append(True);raise MemoryError('injected diagnostic allocation failure')
 return original(value,*args,**kwargs)
script=("import subprocess,sys; child=subprocess.Popen([sys.executable,'-I','-B','-c',"
        "\"import threading;print('ready',flush=True);threading.Event().wait()\"],stdout=subprocess.PIPE);"
        "assert child.stdout.readline().strip()==b'ready';print('leader completed',flush=True)")
with tempfile.TemporaryDirectory() as temporary,patch.object(ctypes,'create_unicode_buffer',side_effect=allocate):
 result=m.run_owned([sys.executable,'-I','-B','-c',script],Path(temporary),Path(temporary)/'observer-error.log',10)
assert observed and result['exit_code']==0 and result['owned_job_forced_termination'] and not m.successful(result)
assert result['owned_descendants_settled'] and result['owned_job_final_active_processes']==0 and result['owned_job_final_member_pids']==[]
assert any(row.get('observer_error')=='MemoryError' for row in result['owned_job_observed_member_images'])
assert source.read_bytes()==before
receipt={'private_corrective_seam':True,'public_source_unchanged':True,'public_sha256':hashlib.sha256(before).hexdigest(),'fixed_function_sha256':hashlib.sha256(fixed.encode()).hexdigest(),'result':result,'passed':True}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt))
