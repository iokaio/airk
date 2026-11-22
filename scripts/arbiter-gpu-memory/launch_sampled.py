# SPDX-License-Identifier: Apache-2.0
"""Launch one child and sample its Windows GPU memory (plan 12.5.2, L5 phase A).

Uses the sampled observer in ``../arbiter-gpu-memory.py`` on the ORIGINAL
child HANDLE retained by this parent. Each ``--luid`` names one adapter LUID
explicitly (``high:low`` hexadecimal, physical index 0); every named adapter
is sampled and reported separately, so no instance is guessed. Samples are
sampled current usage, never a lifetime peak, and grant no admission.

Outputs go to a fresh ``--output-dir``: ``child.log`` (stdout and stderr),
``samples.jsonl`` (every sample) and ``summary.json``. The child inherits this
process's environment plus each ``--env KEY=VALUE``. Run as the verified human.
"""
import argparse, hashlib, importlib.util, json, os, pathlib, subprocess, sys, time


def observer():
    path = pathlib.Path(__file__).resolve().parents[1] / 'arbiter-gpu-memory.py'
    spec = importlib.util.spec_from_file_location('airk_gpu_memory', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module, hashlib.sha256(path.read_bytes()).hexdigest()


def file_sha(path):
    h = hashlib.sha256()
    with open(path, 'rb') as handle:
        while chunk := handle.read(4 << 20):
            h.update(chunk)
    return h.hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output-dir', type=pathlib.Path, required=True)
    p.add_argument('--luid', action='append', required=True, help='high:low hexadecimal adapter LUID')
    p.add_argument('--interval', type=float, default=0.1)
    p.add_argument('--deadline-s', type=float, default=900.0)
    p.add_argument('--env', action='append', default=[])
    p.add_argument('--cwd', type=pathlib.Path)
    p.add_argument('command', nargs=argparse.REMAINDER)
    args = p.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command:
        raise SystemExit('child command required after --')
    # Only the child process itself is sampled, so the child must be the measured image,
    # never a cargo/nextest wrapper whose grandchild would hold the GPU memory.
    image = pathlib.Path(command[0])
    if (not image.is_file() or image.suffix.lower() != '.exe'
            or image.stem.lower() in ('cargo', 'cargo-nextest', 'nextest')):
        raise SystemExit('the child must be the measured test image file, not a wrapper')
    image_sha = file_sha(image)
    args.output_dir.mkdir(exist_ok=False)
    module, observer_sha = observer()
    api = module.WindowsApi()
    environment = dict(os.environ)
    for item in args.env:
        key, _, value = item.partition('=')
        environment[key] = value
    started_wall = time.time()
    started = time.monotonic()
    deadline = started + args.deadline_s
    log = (args.output_dir / 'child.log').open('xb')
    child = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, env=environment,
                             cwd=str(args.cwd) if args.cwd else None)
    handle = int(child._handle)
    identity = api.process_state(handle).identity
    samplers = {}
    for text in args.luid:
        high, low = (int(x, 16) for x in text.split(':'))
        samplers[text] = module.Sampler(api, handle, identity, module.AdapterIdentity(low, high, 0), deadline)
    samples = (args.output_dir / 'samples.jsonl').open('x', encoding='utf-8')
    timed_out = False
    while child.poll() is None:
        if time.monotonic() >= deadline:
            timed_out = True
            child.kill()
            break
        for name, sampler in samplers.items():
            record = sampler.sample()
            record['luid'] = name
            record['t'] = time.monotonic() - started
            samples.write(json.dumps(record) + '\n')
        time.sleep(args.interval)
    code = child.wait()
    # Final attempt at exit; the observer marks it invalid once the process is gone.
    for name, sampler in samplers.items():
        record = sampler.sample()
        record['luid'] = name
        record['t'] = time.monotonic() - started
        record['final_at_exit'] = True
        samples.write(json.dumps(record) + '\n')
    samples.close()
    log.close()
    summary = {
        'schema': 'airk.private.l5a-sampled-launch/1',
        'command': command, 'extra_env': args.env,
        'image_sha256': image_sha, 'image_is_test_binary': True, 'sampled_scope': 'child process only',
        'observer_sha256': observer_sha,
        'launcher_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
        'pid': identity.pid, 'exit_code': code, 'timed_out': timed_out,
        'started_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(started_wall)),
        'elapsed_s': round(time.monotonic() - started, 3), 'interval_s': args.interval,
        'adapters': {name: {**s.summary(), 'close': s.close()} for name, s in samplers.items()},
        'lifetime_peak_bytes': None, 'admission_authority': False,
    }
    with (args.output_dir / 'summary.json').open('x', encoding='utf-8') as handle:
        json.dump(summary, handle, indent=2)
        handle.write('\n')
    print(json.dumps({'exit_code': code, 'pid': identity.pid,
                      'sampled_max': {k: v['sampled_max_current_usage_bytes'] for k, v in summary['adapters'].items()}}))
    sys.exit(code)


if __name__ == '__main__':
    main()
