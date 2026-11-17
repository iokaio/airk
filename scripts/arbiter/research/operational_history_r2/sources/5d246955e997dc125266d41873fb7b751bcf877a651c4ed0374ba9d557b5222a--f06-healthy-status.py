# SPDX-License-Identifier: Apache-2.0
"""Windows original-fixture first-status and independent-client qualification."""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time


def require(value, code):
    if not value:
        raise AssertionError(code)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_module(name, path, expected):
    require(digest(path) == expected, 'qualification_helper_changed')
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class OwnedProcess:
    """Read-only process handle, pinned before shutdown; never kills by discovered PID."""
    def __init__(self, pid, install, expected_sha):
        self.api = ctypes.WinDLL('kernel32', use_last_error=True)
        self.api.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        self.api.OpenProcess.restype = wintypes.HANDLE
        self.api.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD,
                                                       wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
        self.api.QueryFullProcessImageNameW.restype = wintypes.BOOL
        self.api.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        self.api.WaitForSingleObject.restype = wintypes.DWORD
        self.api.CloseHandle.argtypes = [wintypes.HANDLE]
        self.api.CloseHandle.restype = wintypes.BOOL
        require(type(pid) is int and pid > 0, 'invalid_owned_service_pid')
        self.handle = self.api.OpenProcess(0x100000 | 0x1000, False, pid)
        require(self.handle, 'owned_process_open_failed')
        try:
            buffer, size = ctypes.create_unicode_buffer(32768), wintypes.DWORD(32768)
            require(self.api.QueryFullProcessImageNameW(self.handle, 0, buffer, ctypes.byref(size)),
                    'owned_process_identity_unavailable')
            actual = Path(buffer.value)
            candidates = list(install.rglob('*.exe'))
            require(0 < len(candidates) <= 128, 'owned_executable_inventory_bound')
            matches = []
            for candidate in candidates:
                require(not candidate.is_symlink() and not candidate.lstat().st_file_attributes & 0x400,
                        'owned_executable_reparse')
                require(candidate.resolve(strict=True).is_relative_to(install.resolve(strict=True)),
                        'owned_executable_outside_install')
                if actual.samefile(candidate):
                    matches.append(candidate)
            require(matches and all(digest(path) == expected_sha for path in matches),
                    'service_process_not_owned_payload')
        except BaseException:
            self.close()
            raise

    def wait(self):
        require(self.api.WaitForSingleObject(self.handle, 45000) == 0, 'owned_service_drain_timeout')

    def close(self):
        if self.handle:
            self.api.CloseHandle(self.handle)
            self.handle = None


