# SPDX-License-Identifier: Apache-2.0
"""One reviewed local training process; bounded ownership, never auto-retry."""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def digest(path):
    result = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1048576), b''):
            result.update(block)
    return result.hexdigest()


def runtime_paths(root):
    root = Path(root).resolve()
    result = set()
    stack = [str(root)]
    while stack:
        with os.scandir(stack.pop()) as entries:
            for entry in entries:
                if entry.name == '__pycache__':
                    continue
                metadata = entry.stat(follow_symlinks=False)
                if entry.is_symlink() or getattr(metadata, 'st_file_attributes', 0) & 0x400:
                    raise ValueError('runtime alias/reparse point refused')
                if entry.is_dir(follow_symlinks=False):
                    stack.append(entry.path)
                elif entry.is_file(follow_symlinks=False):
                    if Path(entry.name).suffix not in {'.pyc', '.pyo'}:
                        result.add(os.path.normcase(entry.path))
                else:
                    raise ValueError('runtime special file refused')
    return result


def verify_seal(path, expected, wait_for_gate=False):
    if digest(path) != expected:
        raise ValueError('reviewed training execution seal changed')
    value = json.loads(Path(path).read_text(encoding='utf-8'))
    if value.get('schema') != 'airk.private-minilm-training-execution-seal/1':
        raise ValueError('wrong execution seal schema')
    if wait_for_gate:
        if os.environ.get('AIRK_TRAIN_START_GATE') != '1' or sys.stdin.readline().strip() != 'GO ' + expected:
            raise ValueError('owned bounded training gate missing')
    for key, required in value['environment'].items():
        if os.environ.get(key) != required:
            raise ValueError('training environment changed: ' + key)
    if any(Path(value['environment']['PYTHONPYCACHEPREFIX']).glob('**/*')):
        raise ValueError('unexpected executable bytecode cache')
    required = {'base_weights': '53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db',
                'tokenizer': 'be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037',
                'corpus': '6e9d8ade6f799ed2a5e1903de69482db9fcd3cb1731a5fee4a504adede5fca6f',
                'requests': '916b398de0fc9774041fe174d0e585c862bc68e9aa18fa04028c3126f693a18f',
                'master315': '314825dccad822874d3405305df789317c8e90a0a0e4cdcdb590acd27a555c48',
                'candidate_protocol': 'ab86b6cfd996d8f19b6c1891d275c85bee74f92d62efb234db251d7c45218469'}
    indexed = {os.path.normcase(r['path']): r['sha256'] for r in value['files']}
    for name, expected_hash in required.items():
        role = value['file_roles'][name]
        if role['sha256'] != expected_hash or indexed.get(os.path.normcase(str(Path(role['path']).resolve()))) != expected_hash:
            raise ValueError('missing/wrong mandatory training input: ' + name)
    for tree in value['runtime_trees']:
        root = Path(tree).resolve()
        actual = runtime_paths(root)
        expected_paths = {p for p in indexed if Path(p).is_relative_to(root)}
        if actual != expected_paths:
            raise ValueError('runtime source closure changed: ' + str(root))
    for item in value['files']:
        if digest(item['path']) != item['sha256']:
            raise ValueError('training source/runtime/input drift: ' + item['path'])
    if not {'torch', 'transformers', 'safetensors', 'tokenizers', 'numpy'} <= set(value['packages']):
        raise ValueError('mandatory actual training package versions missing')
    if {name: importlib.metadata.version(name) for name in value['packages']} != value['packages']:
        raise ValueError('actual training package versions changed')
    if sys.version != value['python_version']:
        raise ValueError('actual Python runtime changed')
    return value


def verify_torch_runtime(torch, path):
    value = json.loads(Path(path).read_text(encoding='utf-8'))
    if str(torch.__version__) != value['torch']['version'] or str(torch.version.cuda) != value['torch']['cuda'] or torch.backends.cudnn.version() != value['torch']['cudnn']:
        raise ValueError('actual Torch/CUDA/cuDNN versions changed')
    props = torch.cuda.get_device_properties(0)
    if props.name != value['gpu']['name'] or [props.major, props.minor] != value['gpu']['compute_capability']:
        raise ValueError('actual training GPU changed')
    # Bounds allocator use only; driver/context overhead is reported separately.
    torch.cuda.set_per_process_memory_fraction(value['limits']['cuda_allocator_bytes'] / props.total_memory, 0)


class Basic(ctypes.Structure):
    _fields_ = [('process_time', ctypes.c_longlong), ('job_time', ctypes.c_longlong), ('flags', wintypes.DWORD),
                ('min_working_set', ctypes.c_size_t), ('max_working_set', ctypes.c_size_t), ('active', wintypes.DWORD),
                ('affinity', ctypes.c_size_t), ('priority', wintypes.DWORD), ('scheduling', wintypes.DWORD)]


class IO(ctypes.Structure):
    _fields_ = [(name, ctypes.c_ulonglong) for name in ('read_ops', 'write_ops', 'other_ops', 'read_bytes', 'write_bytes', 'other_bytes')]


class Extended(ctypes.Structure):
    _fields_ = [('basic', Basic), ('io', IO), ('process_memory', ctypes.c_size_t), ('job_memory', ctypes.c_size_t),
                ('peak_process', ctypes.c_size_t), ('peak_job', ctypes.c_size_t)]


