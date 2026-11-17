# SPDX-License-Identifier: Apache-2.0
"""Hash complete actual training runtime and reviewed inputs; never train/infer."""
from pathlib import Path
import hashlib
import json
import os
import platform
import sys
import time

START = time.monotonic()
HERE = Path(__file__).resolve().parent
BASE = HERE.parent
REPO = HERE.parents[3]
probe = json.loads((HERE / 'runtime-probe.json').read_text(encoding='utf-8'))
venv = BASE / 'training-env/venv'
python_base = Path(probe['base_prefix'])
driver = next(Path(p).parent for p in probe['loaded_modules'] if Path(p).name.lower() == 'nvcuda64.dll')
trees = [venv / 'Lib/site-packages', python_base / 'Lib', python_base / 'DLLs', driver,
         REPO / 'tools/retrieval-train', BASE / 'minilm-training-source']
paths = set()
for tree in trees:
    paths.update(p.resolve() for p in tree.rglob('*') if p.is_file() and '__pycache__' not in p.parts and p.suffix not in {'.pyc', '.pyo'})
paths.update(Path(p).resolve() for p in probe['loaded_modules'])
paths.update(p.resolve() for p in python_base.glob('*.dll'))
paths.update([Path(probe['executable']).resolve(), python_base / 'python.exe', venv / 'pyvenv.cfg', HERE / 'runtime-probe.json',
              HERE / 'probe.py', Path(__file__).resolve()])
role_paths = {'base_weights': BASE / 'minilm-training-source/model.safetensors',
              'base_config': BASE / 'minilm-training-source/config.json',
              'tokenizer': BASE / 'minilm-training-source/tokenizer.json',
              'corpus': BASE / 'human-corpus-r1/corpus.private.json',
              'requests': BASE / 'requests-train-rights/pairs.private.json',
              'master315': BASE / 'a1-visible-prefix-mining-candidate-r1/candidate-training-rows.private.json',
              'candidate_protocol': BASE / 'a1-visible-prefix-mining-candidate-r1/candidate-protocol.json',
              'trainer': REPO / 'tools/retrieval-train/train_encoder.py',
              'execution_module': REPO / 'tools/retrieval-train/training_execution.py',
              'venv_launcher': Path(probe['executable']), 'base_interpreter': python_base / 'python.exe'}
paths.update(p.resolve() for p in role_paths.values())
paths.add(Path(probe['nvidia_smi']).resolve())


def pin(path):
    path = path.resolve()
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1048576), b''):
            value.update(block)
    return {'path': str(path), 'sha256': value.hexdigest(), 'bytes': path.stat().st_size}


files = []
for index, path in enumerate(sorted(paths, key=lambda p: str(p).casefold())):
    files.append(pin(path))
    if index % 5000 == 0:
        print(json.dumps({'hashed_files': index, 'elapsed_seconds': time.monotonic() - START}), flush=True)
lookup = {r['path']: r for r in files}
roles = {name: lookup[str(p.resolve())] for name, p in role_paths.items()}
gpu_fields = [p.strip() for p in probe['gpu_csv'].split(',')]
gpu = {'index': int(gpu_fields[0]), 'uuid': gpu_fields[1], 'name': gpu_fields[2], 'driver_version': gpu_fields[3],
       'total_mib': int(gpu_fields[4]), 'used_mib': int(gpu_fields[5]), 'free_mib': int(gpu_fields[6]),
       'compute_capability': [int(v) for v in gpu_fields[7].split('.')]}
cache = str((HERE / 'disabled-pycache').resolve())
environment = {'PYTHONDONTWRITEBYTECODE': '1', 'PYTHONNOUSERSITE': '1', 'PYTHONHASHSEED': '1908',
               'PYTHONPYCACHEPREFIX': cache, 'PYTHONPATH': '', 'CUDA_VISIBLE_DEVICES': '0', 'CUDA_DEVICE_ORDER': 'PCI_BUS_ID',
               'CUDA_CACHE_DISABLE': '1', 'CUBLAS_WORKSPACE_CONFIG': ':4096:8', 'HF_HUB_OFFLINE': '1',
               'TRANSFORMERS_OFFLINE': '1', 'HF_DATASETS_OFFLINE': '1', 'TOKENIZERS_PARALLELISM': 'false',
               'RAYON_NUM_THREADS': '1', 'OMP_NUM_THREADS': '2', 'MKL_NUM_THREADS': '2', 'OPENBLAS_NUM_THREADS': '1',
               'NUMEXPR_NUM_THREADS': '1', 'PATH': os.environ['PATH']}