def first_observation(session, label, tool, decode):
    origin = time.perf_counter()
    row = {'label': label, 'tool': tool,
           'request_attempted': False, 'outcome': 'transport_failure'}
    try:
        row['request_attempted'] = True
        envelope = session.tool(tool, {'response_mode': 'full'},
                                deadline=time.monotonic() + 20)
        row.update(outcome='response', is_error=envelope.get('isError', False),
                   body=decode(envelope))
    except Exception as error:
        row['failure_category'] = 'first_status_transport_failed'
        row['exception_type'] = type(error).__name__[:64]
    row['elapsed_ms'] = round((time.perf_counter() - origin) * 1000, 3)
    return row



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--approved-sha256', required=True)
    parser.add_argument('--task-root', type=Path, required=True)
    parser.add_argument('--protected-root', action='append', type=Path, required=True)
    parser.add_argument('--report-tag', default='healthy-status')
    args = parser.parse_args()
    require(os.name == 'nt', 'windows_qualification_only')
    task, runtime = args.task_root.resolve(strict=True), args.runtime.resolve(strict=True)
    protected_roots = []
    for root in args.protected_root:
        root = root.resolve(strict=True)
        protected_roots.append(root)
        require(not task.is_relative_to(root) and not root.is_relative_to(task), 'state_source_overlap')
    for directory in (task / 'fixtures', task / 'reports', task / 'scripts'):
        metadata = directory.lstat()
        require(directory.is_dir() and not directory.is_symlink()
                and not metadata.st_file_attributes & 0x400
                and directory.resolve(strict=True).is_relative_to(task), 'redirected_task_directory')
    require(digest(runtime / 'airk-install.json') == args.approved_sha256, 'unapproved_runtime')
    scripts = task / 'scripts'
    clients = load_module('f06_clients', scripts / 'check-service-clients.py',
                          'a4588c7202f3ba3aee27057bf0aa9e15dfeb627815ee61e3c8df249491d6984b')
    scope = load_module('f06_scope', scripts / 'check-source-scope-original-identity.py',
                        '02d6c9573972cb1b21aa67813c29d1effde48bd7237c6374b8b36e7a542e82ae')
    require(digest(scripts / 'check-service-pool.py') ==
            '6a35e24cb66497b722c6c252d23d7c6203dd6402be2eba25bdc935cd5102ae5f', 'pool_helper_changed')
    require(1 <= len(args.report_tag) <= 64 and
            all(c.isascii() and (c.isalnum() or c in '-_') for c in args.report_tag), 'invalid_report_tag')
    reports = task / 'reports' / args.report_tag
    reports.mkdir()
    result = {'schema': 1, 'completed': False, 'cleanup': False,
              'runtime_manifest_sha256': args.approved_sha256, 'original_fixture_only': True,
              'first_observations': [], 'receipt_observations': [], 'assertions': {},
              'limits': ['functional Windows fixture, not corpus latency comparison',
                         'OS caches uncontrolled', 'no worker-wide observation reuse claim',
                         'one existing worker query permit; client queries intentionally sequential']}
    sessions, fixture, owner, stage = [], None, None, 'setup'

    class Installed(clients.Fixture):
        def __init__(self, owner):
            super().__init__(runtime, owner)
            self.install, self.state = owner / 'install', owner / 'state'
            self.control, self.staged = self.state / 'control', False

        def activate(self):
            self.run('runtime', 'stage', '--source', runtime, '--install-dir', self.install,
                     '--approved-sha256', args.approved_sha256, '--verification-basis',
                     'organization-digest', '--state-dir', self.state)
            self.staged = True
            self.run('runtime', 'activate', '--install-dir', self.install,
                     '--runtime-sha256', args.approved_sha256)

        def close(self):
            try:
                if self.staged:
                    self.run('runtime', 'uninstall', '--install-dir', self.install)
            finally:
                self.log.close()

    try:
        owner = Path(tempfile.mkdtemp(prefix='status-healthy-', dir=task / 'fixtures')).resolve(strict=True)
        require(owner.is_relative_to(task) and all(not owner.is_relative_to(root)
                and not root.is_relative_to(owner) for root in protected_roots), 'redirected_fixture_owner')
        require(not owner.is_relative_to(runtime) and not runtime.is_relative_to(owner), 'payload_state_overlap')
        source, data = owner / 'source', owner / 'data'
        source.mkdir(); data.mkdir()
        (source / 'README.md').write_text('Original healthyfixturebeacon reference.\n', encoding='utf-8')
        (source / 'entry.rs').write_text('fn healthyfixturebeacon() {}\n', encoding='utf-8')
        inventory = scope.source_inventory([source])
        fixture = Installed(owner)
        fixture.activate()
        stage = 'prepare_saved_index'
        fixture.register('healthy', source, data)
        binding = fixture.projects['healthy']['binding']
        fixture.rpc({'configure_watch': {'project': binding, 'enabled': False}})
        require(fixture.rpc({'watch': {'project': binding}})['observation']['enabled'] is False,
                'watch_not_disabled')
        fixture.configure('healthy', {'operation': 'vectors', 'enabled': False})
        fixture.job('healthy', 'index')
        prepared = fixture.tool('healthy', 'airk_status', {'response_mode': 'full'})
        require(prepared.get('index_state') == 'available' and prepared.get('files') == 2
                and prepared.get('chunks', 0) > 0, 'saved_index_not_available')
        publication = prepared['publication']['token']
        result['saved_index'] = {'files': prepared['files'], 'chunks': prepared['chunks'],
                                 'publication': publication}
        pin = ['--service-install-dir', fixture.install, '--service-project', binding,
               '--root', source, '--data-dir', data, '--collection', 'healthy']
        root = ['--service-install-dir', fixture.install, '--root', source]
        definitions = [('source-public', root, 'status'), ('pinned-public', pin, 'status'),
                       ('source-legacy', root, 'airk_status'), ('pinned-legacy', pin, 'airk_status')]
        for label, arguments, tool in definitions:
            # Existing worker admission permits one in-flight query. Give each
            # exact first observation its own fully drained saved-service start;
            # parallel availability would test a different admission contract.
            stage = 'stop_and_drain_saved_service_' + label
            health = fixture.rpc('health')['observation']
            require(health['runtime_sha256'] == args.approved_sha256, 'wrong_owned_runtime_health')
            process = OwnedProcess(health['process_id'], fixture.install, health['executable_sha256'])
            try:
                stopped = fixture.rpc('shutdown')
                require(stopped.get('status') == 'stopping', 'shutdown_not_acknowledged')
                process.wait()
            finally:
                process.close()
            # No native metadata/status/diagnostics/evidence after the drain
            # until this session's exact first status. Static stdio is permitted.
            stage = 'initialize_cold_frontend_' + label
            session = scope.Session(fixture, arguments, label, reports)
            sessions.append(session)
            session.start()
            projects = scope.body(session.request('tools/call', {'name': 'projects', 'arguments': {}}))
            session.expected_project = projects['default_project']
            stage = 'exact_first_status_' + label
            row = first_observation(session, label, tool, scope.body)
            row['frontend_pid'] = session.child.pid
            row['prior_service_drained'] = True
            result['first_observations'].append(row)
            require(row['outcome'] == 'response', 'first_status_transport_failed')
            value = row['body']
            require(not row['is_error'] and value.get('index_state') == 'available'
                    and value.get('publication', {}).get('token') == publication
                    and value.get('files') == prepared['files'] and value.get('chunks') == prepared['chunks']
                    and value.get('project') == session.expected_project, 'first_status_not_saved_index')
            stage = 'public_legacy_parity_' + label
            alternate = 'airk_status' if tool == 'status' else 'status'
            value = scope.body(session.tool(alternate, {'response_mode': 'full'}))
            require(value.get('index_state') == 'available' and value.get('publication', {}).get('token') == publication
                    and value.get('project') == session.expected_project and value.get('files') == prepared['files']
                    and value.get('chunks') == prepared['chunks'], 'public_legacy_status_mismatch')
            session.close()
            sessions.remove(session)
        result['assertions']['four_cold_saved_service_first_statuses_available_without_warmup_or_retry'] = True
        result['assertions']['public_legacy_status_parity'] = True
        # Keep four distinct clients open against this same worker. Sequential
        # queries preserve its existing admission allowance while testing that
        # one client's acknowledged detail never suppresses another's first read.
        stage = 'initialize_receipt_cohort'
        for label, arguments, _ in definitions:
            session = scope.Session(fixture, arguments, 'receipt-' + label, reports)
            sessions.append(session)
            session.start()
        require(len({session.child.pid for session in sessions}) == 4, 'frontends_not_independent')
        result['assertions']['four_independent_receipt_frontend_processes'] = True
        worker_before = fixture.rpc({'worker': {'project': binding}})
        require(worker_before.get('project') == binding
                and isinstance(worker_before.get('identity'), dict)
                and worker_before['identity'].get('project') == binding,
                'receipt_worker_identity_unavailable')
        result['receipt_worker_before'] = worker_before['identity']
        stage = 'independent_logical_receipts'
        for session, definition in zip(sessions, definitions):
            arguments = {'query': 'healthyfixturebeacon', 'mode': 'lexical', 'response_mode': 'terse'}
            first_read = session.tool('search', arguments)
            first_body = scope.body(first_read)
            require(first_read.get('isError') is not True and 'discovery_readiness' in first_body,
                    'new_frontend_inherited_another_receipt')
            second_read = session.tool('search', arguments)
            second_body = scope.body(second_read)
            require(second_read.get('isError') is not True and 'discovery_readiness' not in second_body,
                    'complete_observation_receipt_not_retained')
            for value in (first_body, second_body):
                require(value.get('collection') == 'healthy' and value.get('hits')
                        and value.get('publication', {}).get('token') == publication,
                        'receipt_probe_wrong_or_empty_evidence')
            result['receipt_observations'].append({
                'label': definition[0], 'frontend_pid': session.child.pid,
                'first_discovery_included': 'discovery_readiness' in first_body,
                'second_discovery_omitted': 'discovery_readiness' not in second_body,
                'publication': second_body['publication']['token'],
            })
        worker_after = fixture.rpc({'worker': {'project': binding}})
        result['receipt_worker_after'] = worker_after.get('identity')
        require(worker_after.get('project') == binding
                and worker_after.get('identity') == worker_before['identity'],
                'receipt_worker_changed')
        result['assertions']['each_client_observes_then_omits_its_own_detailed_discovery'] = True
        require(scope.source_inventory([source]) == inventory, 'source_inventory_changed')
        result['assertions']['complete_source_inventory_preserved'] = True
        result['completed'] = True
    except Exception:
        result['failed_stage'] = stage
        raise
    finally:
        primary, cleanup_errors = sys.exc_info()[1], []
        for session in reversed(sessions):
            try:
                session.close()
            except Exception:
                cleanup_errors.append('session_cleanup_failed')
        if fixture is not None:
            try:
                fixture.close()
            except Exception:
                cleanup_errors.append('installation_cleanup_failed')
        if owner is not None and not cleanup_errors:
            try:
                parent = (task / 'fixtures').resolve(strict=True)
                require(parent.is_relative_to(task) and owner.parent == parent
                        and owner.name.startswith('status-healthy-')
                        and owner.resolve(strict=True).is_relative_to(parent)
                        and not owner.is_symlink() and not owner.lstat().st_file_attributes & 0x400,
                        'unsafe_fixture_cleanup')
                shutil.rmtree(owner)
            except Exception:
                cleanup_errors.append('owned_filesystem_cleanup_failed')
        result['cleanup'] = not cleanup_errors
        result['cleanup_errors'] = cleanup_errors
        result['completed'] = result['completed'] and result['cleanup']
        with (reports / 'report.json').open('x', encoding='utf-8') as stream:
            json.dump(result, stream, indent=2)
            stream.write('\n')
        print(json.dumps(result, indent=2), flush=True)
        if cleanup_errors:
            if primary is not None:
                primary.add_note('owned cleanup also failed')
            else:
                raise AssertionError('owned_cleanup_failed')


if __name__ == '__main__':
    main()
