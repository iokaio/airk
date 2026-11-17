# SPDX-License-Identifier: Apache-2.0
import sys,unittest
sys.path[:0]=['D:/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z/f2-native-worker-implementation-r2','D:/code/Github/airk/artifacts/advanced-split/models/training-env/venv/Lib/site-packages']
import torch
print('TORCH',torch.__version__,'GPU_INITIALIZED',torch.cuda.is_initialized(),flush=True)
suite=unittest.defaultTestLoader.discover(sys.path[0],pattern='test_lora_torch.py')
result=unittest.TextTestRunner(verbosity=2).run(suite)
print('GPU_INITIALIZED_FINAL',torch.cuda.is_initialized(),flush=True)
sys.exit(0 if result.wasSuccessful() and result.testsRun==4 and not torch.cuda.is_initialized() else 1)