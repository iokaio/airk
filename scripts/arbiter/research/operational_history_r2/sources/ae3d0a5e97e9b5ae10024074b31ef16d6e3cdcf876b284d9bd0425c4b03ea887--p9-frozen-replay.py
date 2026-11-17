# SPDX-License-Identifier: Apache-2.0
"""P9 replay of the unchanged E0 workload through its historical upgrade path.

Private runner only: no normal installation, source corpus or credentials change.
The predecessor is intentionally the same one used at P8. Policy-upgrade and trim
behavior are separate native fixtures; changing that historical path would make
the steady-state comparison harder to attribute.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import uuid

ROOT = Path(__file__).resolve().parents[2]


def module(name, relative):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime-dir', type=Path, required=True)
    parser.add_argument('--previous-runtime', type=Path,
                        default=ROOT / 'artifacts/v190/p6f-resume-predecessor-shared-copy')
    parser.add_argument('--verification', choices=('off', 'report'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('Frozen predecessor-upgrade cohort is qualified on Windows only')
    base = os.environ.get('AIRK_PRIVATE_CENSUS_BASE')
    if not base:
        parser.error('Set AIRK_PRIVATE_CENSUS_BASE to an isolated owner-controlled fixture directory')
    private = Path(base).resolve(strict=True)
    candidate = args.runtime_dir.resolve(strict=True)
    previous = args.previous_runtime.resolve(strict=True)
    assert candidate != previous, 'Candidate and predecessor must have independent retained payload roots'
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    runtimes = (previous, candidate)
    identities = {
        runtime: {name: digest(runtime / name) for name in
                  ('airk.exe', 'airk-embed.exe', 'airk-install.json', 'airk-runtime.json', 'airk-build.json')}
        for runtime in runtimes
    }
    builds = {runtime: json.loads((runtime / 'airk-build.json').read_text(encoding='utf-8'))
              for runtime in runtimes}
    for runtime in runtimes:
        metadata = json.loads((runtime / 'airk-runtime.json').read_text(encoding='utf-8'))
        assert builds[runtime]['runtime'] == metadata
        assert metadata['airk_sha256'] == identities[runtime]['airk.exe']
        assert metadata['embed_sha256'] == identities[runtime]['airk-embed.exe']
    version = json.loads((candidate / 'airk-runtime.json').read_text(encoding='utf-8'))['version']
    assert version == '1.8.8', 'P9 working version changed; review phase identity explicitly'
    benchmark = module('p9_frozen_benchmark', 'scripts/benchmark-mcp-investigation.py')
    clients = module('p9_native_clients', 'scripts/check-service-clients.py')
    owner = Path(tempfile.mkdtemp(prefix='airk-p9-e0-', dir=private)).resolve()
    (output / 'owner-location.json').write_text(json.dumps({'root': str(owner)}) + '\n')
    fixture_path = owner / 'fixture'
    benchmark.create_fixture(fixture_path)
    source, _ = benchmark.load_fixture(fixture_path)
    install, state, data = owner / 'install', owner / 'state', owner / 'data'
    data.mkdir()
    fixture = clients.Fixture(previous, owner)
    fixture.control = state / 'control'
    records = []
    active = None
    passed = False

    def command(runtime, *arguments, stdin=None):
        result = subprocess.run([str(runtime / 'airk.exe'), *map(str, arguments)],
                                input=stdin, capture_output=True, text=True, encoding='utf-8',
                                timeout=360, creationflags=subprocess.CREATE_NO_WINDOW)
        ordinal = len(records)
        records.append({'runtime': str(runtime), 'args': list(map(str, arguments)),
                        'exit_code': result.returncode})
        (owner / f'command-{ordinal}.stdout').write_text(result.stdout, encoding='utf-8')
        (owner / f'command-{ordinal}.stderr').write_text(result.stderr, encoding='utf-8')
        (owner / 'commands.json').write_text(json.dumps(records, indent=2) + '\n')
        assert result.returncode == 0, result.stderr
        return json.loads(result.stdout)

    try:
        for runtime in runtimes:
            command(runtime, 'runtime', 'stage', '--source', runtime, '--install-dir', install,
                    '--state-dir', state, '--approved-sha256', identities[runtime]['airk-install.json'],
                    '--verification-basis', 'organization-digest')
            command(runtime, 'runtime', 'activate', '--install-dir', install,
                    '--runtime-sha256', identities[runtime]['airk-install.json'])
            active = runtime
            if runtime == previous:
                fixture.register('investigation', source, data)
                for configuration in ({'operation': 'vectors', 'enabled': False},
                                      {'operation': 'detail', 'level': 'agent_lean'},
                                      {'operation': 'usage', 'mode': 'disabled'}):
                    fixture.configure('investigation', configuration)
                fixture.job('investigation', 'index')
                fixture.rpc({'pause': {'project': fixture.projects['investigation']['binding'],
                                      'paused': True}})
        fixture.binary, fixture.runtime = candidate / 'airk.exe', candidate
        receipt = json.loads((fixture.control / 'index-catalog-upgrade.json').read_bytes())
        assert receipt['stage'] == 'settled' and receipt['authority_v1']['phase'] == 'settled'
        (output / 'upgrade-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        configuration = output / 'configuration.json'
        configuration.write_text(json.dumps({
            'command': str(candidate / 'airk.exe'), 'args': [*fixture.arguments('investigation'), 'mcp'],
            'launcher_sha256': identities[candidate]['airk.exe'], 'expected_version': version,
            'route': 'isolated-approved-runtime', 'cwd': str(candidate),
        }, indent=2) + '\n')
        passed = benchmark.run(argparse.Namespace(
            fixture=fixture_path, configuration=configuration, output=output / 'replay',
            label='candidate', samples=30, concurrency=[1], timeout=30,
            verification=args.verification, record=None))
        assert passed, 'Frozen replay reported failure; retain all attempts'
    finally:
        try:
            if active is not None:
                fixture.binary, fixture.runtime = active / 'airk.exe', active
                health = fixture.rpc('health')['observation']
                epoch = fixture.rpc('request_epoch')
                request = {'caller': uuid.uuid4().hex,
                           'key': {'namespace': epoch['namespace'], 'epoch': epoch['epoch'],
                                   'nonce': uuid.uuid4().hex},
                           'action': 'stop', 'expected_instance': health['instance'],
                           'runtime': identities[active]['airk-install.json'], 'force': False}
                stop = command(active, 'runtime', 'lifecycle', '--install-dir', install,
                               stdin=json.dumps(request))
                assert stop['status'] == 'lifecycle' and stop['record']['phase'] == 'complete'
                (output / 'lifecycle.json').write_text(json.dumps(stop, indent=2) + '\n')
        finally:
            fixture.log.close()
            after = {runtime: {name: digest(runtime / name) for name in identity}
                     for runtime, identity in identities.items()}
            assert after == identities, 'Retained predecessor or candidate payload changed'
            (output / 'runtime-identities.json').write_text(json.dumps({
                'schema': 'airk.p9.frozen-runtime-identities/1', 'passed': passed,
                'working_version': version,
                'runtime_files': {str(runtime): identity for runtime, identity in identities.items()},
                'build_records': {str(runtime): value for runtime, value in builds.items()},
                'unchanged_after': True,
                'predecessor_path_matches_p8': previous == (ROOT / 'artifacts/v190/p6f-resume-predecessor-shared-copy').resolve(),
            }, indent=2) + '\n')
    print(json.dumps({'passed': passed, 'output': str(output)}))


if __name__ == '__main__':
    main()
