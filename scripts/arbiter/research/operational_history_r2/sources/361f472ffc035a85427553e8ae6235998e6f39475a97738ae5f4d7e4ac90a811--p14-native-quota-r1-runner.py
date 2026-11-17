# SPDX-License-Identifier: Apache-2.0
"""Run an explicitly frozen P14 command plan in a new owned Neo source snapshot.

Transfer this file, source.tar.gz, snapshot.json and plan.json into a new
~/airk-builds/p14-native-* directory. It does not connect to SSH, access signing
credentials, alter installed AIRK or dispatch hosted CI. The parent owns queues.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import resource
import subprocess
import tarfile
import time
import tempfile
import re


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    cache_mode = parser.add_mutually_exclusive_group(required=True)
    cache_mode.add_argument('--fresh', action='store_true', help='Leave a new run/target absent for Cargo to create; reuse no compiler target')
    cache_mode.add_argument('--cache', type=Path, help='Optional existing target to clone with APFS copy-on-write')
    parser.add_argument('--inputs', type=Path, help='Preserved input base containing local-vector-model and local-onnx-runtime; required with --fresh')
    parser.add_argument('--clean-receipt', type=Path, action='append', default=[],
                        help='Retain path/hash of an actual cargo-clean receipt, without inventing coverage')
    parser.add_argument('--tools', type=Path, required=True)
    args = parser.parse_args()
    assert platform.system() == 'Darwin' and platform.machine() == 'arm64'
    run = Path(__file__).resolve().parent
    assert run.parent == Path.home() / 'airk-builds' and run.name.startswith('p14-native-')
    os.umask(0o077)
    if args.fresh and args.inputs is None:
        parser.error('--fresh requires --inputs with preserved offline model/runtime inputs')
    seed_cache = args.cache.resolve(strict=True) if args.cache else None
    tools = args.tools.resolve(strict=True)
    assert tools.is_dir() and (seed_cache is None or seed_cache.is_dir())
    clean_receipts = [{'path': str(path.resolve(strict=True)), 'sha256': digest(path.resolve(strict=True))}
                      for path in args.clean_receipt]
    original_soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
    assert hard == resource.RLIM_INFINITY or hard >= 4096, '64-channel fixtures need supported descriptor capacity'
    resource.setrlimit(resource.RLIMIT_NOFILE, (4096, hard))
    record = json.loads((run / 'snapshot.json').read_text(encoding='utf-8'))
    plan = json.loads((run / 'plan.json').read_text(encoding='utf-8'))
    assert plan['phase'] == 'P14' and plan['working_version'] == '1.8.8'
    assert plan.get('draft') is not True and not plan.get('pending'), 'Finalize the reviewed P14 plan before execution'
    assert plan['steps'] and all(isinstance(row['command'], list) and row['command'] for row in plan['steps'])
    names = [row['name'] for row in plan['steps']]
    assert len(names) == len(set(names))
    assert all(name and all(character.isascii() and (character.isalnum() or character in '-_')
                            for character in name) for name in names)
    archive_path, source = run / 'source.tar.gz', run / 'source'
    assert digest(archive_path) == record['archive_sha256']
    source.mkdir(mode=0o700)
    with tarfile.open(archive_path) as archive:
        members = archive.getmembers()
        assert len(members) == len(record['files'])
        assert {member.name for member in members} == set(record['files'])
        for member in members:
            assert member.isfile() and not Path(member.name).is_absolute()
            assert (source / member.name).resolve().is_relative_to(source)
        archive.extractall(source, filter='data')

    def verify():
        for name, expected in record['files'].items():
            assert digest(source / name) == expected, name
        return {'files': len(record['files']), 'unchanged': True}

    before = verify()
    subprocess.run(['git', 'init', '--quiet'], cwd=source, check=True)
    subprocess.run(['git', 'add', '--all'], cwd=source, check=True)
    spec = importlib.util.spec_from_file_location('p9_runtime_builder', source / 'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    native_source = builder.source_identity()
    assert native_source == record['native_source'], 'Transferred native source differs from frozen Windows source'
    # Never repopulate a requested clean target from a historical target unless
    # the operator explicitly selects --cache. Preserved inputs are separate.
    cache = run / 'target'
    assert not cache.exists()
    if args.fresh:
        cache_observation = {'isolated': str(cache), 'method': 'fresh target; left absent for Cargo first creation',
                             'target_absent_before_commands': True, 'seed': None}
    else:
        clone_started = time.monotonic()
        subprocess.run(['/bin/cp', '-cR', str(seed_cache), str(cache)], check=True)
        cache_observation = {'seed': str(seed_cache), 'isolated': str(cache),
                             'method': 'APFS cp -cR copy-on-write clone',
                             'seconds': round(time.monotonic() - clone_started, 2)}
    inputs = args.inputs.resolve(strict=True) if args.inputs else cache
    models, onnx_runtime = inputs / 'local-vector-model', inputs / 'local-onnx-runtime'
    assert models.is_dir() and onnx_runtime.is_dir()
    assert (models / 'minilm-l6-v2-s8-v1/model.json').is_file(), 'Preserved ARM64 model inputs are missing'
    private = run / 'private-census'
    private.mkdir(mode=0o700)
    environment = dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_NET_OFFLINE='true', CARGO_INCREMENTAL='0',
                       MACOSX_DEPLOYMENT_TARGET='13.3', PYTHONUTF8='1', CARGO_TARGET_DIR=str(cache),
                       AIRK_BUNDLED_MODEL_DIR=str(models),
                       AIRK_BUNDLED_RUNTIME_DIR=str(onnx_runtime),
                       AIRK_TEST_MODEL_DIR=str(models / 'minilm-l6-v2-s8-v1'),
                       AIRK_PRIVATE_CENSUS_BASE=str(private))
    environment.pop('AIRK_TEST_BINARY', None)
    environment.pop('AIRK_CATALOG_CANDIDATE_BINARY', None)
    environment.pop('AIRK_P10_RESOURCE_OUTPUT_DIR', None)
    environment.pop('AIRK_P11B_RESOURCE_OUTPUT_DIR', None)
    environment['TMPDIR'] = str(Path(tempfile.gettempdir()).resolve(strict=True))
    ignore_base = private / 'owned-ignore-controls'
    ignore_base.mkdir(mode=0o700)
    ignore_file = ignore_base / 'empty.ignore'
    ignore_file.write_bytes(b'')
    ignore_config = ignore_base / 'global.gitconfig'
    ignore_config.write_text('[core]\nexcludesFile = "' + str(ignore_file) + '"\n', encoding='utf-8')
    environment['GIT_CONFIG_GLOBAL'] = str(ignore_config)
    environment['PATH'] = str(tools) + ':' + str(Path.home() / '.cargo/bin') + ':' + environment.get('PATH', '')
    steps = []
    for row in plan['steps']:
        if (run / 'runtime-debug/airk').is_file():
            environment['AIRK_TEST_BINARY'] = str(run / 'runtime-debug/airk')
        command = [item.replace('{run}', str(run)).replace('{source}', str(source)) for item in row['command']]
        (run / 'phase.txt').write_text(row['name'], encoding='utf-8')
        started = time.monotonic()
        with (run / (row['name'] + '.log')).open('x', encoding='utf-8') as output:
            result = subprocess.run(command, cwd=source, env=environment, stdout=output,
                                    stderr=subprocess.STDOUT)
        actual_exit_code = result.returncode
        passing_tests = None
        validation_error = None
        if command[:2] == ['cargo', 'test']:
            observed = (run / (row['name'] + '.log')).read_text(encoding='utf-8')
            passing_tests = sum(map(int, re.findall(r'test result: (?:ok|FAILED)[.] (\d+) passed', observed)))
            if result.returncode == 0 and passing_tests == 0:
                validation_error = 'Cargo test filter matched no passing tests'
                result.returncode = 1
        if result.returncode == 0 and row.get('expected_passing_tests') is not None and passing_tests != row['expected_passing_tests']:
            validation_error = 'Passing test count differs from frozen plan'
            result.returncode = 1
        steps.append({'actual_exit_code': actual_exit_code, 'passing_tests': passing_tests, 'validation_error': validation_error, 'name': row['name'], 'command': command, 'exit_code': result.returncode,
                      'seconds': round(time.monotonic() - started, 2)})
        (run / 'steps.json').write_text(json.dumps(steps, indent=2) + '\n', encoding='utf-8')
        print(json.dumps(steps[-1]), flush=True)
        # Freeze after any failure; a correction gets a separately recorded run.
        if result.returncode:
            break
    after = verify()
    assert builder.source_identity() == native_source
    runtime = run / 'runtime-debug'
    payload = {}
    if runtime.is_dir():
        for name in ('airk', 'airk-embed', 'airk-build.json', 'airk-install.json', 'airk-runtime.json'):
            path = runtime / name
            if path.is_file():
                payload[name] = digest(path)
    report = {'schema': 'airk.p14.neo-qualification/1', 'phase': 'P14', 'entry_commit': record['entry_commit'],
              'archive_sha256': record['archive_sha256'], 'plan_sha256': digest(run / 'plan.json'),
              'native_source': native_source, 'platform': platform.platform(),
              'cache': cache_observation, 'clean_receipts': clean_receipts,
              'offline_build_inputs': {'base': str(inputs), 'models': str(models), 'onnx_runtime': str(onnx_runtime),
                                       'arm64_model_manifest_sha256': digest(models / 'minilm-l6-v2-s8-v1/model.json')},
              'environment': {key: environment.get(key) for key in ('TMPDIR','CARGO_TARGET_DIR','CARGO_BUILD_JOBS','CARGO_NET_OFFLINE','CARGO_INCREMENTAL',
                              'AIRK_BUNDLED_MODEL_DIR','AIRK_BUNDLED_RUNTIME_DIR','AIRK_TEST_MODEL_DIR','AIRK_PRIVATE_CENSUS_BASE','AIRK_TEST_BINARY','GIT_CONFIG_GLOBAL')},
              'logical_cpus': os.cpu_count(), 'rlimit_nofile_initial': [original_soft, hard],
              'rlimit_nofile_run': [4096, hard], 'source_before': before, 'source_after': after,
              'runtime_files': payload, 'steps': steps,
              'status': 'passed' if len(steps) == len(plan['steps']) and all(row['exit_code'] == 0 for row in steps) else 'failed',
              'not_run': plan['not_run']}
    (run / 'qualification.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    (run / 'phase.txt').write_text('complete', encoding='utf-8')
    print(json.dumps(report), flush=True)
    raise SystemExit(0 if report['status'] == 'passed' else 1)


if __name__ == '__main__':
    main()
