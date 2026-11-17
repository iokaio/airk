# SPDX-License-Identifier: Apache-2.0
"""Actual base-interpreter dependency startup only. Never loads/constructs a model."""
import json
import sys
from pathlib import Path
import reference_worker as worker
plan=json.loads(Path(sys.argv[1]).read_bytes())
plan['input_seals'][str(Path(__file__).resolve())]=worker.digest(__file__)
# Pin validation precedes pure dependency/module imports; no weight loader call.
for key in ['reference_module', 'worker', 'runtime_manifest']:
    worker.pinned(plan[key])
import torch
import numpy
import onnx
import safetensors
import transformers
from transformers.models.qwen3.modeling_qwen3 import Qwen3ForCausalLM
module=worker.load_module(plan['reference_module'],'approved_focus_source_startup_probe')
context=Path(plan['reference_module']['path']).parents[1]/'qwen3-challengers-r1/qwen3_reference_r2.py'
worker.load_module({'path':str(context),'sha256':plan['input_seals'].get(str(context),'')},'approved_context_source_startup_probe') if str(context) in plan['input_seals'] else None
failure=None
try:
    worker.verify_loaded_runtime(plan)
except ValueError as error:
    failure=str(error)
assert not torch.cuda.is_initialized(), 'startup probe unexpectedly initialized CUDA'
print(json.dumps({'schema':'airk.private.reference-runtime-startup-probe/1','no_site':bool(sys.flags.no_site),
    'runtime_verified':failure is None,'failure':failure,'python_executable':sys.executable,'python_version':sys.version,
    'sys_path':sys.path,'actual_import_sources':[str(Path(m.__dict__['__file__']).resolve()) for m in list(sys.modules.values()) if isinstance(getattr(m,'__dict__',{}).get('__file__'),str) and Path(m.__dict__['__file__']).is_file()],
    'cuda_initialized':torch.cuda.is_initialized(),'selected_checkpoint_loaded':False,'selected_model_constructed':False,
    'selected_model_forward_invocations':0,'qualification_granted':False}))
if failure:
    sys.exit(3)