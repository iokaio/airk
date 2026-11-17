# SPDX-License-Identifier: Apache-2.0
"""One grouped CPU-only literal control batch; no selected checkpoints or native processes."""
import os,sys,time,json,hashlib,unittest,importlib.util,contextlib
from pathlib import Path
HERE=Path(__file__).resolve().parent
assert os.environ.get('CUDA_VISIBLE_DEVICES')==''
sys.path.insert(0,str(HERE));started=time.monotonic()
import torch
assert torch.__version__=='2.10.0+cu128' and not torch.cuda.is_initialized()
spec=importlib.util.spec_from_file_location('g5_r2_controls',HERE/'test_half_reference.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);suite=unittest.defaultTestLoader.loadTestsFromModule(module)
with (HERE/'cpu-literal-controls-r2.log').open('x',encoding='utf-8') as log,contextlib.redirect_stdout(log),contextlib.redirect_stderr(log):
 result=unittest.TextTestRunner(stream=log,verbosity=2).run(suite)
assert not torch.cuda.is_initialized()
receipt={'schema':'airk.private.g5-half-reference-cpu-literal-controls/2','selected':result.testsRun,'passed':result.testsRun-len(result.failures)-len(result.errors)-len(result.skipped),'failures':len(result.failures),'errors':len(result.errors),'skipped':len(result.skipped),'elapsed_seconds':time.monotonic()-started,'torch':torch.__version__,'CUDA_VISIBLE_DEVICES':os.environ['CUDA_VISIBLE_DEVICES'],'GPU_INITIALIZED_before':False,'GPU_INITIALIZED_after':torch.cuda.is_initialized(),'scope':'tiny CPU literals and pinned original Context wrapper instantiated only at synthetic1layerH2; no publisher checkpoint/selected model or external weights read; no selected/native/GPU/model session run','source':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [HERE/'half_reference.py',HERE/'test_half_reference.py']}}
with (HERE/'cpu-literal-controls-r2.json').open('x',encoding='utf-8') as f:json.dump(receipt,f,indent=2);f.write('\n')
print(json.dumps(receipt));sys.exit(0 if result.wasSuccessful() and result.testsRun else 1)