def gpu_observation(executable):
    result = subprocess.run([executable, '--query-gpu=index,uuid,name,driver_version,memory.total,memory.used,memory.free,compute_cap',
                             '--format=csv,noheader,nounits'], capture_output=True, text=True, check=True, timeout=10)
    fields = [p.strip() for p in result.stdout.strip().split(',')]
    if len(fields) != 8 or fields[0] != '0':
        raise ValueError('one explicitly reviewed GPU0 required')
    return {'index': 0, 'uuid': fields[1], 'name': fields[2], 'driver_version': fields[3],
            'total_mib': int(fields[4]), 'used_mib': int(fields[5]), 'free_mib': int(fields[6]),
            'compute_capability': [int(p) for p in fields[7].split('.')]}


def launch(args):
    if digest(args.seal) != args.seal_sha256:
        raise ValueError('training seal differs from primary-reviewed hash')
    seal = json.loads(args.seal.read_text(encoding='utf-8'))
    output = args.log_dir.resolve()
    if output != Path(seal['launch_log_dir']).resolve() or not output.is_relative_to(Path(__file__).resolve().parents[2] / 'artifacts'):
        raise ValueError('training log output outside reviewed private directory')
    if seal['limits']['wall_seconds'] > 600 or seal['limits']['host_committed_bytes'] > 8 * 1024**3 or seal['limits']['cuda_allocator_bytes'] > 4 * 1024**3:
        raise ValueError('reviewed one-trial resource ceiling exceeded')
    if output.exists():
        raise ValueError('fresh private training log directory required')
    baseline = gpu_observation(seal['nvidia_smi'])
    if any(baseline[k] != seal['gpu'][k] for k in ('uuid', 'name', 'driver_version', 'compute_capability')):
        raise ValueError('GPU/driver identity changed before launch')
    if baseline['free_mib'] < seal['limits']['minimum_gpu_free_mib']:
        raise ValueError('insufficient reviewed GPU free-memory reserve')
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.CreateJobObjectW.restype = wintypes.HANDLE
    kernel.SetInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD]
    kernel.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
    kernel.QueryInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD, ctypes.c_void_p]
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    job = kernel.CreateJobObjectW(None, None)
    limits = Extended()
    limits.basic.flags = 0x100 | 0x200 | 0x2000 | 0x8  # per-process+aggregate memory, kill-on-close, two pinned interpreter stages
    limits.basic.active = 2  # Windows venv redirector plus actual base interpreter; no third worker.
    limits.process_memory = seal['limits']['host_committed_bytes']
    limits.job_memory = seal['limits']['host_committed_bytes']
    if not job or not kernel.SetInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits)):
        raise ValueError('cannot establish bounded owned training JobObject')
    env = dict(os.environ)
    env.update(seal['environment'])
    env['AIRK_TRAIN_START_GATE'] = '1'
    command = [part.replace('<SEAL_SHA256>', args.seal_sha256) for part in seal['command']]
    output.mkdir(parents=True)
    started = time.monotonic()
    child = None
    attached = False
    status = 'unknown'
    exit_code = None
    peak_before_close = None
    try:
        with (output / 'stdout.log').open('wb') as stdout, (output / 'stderr.log').open('wb') as stderr:
            child = subprocess.Popen(command, cwd=seal['cwd'], env=env, stdin=subprocess.PIPE, stdout=stdout, stderr=stderr)
            if not kernel.AssignProcessToJobObject(job, wintypes.HANDLE(int(child._handle))):
                raise ValueError('cannot attach owned child to bounded JobObject')
            attached = True
            child.stdin.write(('GO ' + args.seal_sha256 + '\n').encode())
            child.stdin.close()
            # Observed process exit under deadline; no fixed sleeps/retries.
            try:
                exit_code = child.wait(timeout=seal['limits']['wall_seconds'])
                status = 'completed' if exit_code == 0 else 'failed'
            except subprocess.TimeoutExpired:
                status = 'deadline_exhausted'
                before_close = Extended()
                if kernel.QueryInformationJobObject(job, 9, ctypes.byref(before_close), ctypes.sizeof(before_close), None):
                    peak_before_close = before_close.peak_job
                kernel.CloseHandle(job)
                job = None
                exit_code = child.wait(timeout=30)
    finally:
        if child is not None and child.poll() is None:
            # Only this exact owned process. No installed service or unrelated PID is touched.
            if attached and job is not None:
                kernel.CloseHandle(job)
                job = None
            else:
                child.kill()
            exit_code = child.wait(timeout=30)
        observed = Extended()
        peak = peak_before_close
        if job is not None:
            if kernel.QueryInformationJobObject(job, 9, ctypes.byref(observed), ctypes.sizeof(observed), None):
                peak = observed.peak_job
            kernel.CloseHandle(job)
        receipt = {'schema': 'airk.private-bounded-training-execution/1', 'status': status, 'exit_code': exit_code,
                   'owned_pid': child.pid if child else None, 'child_exit_observed': child is not None and child.poll() is not None,
                   'seal_sha256': args.seal_sha256, 'command': command, 'baseline_gpu': baseline,
                   'baseline_is_not_idle_or_exclusive_gpu_claim': True, 'elapsed_seconds': time.monotonic() - started,
                   'peak_host_committed_bytes': peak, 'limits': seal['limits'],
                   'bounded_process_slots': 2, 'process_roster': 'pinned venv redirector and base interpreter only',
                   'host_limit_is_aggregate': True, 'other_processes_touched': False, 'no_automatic_retry': True}
        (output / 'execution-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
    if exit_code != 0 or status != 'completed':
        raise RuntimeError('bounded training did not complete; retained logs/partial checkpoint are not a pass')
    print(json.dumps(receipt))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seal', type=Path, required=True)
    parser.add_argument('--seal-sha256', required=True)
    parser.add_argument('--log-dir', type=Path, required=True)
    launch(parser.parse_args())


if __name__ == '__main__':
    main()
