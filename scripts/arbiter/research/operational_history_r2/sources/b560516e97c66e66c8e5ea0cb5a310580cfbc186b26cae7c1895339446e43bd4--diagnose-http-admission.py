# SPDX-License-Identifier: Apache-2.0
"""One new lexical 64-client status wave; complete refusals, no retry/campaign replay."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import time

ROOT = Path(__file__).resolve().parents[4]
spec = importlib.util.spec_from_file_location('airk_bounded_admission_diagnostic', ROOT / 'scripts/benchmark-service.py')
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)
clients, transport = benchmark.clients, benchmark.http_fixture


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    private = Path(os.environ['AIRK_PRIVATE_CENSUS_BASE']).resolve(strict=True)
    owner = args.output.resolve()
    assert owner.is_relative_to(private) and owner != private
    transport.contained_private_path(owner.parent, private)
    owner.mkdir(exist_ok=False)
    fixture = clients.Fixture(args.runtime_dir, owner, command_timeout=90, job_timeout=90)
    report = {'schema': 1, 'completed': False, 'all_reads_succeeded': False,
              'purpose': 'NEW lexical 64-client status requests, not replay of the failed semantic campaign',
              'semantic_campaign_repeated': False, 'source_updates': 0, 'model_setup_jobs': 0,
              'retry_policy': 'Zero request retries; all initialized sessions receive exactly one new full status request',
              'runtime_manifest_sha256': sha(args.runtime_dir / 'airk-install.json'),
              'background_limits': 'Same host may run independent editor/container qualification; no idle CPU, private physical memory or global process absence claim.',
              'initialized_clients': [], 'responses': [], 'cleanup': []}
    sessions = []
    started = time.monotonic()
    until = started + 300
    source = owner / 'source'
    source.mkdir()
    original = source / 'diagnostic.txt'
    original.write_bytes(b'Original fictional lexical admission diagnostic.\n')
    original_sha = sha(original)
    data = owner / 'data'
    data.mkdir()
    try:
        fixture.start()
        report['native_health_before'] = fixture.managed_health
        fixture.register('admission-diagnostic', source, data)
        fixture.rpc({'configure_watch': {'project': fixture.projects['admission-diagnostic']['binding'], 'enabled': False}})
        fixture.configure('admission-diagnostic', {'operation': 'vectors', 'enabled': False})
        fixture.job('admission-diagnostic', 'index')
        connections = []
        for _ in range(64):
            assert time.monotonic() < until, 'Diagnostic original enrollment deadline exhausted'
            connections.append(benchmark.enroll(fixture, 'admission-diagnostic'))
        assert len({c['connection_id'] for c in connections}) == 64
        report['credential_ids'] = [c['connection_id'] for c in connections]

        def initialize(index):
            session = benchmark.HttpSession(connections[index], fixture.control,
                observer=lambda value: report['cleanup'].append({'client': index, 'observation': value}))
            return index, session

        initialization_failure = None
        with ThreadPoolExecutor(max_workers=16) as pool:
            futures = [pool.submit(initialize, index) for index in range(64)]
            for future in futures:
                try:
                    index, session = future.result()
                    sessions.append((index, session))
                    report['initialized_clients'].append({'client': index, 'session_sha256': session.session_sha256,
                        'connection_id': connections[index]['connection_id']})
                except BaseException as error:
                    report.setdefault('initialization_errors', []).append(benchmark.error_observation(error))
                    initialization_failure = initialization_failure or error
        if initialization_failure is not None:
            raise initialization_failure
        assert len(sessions) == 64
        report['resources_before_wave'] = fixture.rpc('resources')
        report['embedding_status_before_wave'] = fixture.rpc('embedding_status')

        def status(selected):
            index, session = selected
            request = {'client': index, 'connection_id': connections[index]['connection_id'],
                       'session_sha256': session.session_sha256, 'jsonrpc_id': session.sequence + 1,
                       'tool': 'status', 'arguments': {'response_mode': 'full'}}
            began = time.monotonic()
            try:
                response = session.request('tools/call', {'name': 'status', 'arguments': request['arguments']})
                return {**request, 'elapsed_seconds': time.monotonic()-began, 'result': response}
            except BaseException as error:
                return {**request, 'elapsed_seconds': time.monotonic()-began, 'error': benchmark.error_observation(error)}

        assert time.monotonic() < until, 'Diagnostic original wave deadline exhausted'
        with ThreadPoolExecutor(max_workers=64) as pool:
            futures = [pool.submit(status, selected) for selected in sessions]
            try:
                report['native_health_during_wave'] = fixture.rpc('health')
            except BaseException as error:
                report['native_health_during_wave_error'] = benchmark.error_observation(error)
            # Collect every distinct completed/uncertain outcome before cleanup.
            report['responses'] = [future.result() for future in futures]
        report['resources_after_wave'] = fixture.rpc('resources')
        report['embedding_status_after_wave'] = fixture.rpc('embedding_status')
        report['all_reads_succeeded'] = all('result' in row for row in report['responses'])
        report['completed'] = len(report['responses']) == 64
    except BaseException as error:
        report['failure'] = benchmark.error_observation(error)
        raise
    finally:
        try:
            for index, session in reversed(sessions):
                try:
                    session.__exit__(None, None, None)
                except BaseException as error:
                    report.setdefault('cleanup_failures', []).append({'client': index, 'error': benchmark.error_observation(error)})
        finally:
            try:
                fixture.close()
                report['native_stop'] = fixture.managed_stop
                report['native_stop_complete'] = fixture.managed_stop is not None and fixture.managed_stop['record']['phase'] == 'complete'
            finally:
                report['elapsed_seconds'] = time.monotonic()-started
                report['source_bytes_preserved'] = sha(original) == original_sha
                with args.report.open('x', encoding='utf-8') as output:
                    json.dump(report, output, indent=2)
                    output.write('\n')
    assert report['completed'] and report.get('native_stop_complete') and report['source_bytes_preserved']
    assert not report.get('cleanup_failures')
    print(json.dumps({'completed': True, 'clients': len(report['responses']),
        'successes': sum('result' in row for row in report['responses']),
        'refusals': sum('http_response' in row.get('error', {}) for row in report['responses']),
        'all_reads_succeeded': report['all_reads_succeeded'], 'native_stop_complete': True}))


if __name__ == '__main__':
    main()
