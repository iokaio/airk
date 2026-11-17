# SPDX-License-Identifier: Apache-2.0
"""Opt-in approved-pair qualification; disposable synthetic sources only.

Source-only draft. Do not execute before runtime/payload approval.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import time
import uuid


class QualificationError(Exception):
    pass


def require(value, code):
    if not value:
        raise QualificationError(code)


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def snapshot(root):
    files = sorted(root.iterdir())
    require(len(files) == 65 and all(p.is_file() and not p.is_symlink() for p in files), 'source_shape')
    return [(p.name, hashlib.sha256(p.read_bytes()).hexdigest()) for p in files]


def poll(fixture, request, accept, seconds):
    deadline = time.monotonic() + seconds
    for _ in range(3601):
        require(time.monotonic() < deadline, 'observation_deadline')
        value = fixture.rpc(request, deadline=deadline)
        if accept(value):
            return value
        time.sleep(0.05)
    raise QualificationError('observation_count_bound')


def create_plan(fixture, binding, submissions):
    epoch = fixture.rpc('request_epoch')
    submissions['plan_create'] += 1
    plan = fixture.rpc({'plan_create': {
        'caller': uuid.uuid4().hex,
        'key': {'namespace': epoch['namespace'], 'epoch': epoch['epoch'], 'nonce': uuid.uuid4().hex},
        'request': {'selection': {'selected': [binding]}, 'operation': 'verify'},
    }})['plan']
    return plan, epoch['namespace']


def approved_verify(fixture, benchmark, name, token, submissions):
    binding = fixture.projects[name]['binding']
    created, namespace = create_plan(fixture, binding, submissions)
    request = {'plan': {'id': created['id']}}
    ready = poll(fixture, request, lambda v: v['plan']['phase'] in ('ready', 'finished'), 180)['plan']
    require(ready['phase'] == 'ready' and ready['preview_digest'] is not None, 'plan_not_ready')
    page_request = {'plan_items': {'id': ready['id'], 'after': None, 'limit': 2}}
    page = fixture.rpc(page_request)['page']
    require(page['after'] is None and len(page['items']) == 1, 'plan_scope_count')
    item = page['items'][0]
    require(item['project'] == binding and item['action'] == 'verify'
            and item['availability'] == 'read_only' and not item['explicit_fallback_required']
            and item['action_job'] is None and item['inspection_job'] is not None, 'plan_preview')
    inspected = fixture.rpc({'job': {'id': item['inspection_job']}})['job']
    require(inspected['project'] == binding and inspected['kind'] == 'plan_inspect'
            and inspected['state'] == 'succeeded', 'inspection_not_complete')
    # Exactly one approval submission. A timeout is uncertain and MUST NOT replay.
    submissions['plan_execute'] += 1
    fixture.rpc({'plan_execute': {'id': ready['id'], 'preview': ready['preview_digest'], 'allow_fallback': False}})
    final = poll(fixture, request, lambda v: v['plan']['phase'] == 'finished', 180)['plan']
    expected = {key: 0 for key in ('awaiting_inspection', 'ready', 'queued', 'running', 'failed',
                                  'stale', 'skipped', 'canceled', 'cleanup_pending')}
    expected.update(selected=1, succeeded=1)
    require(final['counts'] == expected and final['outcome'] == 'succeeded', 'plan_completion_counts')
    page = fixture.rpc(page_request)['page']
    require(page['after'] is None and len(page['items']) == 1, 'final_plan_scope')
    item = page['items'][0]
    require(item['project'] == binding and item['phase'] == 'succeeded'
            and item['action_job'] is not None and not item['cleanup_pending'], 'repair_item')
    job = fixture.rpc({'job': {'id': item['action_job']}})['job']
    require(job['project'] == binding and job['kind'] == 'repair' and job['state'] == 'succeeded'
            and job['outcome'] == {'state': 'succeeded', 'revision': token}
            and item['result_revision'] == token and item['outcome'] == job['outcome'], 'repair_exact_completion')
    observation = item['progress']
    require(observation is not None and observation['job']['id'] == job['id']
            and observation['worker']['project'] == binding
            and observation['progress']['job'] == job['id']
            and observation['progress']['verified_objects'] == 67, 'aggregate_verify_work_count')
    coverage = physical_coverage(fixture, benchmark, name, namespace, job['id'], token)
    return {'plan': ready['id'], 'inspection_job': inspected['id'], 'repair_job': job['id'],
            'project': binding, 'aggregate_verified_objects': 67, 'physical_coverage': coverage,
            'counts': final['counts'],
            'publication_unchanged': True}


def physical_coverage(fixture, benchmark, name, namespace, job, token):
    # Known collection receipt only; no recursive source/index discovery.
    collection = benchmark.physical(fixture.owner / 'data' / 'collections-1.4' / name)
    require(collection.is_relative_to(fixture.owner), 'receipt_collection_escape')
    path = benchmark.physical(collection / 'integrity-observation.json')
    require(path.parent == collection and path.is_file(), 'receipt_shape')
    with path.open('rb') as stream:
        raw = stream.read(16385)
    require(len(raw) <= 16384, 'receipt_byte_bound')
    state = json.loads(raw)
    progress = state['verify']
    head = state['head']
    require(state['namespace'] == namespace and progress['job'] == job
            and progress['scope'] == head['scope']
            and head['scope']['collection'] == name
            and progress['artifact_root'] == head['artifact_root']
            and progress['lexical_commit'] == head['lexical_commit']
            and progress['checked_files'] == 65 and progress['objects_complete'] is True
            and progress['lexical_observed_unix_ms'] is not None
            and progress['unresolved'] is False and not state['known_damage'], 'durable_physical_coverage')
    # Native receipt serde preserves CurrentHead field order. This owned ASCII
    # fixture recomputes the existing canonical head token; no new wire formula.
    canonical = json.dumps(head, ensure_ascii=False, separators=(',', ':')).encode('utf-8')
    digest = hashlib.sha256(b'AIRK-DIGEST-1\0')
    for field in (b'current-index-head/2', canonical):
        digest.update(len(field).to_bytes(8, 'big'))
        digest.update(field)
    actual = f"airk-r1:{head['scope']['incarnation']}:{head['sequence']}:{digest.hexdigest()}"
    require(actual == token, 'durable_physical_head_mismatch')
    return {'checked_files': 65, 'objects_complete': True, 'lexical_complete': True,
            'unresolved': False, 'exact_job_and_head': True,
            'receipt_sha256': hashlib.sha256(raw).hexdigest()}

def run(args, report):
    repo = next(p for p in Path(__file__).resolve().parents if (p / 'scripts/check-service-clients.py').is_file())
    expected = json.loads(Path(__file__).with_name('BASE.json').read_bytes())['files']
    helper_paths = {'clients': 'scripts/check-service-clients.py',
                    'benchmark': 'scripts/benchmark-real-corpora.py'}
    report['helper_sha256'] = {}
    for label, relative in helper_paths.items():
        actual = hashlib.sha256((repo / relative).read_bytes()).hexdigest()
        require(actual == expected[relative], 'qualification_helper_changed')
        report['helper_sha256'][label] = actual
    helpers = module('f11_clients', repo / 'scripts/check-service-clients.py')
    benchmark = module('f11_benchmark', repo / 'scripts/benchmark-real-corpora.py')
    report['stage'] = 'runtime_verification'
    runtime = Path(args.runtime).resolve(strict=True)
    report['runtime'] = benchmark.verify_runtime(runtime, args.install_sha256)
    report['submissions'] = {'plan_create': 0, 'plan_execute': 0, 'plan_cancel': 0}
    with tempfile.TemporaryDirectory(prefix='airk-f11-approved-') as directory:
        owner = Path(directory).resolve(strict=True)
        require(not owner.is_relative_to(repo) and not owner.is_relative_to(runtime)
                and not runtime.is_relative_to(owner), 'owner_isolation')
        (owner / 'owned-fixture.json').write_text(json.dumps({'schema': 1, 'purpose': 'f11-approved-repair',
                                                            'nonce': uuid.uuid4().hex}), encoding='utf-8')
        root = owner / 'source'
        root.mkdir()
        for i in range(65):
            (root / f'item_{i:03}.txt').write_text(f'Owned physical verification sample {i}.\n', encoding='utf-8')
        original = snapshot(root)
        fixture = helpers.Fixture(runtime, owner, command_timeout=120, job_timeout=180)
        primary = None
        try:
            report['stage'] = 'setup'
            fixture.start()
            name = 'f11-physical'
            fixture.register(name, root, owner / 'data')
            binding = fixture.projects[name]['binding']
            fixture.rpc({'configure_watch': {'project': binding, 'enabled': False}})
            fixture.configure(name, {'operation': 'vectors', 'enabled': False})
            fixture.job(name, 'index')
            before = fixture.published_status(name)
            require(before['files'] == 65, 'indexed_file_count')
            token = before['publication']['token']
            report['stage'] = 'approved_repair'
            report['repair'] = approved_verify(fixture, benchmark, name, token, report['submissions'])
            require(snapshot(root) == original, 'source_changed_after_repair')
            require(fixture.published_status(name)['publication']['token'] == token, 'repair_changed_publication')
            report['stage'] = 'unapproved_cancel'
            canceled, _ = create_plan(fixture, binding, report['submissions'])
            report['submissions']['plan_cancel'] += 1
            fixture.rpc({'plan_cancel': {'id': canceled['id']}})
            done = poll(fixture, {'plan': {'id': canceled['id']}},
                        lambda v: v['plan']['phase'] == 'finished', 180)['plan']
            page = fixture.rpc({'plan_items': {'id': canceled['id'], 'after': None, 'limit': 2}})['page']
            require(done['outcome'] == 'canceled' and done['counts']['selected'] == 1
                    and len(page['items']) == 1 and page['items'][0]['action_job'] is None,
                    'unapproved_cancel_launched_action')
            report['unapproved_cancel'] = {'finished': True, 'action_submitted': False}
            report['stage'] = 'clean_restart'
            fixture.run('service', 'shutdown', '--runtime-dir', fixture.control)
            fixture.child.wait(timeout=45)
            fixture.start()  # Same owned control/data/source, no registration or mutation replay.
            require(fixture.published_status(name)['publication']['token'] == token, 'restart_changed_publication')
            report['restart'] = {'saved_scope_reopened': True, 'publication_unchanged': True}
        except BaseException as error:
            primary = error
            raise
        finally:
            detail = {}
            report['cleanup_failed'] = benchmark.close_owned_fixture(fixture, primary=True, detail=detail)
            report['cleanup'] = detail
            try:
                report['source_unchanged'] = snapshot(root) == original
            except Exception:
                report['source_unchanged'] = False
            if primary is None:
                require(report['source_unchanged'], 'source_changed')
                require(not report['cleanup_failed'], 'service_cleanup_failed')
    report['stage'] = 'complete'
    report['passed'] = True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', required=True)
    parser.add_argument('--install-sha256', required=True)
    args = parser.parse_args()
    report = {'schema': 1, 'passed': False, 'stage': 'preparation', 'cleanup_failed': None,
              'qualification': 'approved_physical_repair_only'}
    try:
        run(args, report)
    except BaseException as error:
        # The first stage is retained; no source paths, stderr, or raw exception text in public output.
        report['failure'] = str(error) if isinstance(error, QualificationError) else 'native_or_fixture_failure'
    encoded = json.dumps(report, sort_keys=True)
    require(len(encoded.encode('utf-8')) <= 16384, 'report_bound')
    print(encoded)
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
