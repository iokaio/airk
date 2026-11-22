# SPDX-License-Identifier: Apache-2.0
"""Seal an owned probe or run plan for `answer_worker.py`. Never dispatches a model.

The plan carries every key `../focus_lora_r8/launch_owned.py` and its
`workflow.continuity_key` read, so that launcher is reused unchanged.
"""
import argparse, hashlib, importlib.util, json, math, os, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = next(p for p in HERE.parents if (p / 'Cargo.toml').is_file() and (p / 'AGENTS.md').is_file())
PILOT = HERE.parent / 'focus_lora_r8'
REFERENCE = ROOT / 'scripts/arbiter/export/qwen/qwen3-reranker06-experiment-r1'
OWNER = ROOT / 'scripts/cuda-native-execution/helpers/reference_owned.py'
SITE = ROOT / 'artifacts/advanced-split/models/training-env/venv/Lib/site-packages'
HUMAN = 'S-1-5-21-3678370870-3858642417-826968105-1002'
VERSIONS = {'torch': '2.10.0+cu128', 'transformers': '5.10.1', 'safetensors': '0.8.0', 'numpy': '2.3.2'}


def digest(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def pin(path):
    path = Path(path).resolve()
    return {'path': str(path), 'bytes': path.stat().st_size, 'sha256': digest(path)}


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.path.insert(0, str(Path(path).parent))
    spec.loader.exec_module(module)
    return module


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--mode', choices=['probe', 'run'], required=True)
    p.add_argument('--examples', type=Path, required=True)
    p.add_argument('--log-dir', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--wall-seconds', type=int, required=True)
    p.add_argument('--probe-rows', type=int, default=64)
    p.add_argument('--microbatch', type=int, default=8)
    p.add_argument('--lr', type=float, default=5e-5)
    p.add_argument('--base-python', type=Path, default=os.environ.get('AIRK_FOCUS_BASE_PYTHON'),
                   help='the sealed base interpreter (or AIRK_FOCUS_BASE_PYTHON); the worker child runs it with -B -S')
    args = p.parse_args(argv)
    if args.base_python is None or not Path(args.base_python).is_absolute() or not Path(args.base_python).is_file():
        raise SystemExit('make_plan: an absolute --base-python (or AIRK_FOCUS_BASE_PYTHON) is required')
    log_dir, output = args.log_dir.resolve(), args.output.resolve()
    if log_dir.exists() or output.exists() or not log_dir.is_relative_to(ROOT / 'artifacts/v190/final-plan') or not output.is_relative_to(ROOT / 'artifacts'):
        raise SystemExit('make_plan: fresh plan and log paths under artifacts required')
    launcher = load(PILOT / 'launch_owned.py', 'pilot_launch_owned')
    workflow = sys.modules['workflow']
    owner = load(OWNER, 'reviewed_owner_for_gpu')
    gpu = owner.gpu_observation(str(Path(os.environ.get('SystemRoot', r'C:\Windows')) / 'System32/nvidia-smi.exe'))
    examples = args.examples.resolve()
    train, evaluation, manifest = examples / 'train.jsonl', examples / 'eval.jsonl', examples / 'manifest.json'
    recorded = json.loads(manifest.read_bytes())['outputs']
    for name, path in (('train.jsonl', train), ('eval.jsonl', evaluation)):
        if digest(path) != recorded[name]:
            raise SystemExit('make_plan: example file differs from its manifest')
    rows = sum(1 for line in train.read_text(encoding='utf-8').splitlines() if line.strip())
    steps = 2 if args.mode == 'probe' else math.ceil(rows / 32)
    sources = {}
    for path in [*HERE.glob('*.py'), *PILOT.glob('*.py'), PILOT / 'support/candidate_overlay.py', *REFERENCE.glob('*.py'), OWNER]:
        sources[str(path.resolve())] = pin(path)
    for path in (train, evaluation, manifest):
        sources[str(path)] = pin(path)
    seals = {v['path']: v['sha256'] for v in sources.values()}
    environment = {'PYTHONPYCACHEPREFIX': launcher.FRESH_PYCACHE, 'PYTHONDONTWRITEBYTECODE': '1', 'PYTHONIOENCODING': 'utf-8',
                   'PYTHONPATH': os.pathsep.join([str(HERE), str(PILOT), str(REFERENCE), str(SITE)]),
                   'CUBLAS_WORKSPACE_CONFIG': ':4096:8', 'CUDA_MODULE_LOADING': 'LAZY', 'HF_HUB_OFFLINE': '1',
                   'TRANSFORMERS_OFFLINE': '1', 'HF_HUB_DISABLE_TELEMETRY': '1', 'TORCH_HOME': str(log_dir / 'cache/torch'),
                   'CUDA_CACHE_PATH': str(log_dir / 'cache/cuda'), 'TOKENIZERS_PARALLELISM': 'false'}
    plan = {'schema': 'airk.private.focus-answer-eval-owned-plan/1', 'mode': args.mode, 'human_sid': HUMAN,
            'repository_root': str(ROOT), 'native_census': pin(train), 'native_owned_receipt': None, 'cohort': pin(evaluation),
            'replay': pin(manifest), 'train_examples': pin(train), 'eval_examples': pin(evaluation),
            'general_replay_instruction': 'publisher', 'width': 1024, 'score_batch': 16,
            'max_rows': args.probe_rows if args.mode == 'probe' else None, 'max_optimizer_steps': steps,
            'gpu': gpu, 'nvidia_smi': str(Path(os.environ.get('SystemRoot', r'C:\Windows')) / 'System32/nvidia-smi.exe'),
            'log_dir': str(log_dir), 'worker_output': str(log_dir / 'worker'), 'environment': environment, 'versions': VERSIONS,
            'base_python': pin(args.base_python), 'reference_module': pin(REFERENCE / 'run_focus_export.py'),
            'reference_worker': None, 'pilot_worker': pin(PILOT / 'focus_worker.py'), 'ownership_module': pin(OWNER),
            'human_identity_helper': pin(PILOT / 'support/candidate_overlay.py'), 'worker': pin(HERE / 'answer_worker.py'),
            'execution_sources': sources, 'input_seals': seals, 'immutable_input_seals': seals,
            'limits': {'wall_seconds': args.wall_seconds, 'host_committed_bytes': 20 * 1024**3,
                       'cuda_allocator_bytes': 32 * 1024**3, 'minimum_cuda_free_bytes': 34 * 1024**3},
            'qualification': False, 'promotion': False, 'original_seed': 1908,
            'optimizer': {'rank': 16, 'alpha': 16, 'lr': args.lr, 'effective_batch': 32, 'microbatch': args.microbatch,
                          'epochs': 1, 'dropout': 0, 'weight_decay': 0.0}}
    plan['continuity_key'] = workflow.continuity_key(plan)
    output.write_text(json.dumps(plan, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps(pin(output)))


if __name__ == '__main__':
    main()
