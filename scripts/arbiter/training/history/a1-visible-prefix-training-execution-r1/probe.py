# SPDX-License-Identifier: Apache-2.0
"""Actual runtime metadata only: imports, DLL identities, driver version; no tensors/model."""
from pathlib import Path
import ctypes
from ctypes import wintypes
import importlib.metadata
import json
import os
import subprocess
import sys

import torch
from transformers import BertConfig, BertModel  # Same actual imports as trainer; never construct either.
from safetensors.torch import load as load_tensors
from tokenizers import Tokenizer
import numpy

HERE = Path(__file__).resolve().parent
kernel = ctypes.WinDLL('kernel32', use_last_error=True)
kernel.GetCurrentProcess.restype = wintypes.HANDLE
process = kernel.GetCurrentProcess()
psapi = ctypes.WinDLL('psapi', use_last_error=True)
psapi.EnumProcessModulesEx.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), wintypes.DWORD]
psapi.GetModuleFileNameExW.argtypes = [wintypes.HANDLE, wintypes.HMODULE, wintypes.LPWSTR, wintypes.DWORD]
driver = ctypes.WinDLL('nvcuda.dll')
driver_version = ctypes.c_int()
if driver.cuDriverGetVersion(ctypes.byref(driver_version)) != 0:
    raise RuntimeError('CUDA driver metadata query failed')
modules = (wintypes.HMODULE * 2048)()
needed = wintypes.DWORD()
if not psapi.EnumProcessModulesEx(process, modules, ctypes.sizeof(modules), ctypes.byref(needed), 3) or needed.value > ctypes.sizeof(modules):
    raise RuntimeError('cannot close actual loaded DLL roster')
paths = []
for module in modules[:needed.value // ctypes.sizeof(wintypes.HMODULE)]:
    text = ctypes.create_unicode_buffer(32768)
    if not psapi.GetModuleFileNameExW(process, module, text, len(text)):
        raise RuntimeError('loaded module path unavailable')
    paths.append(str(Path(text.value).resolve()))
smi = str(Path(os.environ['SystemRoot']) / 'System32/nvidia-smi.exe')
gpu = subprocess.run([smi, '--query-gpu=index,uuid,name,driver_version,memory.total,memory.used,memory.free,compute_cap',
                      '--format=csv,noheader,nounits'], check=True, capture_output=True, text=True, timeout=10)
value = {'schema': 'airk.private-training-runtime-probe/1', 'python_version': sys.version, 'executable': sys.executable,
         'base_prefix': sys.base_prefix, 'sys_path': sys.path,
         'packages': {n: importlib.metadata.version(n) for n in ['torch', 'transformers', 'safetensors', 'tokenizers', 'numpy']},
         'torch': {'version': str(torch.__version__), 'cuda': str(torch.version.cuda), 'cudnn': torch.backends.cudnn.version(),
                   'compiled_cuda': torch._C._cuda_getCompiledVersion(), 'cuda_initialized': torch.cuda.is_initialized()},
         'cuda_driver_api_version': driver_version.value, 'loaded_modules': sorted(set(paths)), 'nvidia_smi': smi,
         'gpu_csv': gpu.stdout.strip(), 'scope': 'No model construction, weights loaded, tensors, inference or training; imports and driver/DLL metadata only.'}
(HERE / 'runtime-probe.json').write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')
print(json.dumps({k: value[k] for k in ['packages', 'torch', 'cuda_driver_api_version', 'gpu_csv']}))