seal_path = HERE / 'training-execution-seal.json'
command = [str(Path(probe['executable']).resolve()), '-B', '-X', 'pycache_prefix=' + cache,
           str(REPO / 'tools/retrieval-train/train_encoder.py'), '--corpus', str(role_paths['corpus'].resolve()),
           '--source', str((BASE / 'minilm-training-source').resolve()), '--additional-train', str(role_paths['requests'].resolve()),
           '--training-recipe', 'visible-prefix-mining-v1', '--training-master', str(role_paths['master315'].resolve()),
           '--training-seal', str(seal_path.resolve()), '--training-seal-sha256', '<SEAL_SHA256>',
           '--output', str((BASE / 'minilm-visible-prefix-mining-r1').resolve())]
value = {'schema': 'airk.private-minilm-training-execution-seal/1', 'status': 'PREPARED_ONLY_AWAIT_PRIMARY_GPU_RELEASE',
         'files': files, 'runtime_trees': [str(p.resolve()) for p in trees], 'file_roles': roles,
         'python_version': probe['python_version'], 'packages': probe['packages'], 'torch': probe['torch'],
         'cuda_driver_api_version': probe['cuda_driver_api_version'], 'nvidia_smi': probe['nvidia_smi'], 'gpu': gpu,
         'platform': platform.platform(), 'environment': environment, 'command': command, 'cwd': str(REPO),
         'launch_log_dir': str((HERE / 'training-run').resolve()),
         'limits': {'wall_seconds': 300, 'host_committed_bytes': 8 * 1024**3, 'cuda_allocator_bytes': 4 * 1024**3,
                    'minimum_gpu_free_mib': 8192, 'owned_process_slots': 2},
         'process_ownership': 'Pinned Windows venv redirector plus pinned actual basePython in ONE aggregate8GiB Job, kill-on-close, no third worker. StdinGO gate beforeTorchimports, assigned beforerelease. No unrelated PID kills, userOllama/AIRK/desktop untouched.',
         'desktop_gpu_disclosure': 'Observed~9.7GiB desktopGPUuse; NOT idle/exclusive or unknownPIDactivity proof. Onlyone newowned trainingtrial authorizedafterprimarybaton; CUDAallocator4GiB bound excludes driver/context overhead.',
         'candidate': {'rows': 315, 'master_sha256': roles['master315']['sha256'], 'epochs': 3, 'steps_each': 40,
                       'selected_checkpoint': 'epoch1ONLY; fixed before any DEV model scores; epochs2/3 never checkpointsearch',
                       'no_old205_or_newCAL_FINAL_content_or_outcome_use': True},
         'bytecode_policy': 'Fresh nonexistent pycache_prefix plus-B; no existingpyc read andno cacheswritten. Rootmustnotcreatepopulatecachepath.',
         'runtime_probe': 'ActualTorch/HF imports+CUDAversion/driver metadata only, CUDAinitializedtrue; no model/weights/tensorconstruction/forward/gradients; probeprocess exit0. All packageDLLs, actualloaded systemimages and completeactualNVIDIA driverfolder sealed.',
         'prepared_seconds': time.monotonic() - START, 'runtime_file_count': len(files), 'runtime_bytes': sum(r['bytes'] for r in files),
         'not_run': 'Training/model/inference/gradient/export/native/DEVscoring/Cargo NOT launched. Seal is not GPU release or qualification.'}
with seal_path.open('w', encoding='utf-8') as stream:
    json.dump(value, stream, separators=(',', ':'))
    stream.write('\n')
print(json.dumps({'seal_sha256': pin(seal_path)['sha256'], 'files': len(files), 'bytes': value['runtime_bytes'],
                  'elapsed_seconds': value['prepared_seconds'], 'training_launched': False}), flush=True)
