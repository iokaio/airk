# SPDX-License-Identifier: Apache-2.0
"""Execute only the reviewed frozen Neo plan; parent serializes Cargo ownership."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import resource
import stat
import subprocess
import sys
import tarfile
import tempfile
import time

EXCLUDED = {'.git', '.codex', '.agents', '.aws', '.mcp.json', '.ssh', 'temp', 'private',
            'index', 'indexes', 'state', 'airk-state', 'artifacts', 'target', 'node_modules', '__pycache__'}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def save(path, value):
    # This runner alone owns these live receipts. Source/archive pins are immutable.
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')


def load(source, name, relative):
    spec = importlib.util.spec_from_file_location(name, source / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    if sys.flags.optimize:
        raise RuntimeError('Qualification requires Python assertions enabled')
    assert platform.system() == 'Darwin' and platform.machine() == 'arm64'
    os.umask(0o077)
    run = Path(__file__).resolve().parent
    assert run.parent == Path.home() / 'airk-builds' and re.fullmatch(r'readiness-1817-[a-z0-9-]{1,40}', run.name)
    assert not run.is_symlink() and run.resolve(strict=True) == run
    snapshot = json.loads((run / 'snapshot.json').read_text())
    plan = json.loads((run / 'plan.json').read_text())
    assert snapshot['label'] == run.name and plan['schema'] == 'airk.readiness.neo-plan/1'
    assert set(snapshot['harness_sha256']) == {'owner.py', 'runner.py', 'dependency-fetch.py'}
    assert snapshot['public_export_preflight'] == {'canonical_native_roster_equal': True, 'tracked_fixture_omissions': []}
    assert plan['status'] == 'PREPARED_FOR_PRIMARY_REVIEW_NOT_LAUNCHED'
    assert digest(run / 'source.tar.gz') == snapshot['archive_sha256']
    assert digest(run / 'plan.json') == snapshot['plan_sha256']
    assert all(digest(run / name) == expected for name, expected in snapshot['harness_sha256'].items())
    fetched = json.loads((run / 'dependency-fetch.json').read_text())
    assert fetched['status'] == 'passed' and fetched['exit_code'] == 0
    assert fetched['archive_sha256'] == snapshot['archive_sha256']
    assert fetched['native_source'] == snapshot['native_source'] and fetched['source_unchanged']
    assert fetched['no_native_build'] and fetched['no_target_created']
    source, target = run / 'source', run / 'target'
    assert not source.exists() and not target.exists(), 'Fresh batch only; preserve earlier evidence'
    source.mkdir(mode=0o700)
    with tarfile.open(run / 'source.tar.gz') as archive:
        members = archive.getmembers()
        assert len(members) == len(snapshot['files'])
        assert {member.name for member in members} == set(snapshot['files'])
        for member in members:
            relative = PurePosixPath(member.name)
            assert member.isfile() and not relative.is_absolute() and '..' not in relative.parts
            assert not set(relative.parts) & EXCLUDED and ':' not in member.name and '\\' not in member.name
            assert member.size == snapshot['files'][member.name]['bytes']
        archive.extractall(source, filter='data')

    def verify():
        for name, expected in snapshot['files'].items():
            path = source / name
            assert path.is_file() and not path.is_symlink() and path.resolve(strict=True) == path
            assert path.stat().st_size == expected['bytes'] and digest(path) == expected['sha256'], name
        assert all(digest(run / name) == expected for name, expected in snapshot['harness_sha256'].items())
        assert digest(run / 'plan.json') == snapshot['plan_sha256']
        return {'public_files': len(snapshot['files']), 'unchanged': True}

    before = verify()
    subprocess.run(['git', 'init', '--quiet'], cwd=source, check=True)
    subprocess.run(['git', 'add', '--force', '--all'], cwd=source, check=True)
    builder = load(source, 'neo_readiness_builder', 'scripts/build-service-runtime.py')
    assert builder.source_identity() == snapshot['native_source'], 'Transferred native source differs from frozen source'
    fixtures = Path(tempfile.mkdtemp(prefix='airk-rdy-', dir='/private/tmp'))
    metadata = fixtures.lstat()
    assert fixtures.resolve(strict=True) == fixtures and metadata.st_uid == os.getuid()
    assert stat.S_IMODE(metadata.st_mode) == 0o700 and not fixtures.is_symlink()
    private = run / 'private-census'
    private.mkdir(mode=0o700)
    tools = Path(plan['tools'])
    # Remove inherited per-session redirects only from child environments.
    blocked = {'PYTHONOPTIMIZE', 'PYTHONPATH', 'PYTHONHOME', 'PYTHONSTARTUP', 'PYTHONINSPECT',
               'GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_COMMON_DIR', 'GIT_OBJECT_DIRECTORY',
               'GIT_ALTERNATE_OBJECT_DIRECTORIES', 'GIT_REPLACE_REF_BASE', 'GIT_NAMESPACE', 'GIT_CONFIG',
               'GIT_CONFIG_COUNT', 'GIT_CONFIG_PARAMETERS'}
    environment = {key: value for key, value in os.environ.items()
                   if key not in blocked and not key.startswith(('AIRK_', 'GIT_CONFIG_KEY_', 'GIT_CONFIG_VALUE_'))}
    environment.update(PATH=str(Path.home() / '.cargo/bin') + ':' + str(tools) + ':' + environment.get('PATH', ''),
                       CARGO_BUILD_JOBS='2', CARGO_NET_OFFLINE='true', CARGO_INCREMENTAL='0',
                       CARGO_TARGET_DIR=str(target), MACOSX_DEPLOYMENT_TARGET='13.3',
                       TMPDIR=str(fixtures), TEMP=str(fixtures), TMP=str(fixtures),
                       AIRK_PRIVATE_CENSUS_BASE=str(private), PYTHONNOUSERSITE='1', PYTHONDONTWRITEBYTECODE='1')
    ignore_config = run / 'fixture-gitconfig'
    ignore_file = run / 'empty.ignore'
    ignore_file.write_bytes(b'')
    ignore_config.write_text('[core]\nexcludesFile = ' + json.dumps(str(ignore_file)) + '\n')
    environment['GIT_CONFIG_GLOBAL'] = str(ignore_config)
    initial_limit, hard_limit = resource.getrlimit(resource.RLIMIT_NOFILE)
    assert hard_limit == resource.RLIM_INFINITY or hard_limit >= 4096
    resource.setrlimit(resource.RLIMIT_NOFILE, (4096, hard_limit))
    report = {'schema': 'airk.readiness.neo-qualification/1', 'native_source': snapshot['native_source'],
              'entry_commit': snapshot['entry_commit'], 'plan_sha256': snapshot['plan_sha256'],
              'archive_sha256': snapshot['archive_sha256'], 'source_before': before,
              'platform': platform.platform(), 'fixture_root': str(fixtures),
              'fixture_cleanup': 'Preserved; no deletion of failed fixtures',
              'target': str(target), 'incremental': '0', 'build_jobs': '2', 'network': 'offline',
              'rlimit_nofile_before': [initial_limit, hard_limit], 'rlimit_nofile_run': [4096, hard_limit],
              'steps': [], 'not_run': plan['not_run'], 'status': 'running'}
    save(run / 'qualification.json', report)
    for row in plan['steps']:
        verify()
        assert builder.source_identity() == snapshot['native_source']
        expected_tests = row.get('tests')
        if expected_tests is not None:
            assert expected_tests and len(expected_tests) == len(set(expected_tests))
            command = ['cargo', 'test', '-p', row['package'], '--no-default-features', '--lib', '--locked', '--timings',
                       '--', '--exact', '--test-threads=2', *expected_tests]
        else:
            replacements = {'{python}': sys.executable, '{target}': str(target)}
            command = [replacements.get(value, value) for value in row.get('command', [])]
        name = row['name']
        assert re.fullmatch(r'[a-z0-9-]+', name)
        (run / 'phase.txt').write_text(name + '\n')
        started = time.monotonic()
        code, error, actual_tests = 0, None, []
        with (run / (name + '.log')).open('x', encoding='utf-8') as output:
            try:
                if row.get('internal') == 'verify-static-inputs':
                    static_path = Path(plan['static_environment'])
                    model = Path(plan['model_base']) / 'minilm-l6-v2-s8-v1/model.json'
                    assert digest(static_path) == plan['static_environment_sha256']
                    assert digest(model) == plan['arm64_model_manifest_sha256']
                    static = load(source, 'neo_readiness_static_owner', 'scripts/static-runtime-environment.py')
                    verified = static.load_environment_file(static_path, target='aarch64-apple-darwin')
                    output.write(json.dumps({'canonical_static_validator_passed': True,
                                            'static_environment_sha256': digest(static_path),
                                            'arm64_model_manifest_sha256': digest(model),
                                            'environment_keys': sorted(verified), 'no_build_or_model_execution': True}) + '\n')
                    command = ['canonical_static_environment_validator', str(static_path)]
                else:
                    result = subprocess.run(command, cwd=source, env=environment, stdout=output, stderr=subprocess.STDOUT)
                    code = result.returncode
            except Exception as failure:
                code, error = 1, repr(failure)
                output.write(error + '\n')
        actual_code = code
        if expected_tests is not None:
            observed = (run / (name + '.log')).read_text(errors='replace')
            outcomes = re.findall(r'^test (\S+) \.\.\. (ok|FAILED|ignored)(?:,.*)?$', observed, re.M)
            actual_tests = [{'identity': identity, 'outcome': outcome} for identity, outcome in outcomes]
            if len(outcomes) != len(expected_tests) or {identity for identity, _ in outcomes} != set(expected_tests) or any(outcome != 'ok' for _, outcome in outcomes):
                code, error = code or 1, 'Executed identities/outcomes differ from reviewed nonzero selection'
        try:
            after = verify()
            assert builder.source_identity() == snapshot['native_source']
        except Exception as failure:
            code, error = 1, 'Source freeze violation: ' + repr(failure)
            after = {'unchanged': False}
        report['steps'].append({'name': name, 'command': command, 'actual_exit_code': actual_code, 'exit_code': code,
                                'seconds': round(time.monotonic() - started, 3), 'validation_error': error,
                                'selected_tests': len(actual_tests) if expected_tests is not None else None,
                                'tests': actual_tests, 'expected': row['expected']})
        report['source_after'] = after
        save(run / 'steps.json', report['steps'])
        save(run / 'qualification.json', report)
        print(json.dumps(report['steps'][-1]), flush=True)
        if code:
            report['status'] = 'failed'
            report['unrun_steps'] = [later['name'] for later in plan['steps'][len(report['steps']):]]
            save(run / 'qualification.json', report)
            return 1
    report['status'] = 'passed'
    save(run / 'qualification.json', report)
    (run / 'phase.txt').write_text('complete\n')
    return 0


if __name__ == '__main__':
    sys.exit(main())
