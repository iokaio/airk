# SPDX-License-Identifier: Apache-2.0
"""Prepared OS negative control only: invalid bytes are never executable images.
No CUDA/provider DLL, model, compiler, install or global environment is touched.
Run only after Root review; this does not compile/test the ORT patch itself.
"""
import ctypes,json,os,sys,uuid
from pathlib import Path

def main():
    if os.name != 'nt': raise RuntimeError('Windows control only')
    root=Path(sys.argv[1]).absolute()
    if root.exists(): raise RuntimeError('fresh control output required')
    root.mkdir()
    poison=root/'poison';approved=root/'approved';poison.mkdir();approved.mkdir()
    name='airk_d08_invalid_'+uuid.uuid4().hex+'.dll'
    (poison/name).write_bytes(b'AIRK invalid-image negative control. This is not executable code.\n')
    k=ctypes.WinDLL('kernel32',use_last_error=True)
    k.LoadLibraryExW.argtypes=[ctypes.c_wchar_p,ctypes.c_void_p,ctypes.c_uint32];k.LoadLibraryExW.restype=ctypes.c_void_p
    k.FreeLibrary.argtypes=[ctypes.c_void_p];k.FreeLibrary.restype=ctypes.c_int
    k.SetErrorMode(0x0001|0x8000)
    old=os.getcwd();oldpath=os.environ.get('PATH','')
    observations=[]
    try:
        os.chdir(poison);os.environ['PATH']=str(poison)
        for label,target,flags,expected in [('legacy_bare_name_finds_poison',name,0,193),('missing_authenticated_path_refuses_despite_poison',str(approved/name),0x100|0x800,126)]:
            ctypes.set_last_error(0);handle=k.LoadLibraryExW(target,None,flags);error=ctypes.get_last_error()
            if handle: k.FreeLibrary(handle);raise AssertionError('unexpected executable module loaded')
            observations.append({'control':label,'flags':flags,'error':error,'expected_error':expected,'loaded_module':False})
            if error != expected: raise AssertionError(json.dumps(observations))
    finally:
        os.chdir(old);os.environ['PATH']=oldpath
    result={'schema':'airk.private.gpu-loader-os-negative-controls/1','pid':os.getpid(),'observations':observations,'model_calls':0,'provider_loads':0,'scope':'Windows API search behavior only; patched ORT is not built/exercised'}
    with (root/'receipt.json').open('x',encoding='utf8') as f:json.dump(result,f,indent=2);f.write('\n')
    print(json.dumps(result))
if __name__=='__main__':main()
