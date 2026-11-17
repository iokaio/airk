# SPDX-License-Identifier: Apache-2.0
"""Source-only fresh-pair Verify/query coexistence qualification. Requires approval."""
import argparse
from contextlib import contextmanager
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import time
import traceback


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def require(value, code):
    if not value:
        raise AssertionError(code)


@contextmanager
def hold_ledger(owner, path):
    # Existing Rust advisory lock only; never alter ledger/marker bytes.
    require(path.is_file() and path.resolve(strict=True).is_relative_to(owner), 'ledger_escape')
    for component in (path, *path.parents):
        require(not component.is_symlink()
                and not getattr(component.lstat(), 'st_file_attributes', 0) & 0x400, 'ledger_reparse')
        if component.samefile(owner):
            break
    with path.open('r+b') as stream:
        if os.name == 'nt':
            import msvcrt
            def lock():
                stream.seek(0)
                msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
            def unlock():
                stream.seek(0)
                msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            def lock():
                fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            def unlock():
                fcntl.flock(stream.fileno(), fcntl.LOCK_UN)
        deadline = time.monotonic() + 10
        while True:
            try:
                lock()
                break
            except OSError:
                require(time.monotonic() < deadline, 'ledger_lock_deadline')
                time.sleep(0.02)
        try:
            yield
        finally:
            unlock()


