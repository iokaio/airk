# SPDX-License-Identifier: Apache-2.0
"""Exact scrubbed R8 producer environment policy; fresh protected paths only."""
import os,sys
from pathlib import Path
def producer_environment(dest,CUDA):
 environment=dict(os.environ)
 forbidden={'CC','CXX','CFLAGS','CXXFLAGS','CPPFLAGS','LDFLAGS','CUDAFLAGS','CUDACXX','CUDAHOSTCXX','CUDAARCHS','NVCC_PREPEND_FLAGS','NVCC_APPEND_FLAGS','CL','_CL_','LINK','LIB','LIBPATH','INCLUDE','PYTHONPATH','PYTHONHOME','CUDNN_FRONTEND_CUDART_LIB_NAME'}
 for key in list(environment):
  if key.upper() in forbidden or key.upper().startswith(('CMAKE_','ORT_','CUDNN_','CUDA_')):environment.pop(key)
 private_temp=dest/'t';private_temp.mkdir()
 environment.update(PATH='C:/Windows/System32;C:/Windows;'+str(Path(sys.executable).parent)+';'+str(CUDA/'bin'),TEMP=str(private_temp),TMP=str(private_temp),TMPDIR=str(private_temp),PYTHONNOUSERSITE='1',CUDA_CACHE_PATH=str(dest/'cc'))
 return environment
