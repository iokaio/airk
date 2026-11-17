# SPDX-License-Identifier: Apache-2.0
"""Original-only installed scope lifecycle qualification; never real corpus data."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import queue
import shutil
import sys
import subprocess
import tempfile
import threading
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(value, code):
    if not value:
        raise AssertionError(code)


def body(envelope):
    try:
        return json.loads(envelope['content'][0]['text'])
    except (KeyError, IndexError, TypeError, json.JSONDecodeError):
        return {'opaque_error': True}


def source_inventory(roots):
    rows = {}
    for root in roots:
        for parent, directories, files in os.walk(root, topdown=True, followlinks=False):
            for path in [Path(parent), *(Path(parent) / name for name in directories + files)]:
                metadata = path.lstat()
                require(not path.is_symlink() and not getattr(metadata, 'st_file_attributes', 0) & 0x400,
                        'source_link_or_reparse')
                relative = (root.name, path.relative_to(root).as_posix())
                require(path.is_dir() or path.is_file(), 'source_special_entry')
                rows[relative] = ('directory', None) if path.is_dir() else ('file', digest(path))
    return rows


class Session:
    def __init__(self, fixture, arguments, label, reports, suffix=()):
        self.fixture, self.arguments, self.label = fixture, tuple(arguments), label
        self.suffix = tuple(suffix)
        self.responses, self.counter = queue.Queue(), 0
        self.stderr = (reports / (label + '.stderr')).open('xb')
        self.trace = (reports / (label + '.jsonl')).open('x', encoding='utf-8')
        self.child = None

    def start(self):
        f = self.fixture
        self.child = subprocess.Popen([str(f.binary), *map(str, self.arguments), 'mcp', *self.suffix],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr,
            text=True, encoding='utf-8', cwd=f.runtime, env=f.environment,
            creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        def pump():
            try:
                for line in self.child.stdout:
                    self.responses.put(json.loads(line))
            except Exception:
                self.responses.put(None)
            finally:
                self.responses.put(None)
        threading.Thread(target=pump, daemon=True).start()
        initialized = self.request('initialize', {'protocolVersion': '2025-11-25',
            'capabilities': {}, 'clientInfo': {'name': 'airk-original-scope-check', 'version': '1'}})
        require('serverInfo' in initialized, 'initialize_failed')
        self.notify('notifications/initialized')
        require(isinstance(self.request('tools/list', {})['tools'], list), 'catalog_missing')
        return self

    def notify(self, method):
        self.child.stdin.write(json.dumps({'jsonrpc': '2.0', 'method': method}) + '\n')
        self.child.stdin.flush()

    def request(self, method, parameters, deadline=None):
        self.counter += 1
        self.child.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': self.counter,
                                          'method': method, 'params': parameters}) + '\n')
        self.child.stdin.flush()
        deadline = deadline if deadline is not None else time.monotonic() + 90
        while True:
            remaining = deadline - time.monotonic()
            require(remaining > 0, 'transport_deadline')
            response = self.responses.get(timeout=remaining)
            self.trace.write(json.dumps(response) + '\n'); self.trace.flush()
            require(isinstance(response, dict), 'transport_closed')
            if 'id' not in response:
                continue
            require(response['id'] == self.counter and 'error' not in response, 'protocol_response_failed')
            return response['result']

    def tool(self, name, arguments=None, deadline=None):
        return self.request('tools/call', {'name': name, 'arguments': arguments or {'response_mode': 'full'}}, deadline)

    def close(self):
        failed = False
        try:
            if self.child is not None:
                try:
                    self.child.stdin.close()
                except (OSError, ValueError):
                    failed = True
                try:
                    self.child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    failed = True
                finally:
                    if self.child.poll() is None:
                        self.child.kill()
                        self.child.wait(timeout=10)
                failed |= self.child.returncode != 0
                self.child.stdout.close()
        finally:
            self.stderr.close()
            self.trace.close()
        require(not failed, 'owned_mcp_drain_failed')


def status(session, predicate, observations=None):
    deadline = time.monotonic() + 90
    while True:
        envelope = session.tool('status', deadline=deadline)
        value = body(envelope)
        code = value.get('index_error', {}).get('code') or value.get('error', {}).get('code')
        if observations is not None:
            observations.append(code if code in ('status_timeout', 'direct_query_setup',
                'PROJECT_NOT_REGISTERED', 'PROJECT_REMOVED', 'PROJECT_AMBIGUOUS') else 'other')
        if envelope.get('isError') is not True and predicate(value):
            return value
        require(code in ('status_timeout', 'direct_query_setup', 'direct_query_admission',
            'index_worker_unavailable', 'index_status_unavailable') and time.monotonic() < deadline,
            'unexpected_status_or_budget')
        # Independent metadata observation, never replay of an uncertain evidence query.
        time.sleep(0.05)


def negative(value):
    require(value.get('index_state') == 'unavailable' and value.get('publication') is None
            and value.get('files') is None and value.get('chunks') is None,
            'negative_scope_contains_index_availability')
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--approved-sha256', required=True)
    parser.add_argument('--task-root', type=Path, required=True)
    parser.add_argument('--report-tag', default='source-scope')
    parser.add_argument('--protected-root', action='append', type=Path, required=True)
    args = parser.parse_args()
    task, runtime = args.task_root.resolve(strict=True), args.runtime.resolve(strict=True)
    for protected in args.protected_root:
        protected = protected.resolve(strict=True)
        require(not task.is_relative_to(protected) and not protected.is_relative_to(task), 'state_source_overlap')
    require(digest(runtime / 'airk-install.json') == args.approved_sha256, 'unapproved_pair')
    scripts = task / 'scripts'
    copied = scripts / 'check-service-clients.py'
    require(digest(copied) == 'a4588c7202f3ba3aee27057bf0aa9e15dfeb627815ee61e3c8df249491d6984b', 'fixture_script_changed')
    require(digest(scripts / 'check-service-pool.py') == '6a35e24cb66497b722c6c252d23d7c6203dd6402be2eba25bdc935cd5102ae5f', 'pool_script_changed')
    spec = importlib.util.spec_from_file_location('scope_clients', copied)
    clients = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(clients)
    require(1 <= len(args.report_tag) <= 64 and args.report_tag not in ('.', '..')
        and all(c.isascii() and (c.isalnum() or c in '-_') for c in args.report_tag), 'invalid_report_tag')
    reports = task / 'reports' / args.report_tag
    reports.mkdir()  # Exclusive campaign; existing reports are never overwritten.
    result = {'schema': 1, 'completed': False, 'runtime_manifest_sha256': args.approved_sha256,
              'original_fixture_only': True, 'assertions': {}, 'cleanup': False}
    sessions, fixture, stage = [], None, 'setup'

    class Installed(clients.Fixture):
        def __init__(self, owner):
            super().__init__(runtime, owner)
            self.install, self.state = owner / 'install', owner / 'state'
            self.control, self.installed = self.state / 'control', False

        def activate(self):
            self.run('runtime', 'stage', '--source', runtime, '--install-dir', self.install,
                '--approved-sha256', args.approved_sha256, '--verification-basis', 'organization-digest',
                '--state-dir', self.state)
            self.run('runtime', 'activate', '--install-dir', self.install,
                '--runtime-sha256', args.approved_sha256)
            self.installed = True

        def detach(self):
            self.run('runtime', 'uninstall', '--install-dir', self.install)
            self.installed = False

        def close(self):
            try:
                if self.installed:
                    self.detach()
            finally:
                self.log.close()

    try:
        owner = Path(tempfile.mkdtemp(prefix='original-scope-', dir=task / 'fixtures')).resolve(strict=True)
        try:
            require(not owner.is_relative_to(runtime) and not runtime.is_relative_to(owner), 'payload_state_overlap')
            source, foreign = owner / 'source', owner / 'foreign'
            source.mkdir(); foreign.mkdir()
            original = {source / 'README.md': b'Original scopeconfigurationmarker configuration.\n',
                        source / 'entry.rs': b'fn configured_scope() { }\n',
                        foreign / 'README.md': b'Original foreignscopeonlymarker.\n'}
            for path, content in original.items():
                path.write_bytes(content)
            original_inventory = source_inventory([source, foreign])
            fixture = Installed(owner)
            try:
                fixture.activate()
                projects = fixture.rpc({'projects': {'after': None, 'revision': None, 'limit': 1}})
                require(projects.get('status') == 'projects' and projects.get('page', {}).get('projects') == [],
                    'fresh_registry_bootstrap_precondition_failed')
                result['authenticated_projects_bootstrap_precondition'] = True
                # Explicit read-only registry warmup, no diagnostics/evidence or cold-cache claim.
                def open_session(arguments, label, suffix=()):
                    session = Session(fixture, arguments, label, reports, suffix)
                    sessions.append(session)
                    return session.start()
                root_args = ['--service-install-dir', fixture.install, '--root', source]
                root = open_session(root_args, 'root')
                stage = 'initial_missing'
                first = []
                missing = negative(status(root, lambda v: v.get('registration_state') == 'missing', first))
                hit = negative(status(root, lambda v: v.get('registration_state') == 'missing'))
                require(hit.get('cached') is True and hit.get('observed_unix_ms') == missing.get('observed_unix_ms'), 'negative_cache_age_changed')
                result['first_status_codes'] = first
                result['assertions']['missing_cache_retains_age'] = True
                scope_key = root.request('tools/call', {'name': 'projects', 'arguments': {}})
                configured = body(scope_key)['default_project']

                def register(name, data, selected_source=source):
                    data.mkdir()
                    fixture.register(name, selected_source, data)
                def index(name):
                    fixture.configure(name, {'operation': 'vectors', 'enabled': False})
                    fixture.job(name, 'index')
                def evidence(session, collection, query='scopeconfigurationmarker', nonempty=True):
                    envelope = session.tool('search', {'query': query, 'mode': 'lexical', 'response_mode': 'full'})
                    value = body(envelope)
                    require(envelope.get('isError') is not True and value.get('collection') == collection, 'wrong_scope_evidence')
                    require(bool(value.get('hits')) == nonempty, 'wrong_scope_hits')
                    return value['publication']['token']

                stage = 'late_registration'
                register('first', owner / 'data-first'); index('first')
                late_status = status(root, lambda v: v.get('index_state') == 'available')
                token_first = evidence(root, 'first')
                require((late_status.get('publication') or {}).get('token') == token_first, 'late_registration_scalar_disagrees_with_evidence')
                pinned = open_session(['--service-install-dir', fixture.install, *fixture.arguments('first')[2:]], 'pinned-first')
                require(evidence(pinned, 'first') == token_first, 'pinned_source_publications_differ')
                result['assertions']['late_registration_and_pinned_evidence'] = True

                stage = 'ambiguity_and_isolation'
                register('second', owner / 'data-second'); index('second')
                negative(status(root, lambda v: v.get('registration_state') == 'ambiguous'))
                # --index-collection belongs after the MCP subcommand.
                filtered = open_session(root_args, 'filtered-second', ('--index-collection', 'second'))
                filtered_status = status(filtered, lambda v: v.get('index_state') == 'available')
                second_token = evidence(filtered, 'second')
                require(second_token != token_first, 'independent_collections_aliased')
                require((filtered_status.get('publication') or {}).get('token') == second_token, 'filtered_scalar_disagrees_with_evidence')
                require(evidence(pinned, 'first') == token_first, 'ambiguity_retargeted_pinned')
                register('foreign', owner / 'data-foreign', foreign); index('foreign')
                evidence(pinned, 'first', 'foreignscopeonlymarker', False)
                result['assertions']['ambiguity_filter_pinned_and_isolation'] = True

                def remove_fully(name):
                    saved_request, _ = fixture.remove(name)
                    binding = fixture.projects[name]['binding']
                    deadline = time.monotonic() + fixture.job_timeout
                    while time.monotonic() < deadline:
                        observed = fixture.rpc({'inactive_console': {'after': None,
                            'revision': None, 'limit': 32}}, deadline=deadline)
                        require(observed.get('status') == 'console', 'inactive_page_missing')
                        page = observed['page']
                        # At most three original fixture registrations exist.
                        # A partial page can never prove physical row absence.
                        require(page.get('after') is None, 'inactive_page_incomplete')
                        inactive = {row['project']['registration']['binding'] for row in page['projects']}
                        receipt = fixture.rpc(saved_request, deadline=deadline)
                        require(receipt.get('status') == 'request_receipt', 'removal_receipt_missing')
                        require(receipt.get('removed_project') in (None, binding), 'crossed_removal_proof')
                        if binding not in inactive and receipt.get('removed_project') == binding:
                            result.setdefault('full_purge_confirmed', []).append(name)
                            return
                        time.sleep(0.05)
                    raise AssertionError('full_purge_precondition_deadline')

                stage = 'tombstone_revision'
                remove_fully('second')
                negative(status(filtered, lambda v: v.get('registration_state') == 'removed'))
                fixture.register('first', source, owner / 'data-first')
                negative(status(filtered, lambda v: v.get('registration_state') == 'missing'))
                result['assertions']['active_sibling_tombstone_clear_invalidates_negative_cache'] = True

                stage = 'removal_and_readd'
                remove_fully('first')
                negative(status(root, lambda v: v.get('registration_state') == 'removed'))
                rejected = pinned.tool('search', {'query': 'scopeconfigurationmarker', 'mode': 'lexical'})
                require(rejected.get('isError') is True, 'removed_pinned_retargeted')
                register('first', owner / 'data-first-new'); index('first')
                readd_status = status(root, lambda v: v.get('index_state') == 'available')
                token_new = evidence(root, 'first')
                require((readd_status.get('publication') or {}).get('token') == token_new, 'first_readd_scalar_disagrees_with_evidence')
                require(token_new != token_first, 'readd_reused_publication')
                result['assertions']['removed_and_recreated_binding'] = True

                stage = 'namespace_replacement'
                # Prove the current pinned binding before namespace replacement;
                # this is distinct from the earlier already-removed pinned client.
                current_binding = fixture.projects['first']['binding']
                native_current_data = Path(fixture.projects['first']['data_root'])
                current_data = owner / 'data-first-new'
                require(native_current_data.samefile(current_data), 'current_data_identity_mismatch')
                current_pinned = open_session(['--service-install-dir', fixture.install,
                    *fixture.arguments('first')[2:]], 'pinned-current-namespace')
                require(evidence(current_pinned, 'first') == token_new, 'current_pinned_baseline_failed')
                status(current_pinned, lambda v: v.get('index_state') == 'available'
                    and v.get('publication', {}).get('token') == token_new)
                old_namespace = fixture.rpc('request_epoch')['namespace']
                # MCP is the exact approved main outside the disposable install;
                # the installed service/workers must drain before either rename.
                fixture.detach()
                retired = owner / 'retired-state'
                retired_data = owner / 'retired-current-data'
                require(fixture.state.resolve().is_relative_to(owner) and retired.parent.samefile(owner), 'unsafe_state_move')
                # Native canonical Windows paths may use a verbatim spelling.
                # Bind it by filesystem identity; move only our known owned path.
                metadata = current_data.lstat()
                require(current_data.parent.samefile(owner) and current_data.is_dir()
                    and not current_data.is_symlink()
                    and not getattr(metadata, 'st_file_attributes', 0) & 0x400
                    and native_current_data.samefile(current_data)
                    and current_data.resolve(strict=True).is_relative_to(owner.resolve(strict=True))
                    and retired_data.parent.samefile(owner) and not retired_data.exists(), 'unsafe_data_move')
                fixture.state.rename(retired)
                current_data.rename(retired_data)
                fixture.projects.clear()
                fixture.activate()
                require(fixture.rpc('request_epoch')['namespace'] != old_namespace, 'registry_namespace_did_not_change')
                register('first', current_data); index('first')
                require(fixture.projects['first']['binding'] == current_binding, 'namespace_binding_not_preserved')
                namespace_status = status(root, lambda v: v.get('index_state') == 'available')
                namespace_token = evidence(root, 'first')
                require((namespace_status.get('publication') or {}).get('token') == namespace_token, 'first_namespace_source_scalar_disagrees_with_evidence')
                require(namespace_token != token_new, 'namespace_reused_old_publication')
                require(body(root.request('tools/call', {'name': 'projects', 'arguments': {}}))['default_project'] == configured, 'configured_scope_changed')
                # Ordinary pinned reads may legitimately rebroker the same saved
                # binding. Observe the first post-replacement metadata outcome;
                # do not replay an uncertain evidence request to recover a pipe.
                first_pinned = current_pinned.tool('status')
                first_value = body(first_pinned)
                first_state = first_value.get('index_state')
                first_code = first_value.get('index_error', {}).get('code') or first_value.get('error', {}).get('code')
                first_token = (first_value.get('publication') or {}).get('token')
                require(first_state != 'available' or first_token == namespace_token,
                        'first_pinned_metadata_reused_old_publication')
                result['pinned_namespace_first_metadata'] = {
                    'is_error': first_pinned.get('isError') is True,
                    'index_state': first_state if first_state in ('available', 'unavailable', 'unknown') else 'unknown',
                    'code': first_code if first_code in ('status_timeout', 'status_unavailable',
                        'index_worker_unavailable', 'index_status_unavailable', 'direct_query_setup',
                        'direct_query_admission') else 'unknown',
                    'publication_matches_expected': first_token == namespace_token if first_token is not None else None}
                status(current_pinned, lambda v: v.get('index_state') == 'available'
                    and v.get('publication', {}).get('token') == namespace_token)
                require(evidence(current_pinned, 'first') == namespace_token, 'pinned_fresh_brokerage_failed')
                read_args = {'path': 'README.md', 'start_line': 1, 'end_line': 1,
                             'revision': namespace_token, 'response_mode': 'full'}
                current_read = current_pinned.tool('read_file', read_args)
                require(current_read.get('isError') is not True
                    and body(current_read).get('read', {}).get('publication', {}).get('token') == namespace_token,
                    'current_publication_read_failed')
                expired_read = current_pinned.tool('read_file', {**read_args, 'revision': token_new})
                require(expired_read.get('isError') is True, 'old_namespace_publication_accepted')
                result['assertions']['same_binding_new_namespace_replaces_source_backend'] = True
                result['assertions']['pinned_fresh_brokerage_and_old_publication_refusal'] = True
                require(source_inventory([source, foreign]) == original_inventory, 'source_inventory_or_bytes_changed')
                result['assertions']['source_bytes_preserved'] = True
                result['assertions']['complete_source_inventory_preserved'] = True
                result['completed'] = True
                stage = 'cleanup'
            finally:
                primary = sys.exc_info()[1]
                cleanup_errors = []
                for session in reversed(sessions):
                    try:
                        session.close()
                    except Exception:
                        cleanup_errors.append('session_cleanup_failed')
                try:
                    fixture.close()
                except Exception:
                    cleanup_errors.append('installation_cleanup_failed')
                result['cleanup'] = not cleanup_errors
                result['completed'] = result['completed'] and result['cleanup']
                if cleanup_errors:
                    if primary is not None:
                        primary.add_note('owned cleanup also failed')
                    else:
                        raise AssertionError('owned_cleanup_failed')
        finally:
            primary = sys.exc_info()[1]
            if result['cleanup']:
                try:
                    require(owner.resolve().is_relative_to((task / 'fixtures').resolve())
                        and owner != (task / 'fixtures').resolve(), 'unsafe_fixture_cleanup')
                    shutil.rmtree(owner)
                except Exception:
                    result['cleanup'] = False
                    result['completed'] = False
                    result['cleanup_error_code'] = 'owned_filesystem_cleanup_failed'
                    if primary is not None:
                        primary.add_note('owned final filesystem cleanup also failed')
                    else:
                        raise
    except Exception:
        result['failed_stage'] = stage
        raise
    finally:
        with (reports / 'report.json').open('x', encoding='utf-8') as stream:
            json.dump(result, stream, indent=2)
            stream.write('\n')
        print(json.dumps(result, indent=2), flush=True)


if __name__ == '__main__':
    main()