def run(args, report):
    repo = next(p for p in Path(__file__).resolve().parents if (p / 'scripts/check-service-clients.py').is_file())
    repair_path = repo / 'artifacts/optimizations/f11-fresh-pair-coverage/qualify-approved-repair.py'
    require(hashlib.sha256(repair_path.read_bytes()).hexdigest()
            == '2943e05a16fef4ff21ea53cac6daef772c599c7dbc0d6918172f42290cc8db58', 'repair_helper_changed')
    repair = load('query_repair', repair_path)
    base = json.loads(repair_path.with_name('BASE.json').read_bytes())['files']
    for relative, digest in base.items():
        require(hashlib.sha256((repo / relative).read_bytes()).hexdigest() == digest, 'helper_changed')
    helpers = load('query_clients', repo / 'scripts/check-service-clients.py')
    benchmark = load('query_benchmark', repo / 'scripts/benchmark-real-corpora.py')
    runtime = Path(args.runtime).resolve(strict=True)
    report['runtime'] = benchmark.verify_runtime(runtime, args.install_sha256)
    with tempfile.TemporaryDirectory(prefix='airk-f11-query-') as directory:
        owner = Path(directory).resolve(strict=True)
        require(not owner.is_relative_to(repo) and not owner.is_relative_to(runtime)
                and not runtime.is_relative_to(owner), 'owner_isolation')
        root = owner / 'source'
        root.mkdir()
        for i in range(65):
            (root / f'item_{i:03}.txt').write_text(f'Owned physical verification sample {i}.\n', encoding='utf-8')
        original = repair.snapshot(root)
        fixture = helpers.Fixture(runtime, owner, command_timeout=120, job_timeout=180)
        primary = None
        paused = False
        try:
            report['stage'] = 'setup'
            fixture.start()
            name = 'f11-query'
            fixture.register(name, root, owner / 'data')
            binding = fixture.projects[name]['binding']
            fixture.rpc({'configure_watch': {'project': binding, 'enabled': False}})
            fixture.configure(name, {'operation': 'vectors', 'enabled': False})
            fixture.job(name, 'index')
            status = fixture.published_status(name)
            require(status['files'] == 65, 'indexed_count')
            token = status['publication']['token']
            epoch = fixture.rpc('request_epoch')
            report['stage'] = 'accepted_verify'
            with hold_ledger(owner, owner / 'data/.quota-1.4/ledger.lock'):
                receipt = fixture.enqueue(name, 'verify')  # Exactly once; no uncertain replay.
                job_id = receipt['job']['id']
                def observation():
                    return fixture.rpc({'progress': {'project': binding}})
                def matching(value):
                    item = value.get('observation')
                    return (item is not None and item['job']['id'] == job_id
                            and item['job']['state'] == 'running' and item['progress']['verified_objects'] == 0)
                # Actual persisted audit ownership, independently read without mutation.
                database = owner / 'control/service-1.4.sqlite3'
                require(database.is_file() and not database.is_symlink(), 'registry_shape')
                def guarded(value):
                    if not matching(value):
                        return False
                    with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True, timeout=5) as connection:
                        row = connection.execute('SELECT engine FROM service_recovery_budget WHERE scope=? AND failures=0 AND suppressed=1',
                                                 ('integrity:' + binding,)).fetchone()
                    return row == (job_id,)
                repair.poll(fixture, {'progress': {'project': binding}}, guarded, 30)
                fixture.rpc({'pause': {'project': binding, 'paused': True}})
                paused = True
                acknowledged = repair.poll(fixture, {'progress': {'project': binding}},
                                           lambda value: matching(value) and value['observation']['progress']['paused'] is True, 30)
                report['pause_acknowledged'] = acknowledged['observation']['progress']['paused']
            # Ledger is released; actual WORK remains paused. Evidence does not
            # inherit that thread-local checkpoint and must complete independently.
            report['stage'] = 'query_while_verify_paused'
            before = observation()
            require(matching(before) and before['observation']['progress']['paused'], 'lost_paused_owner_before_query')
            evidence = fixture.query(name, 'tools', 'call', 'airk_read_file', '--args',
                                     json.dumps({'path': 'item_000.txt', 'start_line': 1, 'end_line': 1,
                                                 'response_mode': 'full'}), retry=False)
            require(evidence['collection'] == name and evidence['publication']['token'] == token,
                    'query_scope_or_publication')
            require('Owned physical verification sample 0.' in json.dumps(evidence), 'retained_read_missing')
            scalar = fixture.query(name, 'tools', 'call', 'airk_status', '--args',
                                   '{"response_mode":"full"}', retry=False)
            require(scalar['publication']['token'] == token, 'status_publication')
            after = observation()
            require(matching(after) and after['observation']['progress']['paused'], 'lost_paused_owner_after_query')
            report['query'] = {'same_job_running_before_and_after': True, 'worker_pause_acknowledged': True,
                               'read_exact_saved_publication': True, 'status_exact_saved_publication': True}
            report['stage'] = 'resume_same_job'
            fixture.rpc({'pause': {'project': binding, 'paused': False}})
            paused = False
            job = fixture.wait(receipt)
            require(job['outcome'] == {'state': 'succeeded', 'revision': token}, 'verify_revision')
            report['physical_coverage'] = repair.physical_coverage(fixture, benchmark, name, epoch['namespace'], job_id, token)
            final = observation()['observation']
            require(final['job']['id'] == job_id and final['progress']['verified_objects'] == 66,
                    'aggregate_work_count')
            report['aggregate_verified_objects'] = 66
        except BaseException as error:
            primary = error
            raise
        finally:
            if paused:
                try:
                    fixture.rpc({'pause': {'project': binding, 'paused': False}})
                except Exception:
                    report['cleanup_resume_failed'] = True
            detail = {}
            report['cleanup_failed'] = benchmark.close_owned_fixture(fixture, primary=True, detail=detail)
            report['cleanup'] = detail
            try:
                report['source_unchanged'] = repair.snapshot(root) == original
            except Exception:
                report['source_unchanged'] = False
            if primary is None:
                require(report['source_unchanged'] and not report['cleanup_failed'], 'cleanup_or_source_failure')
    report['stage'] = 'complete'
    report['passed'] = True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', required=True)
    parser.add_argument('--install-sha256', required=True)
    args = parser.parse_args()
    report = {'schema': 1, 'passed': False, 'stage': 'preparation', 'cleanup_failed': None,
              'qualification': 'paused_accepted_verify_query_coexistence_only'}
    try:
        run(args, report)
    except BaseException:
        traceback.print_exc()  # Private execution stderr only; public report stays content-free.
        report['failure'] = 'native_or_fixture_failure'
    encoded = json.dumps(report, sort_keys=True)
    require(len(encoded.encode()) <= 16384, 'report_bound')
    print(encoded)
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
