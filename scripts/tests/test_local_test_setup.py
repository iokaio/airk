# SPDX-License-Identifier: Apache-2.0
"""Contracts of scripts/setup-local-test.py: verified runtime pair, native endpoint and
manual entry, loopback HTTP smoke, and `airkctl agents connect` only on request. Mocked; no service."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
import platform
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest import mock

import test_http_fixture as LOOPBACK

SCRIPT = Path(__file__).parents[1] / 'setup-local-test.py'
SPEC = importlib.util.spec_from_file_location('local_setup', SCRIPT)
setup = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(setup)

HOST_OS = {'win32': 'windows', 'darwin': 'macos', 'linux': 'linux'}[sys.platform]
HOST_ARCH = {'amd64': 'x86_64', 'x86_64': 'x86_64', 'arm64': 'aarch64', 'aarch64': 'aarch64'}[platform.machine().lower()]
SUFFIX = '.exe' if os.name == 'nt' else ''
PREFIX = 'AIRK.app/Contents/MacOS/' if sys.platform == 'darwin' else ''
REMOVED_FLOW = ('bearer', 'headersHelper', 'headers_helper', 'existing-connection-id', 'selection-file',
                'registration-file', 'http-connection', 'http-legacy', 'connection-id', 'catalog-session',
                'combined-session', 'retire-legacy', 'build-record', 'airk1.')


class RuntimeInventoryTests(unittest.TestCase):
    """verify_runtime accepts only the sealed, complete, current matched pair."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.runtime = Path(self.temporary.name).resolve() / 'runtime'
        self.runtime.mkdir()
        self.main = self.write(PREFIX + 'airk' + SUFFIX, b'fictional main image')
        self.cli = self.write(PREFIX + 'airkctl' + SUFFIX, b'fictional matched cli')
        self.seal()

    def write(self, name, data):
        path = self.runtime / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return path

    def digest(self, path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def seal(self, pair=None, manifest=None, files=None):
        pair = {'schema': 3, 'version': '1.8.207', 'os': HOST_OS, 'arch': HOST_ARCH,
                'airk_sha256': self.digest(self.main), 'airkctl_sha256': self.digest(self.cli), **(pair or {})}
        self.write('airk-runtime.json', json.dumps(pair).encode())
        listed = {PREFIX + 'airk' + SUFFIX: self.digest(self.main), PREFIX + 'airkctl' + SUFFIX: self.digest(self.cli),
                  'airk-runtime.json': self.digest(self.runtime / 'airk-runtime.json'), **(files or {})}
        body = {'schema': 1, 'version': pair['version'], 'os': pair['os'], 'arch': pair['arch'], 'files': listed,
                **(manifest or {})}
        raw = json.dumps(body).encode()
        self.write('airk-install.json', raw)
        self.approved = hashlib.sha256(raw).hexdigest()

    def verify(self, approved=None, topology='split_pair'):
        clients = mock.Mock()
        clients.classify_runtime.return_value = {'topology': topology}
        with mock.patch.object(setup, 'local_module', return_value=clients):
            return setup.verify_runtime(self.runtime, approved or self.approved)

    def test_sealed_current_pair_returns_the_identities_it_verified(self):
        identity = self.verify()
        self.assertEqual(identity, {'cli': self.cli, 'cli_sha256': self.digest(self.cli), 'version': '1.8.207',
                                    'main_sha256': self.digest(self.main)})

    def test_approval_must_be_the_exact_inventory_digest(self):
        for approved in (None, '', 'A' * 64, 'f' * 63, 'f' * 64):
            with self.subTest(approved=approved), self.assertRaises(ValueError):
                setup.verify_runtime(self.runtime, approved)

    def test_tampered_payload_unlisted_file_and_missing_file_are_refused(self):
        self.cli.write_bytes(b'replaced after sealing')
        with self.assertRaisesRegex(ValueError, 'seal differs'):
            self.verify()
        self.cli.write_bytes(b'fictional matched cli')
        extra = self.write('extra.bin', b'unlisted')
        with self.assertRaisesRegex(ValueError, 'Unlisted'):
            self.verify()
        extra.unlink()
        self.main.unlink()
        with self.assertRaises((ValueError, OSError)):
            self.verify()

    def test_single_image_and_pre_open_read_pairs_are_refused(self):
        self.seal(pair={'schema': 2})
        with self.assertRaisesRegex(ValueError, 'matched airk/airkctl'):
            self.verify()
        self.seal(pair={'version': '1.8.205'})
        with self.assertRaisesRegex(ValueError, 'current matched'):
            self.verify()

    def test_pair_must_agree_with_inventory_host_and_main_cli_digests(self):
        for pair in ({'os': 'plan9'}, {'arch': 'riscv'}, {'airk_sha256': 'e' * 64}, {'airkctl_sha256': 'e' * 64}):
            with self.subTest(pair=pair):
                self.seal(pair=pair)
                with self.assertRaises(ValueError):
                    self.verify()
        self.seal(manifest={'version': '1.8.999'})
        with self.assertRaisesRegex(ValueError, 'metadata identity'):
            self.verify()

    def test_obsolete_runtime_roles_and_wrong_topology_are_refused(self):
        self.write('airk-launcher' + SUFFIX, b'obsolete')
        self.seal(files={'airk-launcher' + SUFFIX: hashlib.sha256(b'obsolete').hexdigest()})
        with self.assertRaisesRegex(ValueError, 'obsolete runtime role'):
            self.verify()
        (self.runtime / ('airk-launcher' + SUFFIX)).unlink()
        self.seal()
        with self.assertRaisesRegex(ValueError, 'Invalid matched runtime pair'):
            self.verify(topology='single_image')

    def test_unsafe_inventory_names_are_refused(self):
        for name in ('../escape', '/absolute', 'back\\slash', 'a/./b', 'c:drive'):
            with self.subTest(name=name):
                self.seal(files={name: 'a' * 64})
                with self.assertRaises((ValueError, OSError)):
                    self.verify()


class NativeCommandTests(unittest.TestCase):
    def test_native_json_refuses_failure_and_oversize_output(self):
        for completed in (subprocess.CompletedProcess(['x'], 1, '{}', 'boom'),
                          subprocess.CompletedProcess(['x'], 0, ' ' * (setup.LIMIT + 1), '')):
            with self.subTest(code=completed.returncode), mock.patch.object(setup.subprocess, 'run', return_value=completed), \
                    self.assertRaises(ValueError):
                setup.native_json(Path('airkctl'), ['agents', 'endpoint'])

    def test_submitted_native_timeout_retains_uncertainty_and_is_not_replayed(self):
        error = subprocess.TimeoutExpired(['native-connect'], 125, output=b'partial output', stderr=b'partial error')
        with mock.patch.object(setup.subprocess, 'run', side_effect=error) as run:
            result = setup.native_outcome(Path('airkctl'), ['agents', 'connect'])
        run.assert_called_once()
        self.assertEqual(result['transport_outcome'], 'uncertain')
        self.assertIsNone(result['exit_code'])
        self.assertEqual(result['partial_stdout'], {'bytes': 14, 'sha256': hashlib.sha256(b'partial output').hexdigest()})

    def test_unreadable_completed_native_output_is_uncertain_even_after_zero_exit(self):
        completed = subprocess.CompletedProcess(['native-connect'], 0, 'partial JSON {', '')
        with mock.patch.object(setup.subprocess, 'run', return_value=completed) as run:
            outcome = setup.native_outcome(Path('airkctl'), ['agents', 'connect'])
        run.assert_called_once()
        self.assertEqual(outcome['exit_code'], 0)
        self.assertEqual(outcome['transport_outcome'], 'uncertain')
        self.assertEqual(outcome['response_failure'], 'invalid_native_response')

    def test_structured_refusal_is_kept_whole(self):
        refusal = {'code': 'agent_config_edit_refused', 'native': {'reason': 'automatic_policy'}}
        completed = subprocess.CompletedProcess(['native-connect'], 4, '', json.dumps(refusal))
        with mock.patch.object(setup.subprocess, 'run', return_value=completed):
            self.assertEqual(setup.native_outcome(Path('airkctl'), ['agents', 'connect']),
                             {'exit_code': 4, 'result': refusal})


class ActualHttpSmokeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        LOOPBACK.ActualHttpFixtureTests.setUpClass()

    @classmethod
    def tearDownClass(cls):
        LOOPBACK.ActualHttpFixtureTests.tearDownClass()

    def test_real_http_smoke_checks_version_revision_source_hash_and_closes_only_its_session(self):
        fixture = LOOPBACK.ActualHttpFixtureTests()
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            content = b'# Original fixture\nVerificationMarker source evidence.\n'
            (root / 'README.md').write_bytes(content)
            source_hash = hashlib.sha256(content).hexdigest()
            calls = []
            class Handler(BaseHTTPRequestHandler):
                protocol_version = 'HTTP/1.1'
                def log_message(self, *_):
                    pass
                def respond(self, status, value=None):
                    data = json.dumps(value).encode() if value is not None else b''
                    self.send_response(status)
                    self.send_header('Content-Type', 'application/json')
                    self.send_header('Content-Length', str(len(data)))
                    self.send_header('Mcp-Session-Id', 'owned-setup-fixture')
                    self.end_headers()
                    self.wfile.write(data)
                def do_POST(self):
                    # U110: the shared HTTP client never sends a credential.
                    if self.headers.get('Authorization') is not None:
                        self.respond(403); return
                    value = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                    calls.append(value)
                    if 'id' not in value:
                        self.respond(202); return
                    method = value['method']
                    if method == 'initialize':
                        result = {'protocolVersion': '2025-11-25', 'serverInfo': {'name': 'airk', 'version': '1.8.14'}}
                    elif method == 'tools/list':
                        result = {'tools': [{'name': name} for name in ('status', 'search', 'read_file')]}
                    else:
                        name = value['params']['name']
                        body = ({'collection': 'original', 'index_state': 'available', 'files': 1, 'publication': {'token': 'exact-saved-revision'}}
                                if name == 'status' else {'hits': [{'text': content.decode(), 'citation': {'path': 'README.md', 'content_hash': source_hash}}]})
                        result = {'content': [{'type': 'text', 'text': json.dumps(body)}], 'isError': False}
                    self.respond(200, {'jsonrpc': '2.0', 'id': value['id'], 'result': result})
                def do_DELETE(self):
                    calls.append({'method': 'DELETE'})
                    self.respond(204)
            server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
            thread = threading.Thread(target=server.serve_forever, kwargs={'poll_interval': .02}, daemon=True)
            thread.start()
            info = {'schema': 1, 'connection_id': fixture.identity, 'selection': {'kind': 'scoped', 'selection': {'namespace': 'fixture'}},
                    'endpoint': {'schema': 1, 'namespace': 'fixture', 'state': 'ready', 'url': f'http://127.0.0.1:{server.server_port}/mcp'}}
            try:
                with mock.patch.object(setup.subprocess, 'Popen', side_effect=AssertionError('No stdio/native process in generic HTTP smoke')):
                    result = setup.http_smoke(info, fixture.root, '1.8.14', root, 'original')
                self.assertEqual(result['source_sha256'], source_hash)
                self.assertEqual(result['codex_client_execution'], 'not_run')
                self.assertEqual(result['transport'], 'http')
                self.assertEqual(result['tool_count'], 3)
                read = next(value for value in calls if value.get('params', {}).get('name') == 'read_file')
                self.assertEqual(read['params']['arguments']['revision'], 'exact-saved-revision')
                self.assertEqual(read['params']['arguments']['verification'], 'require')
                self.assertEqual(calls[-1]['method'], 'DELETE')
                self.assertEqual((root / 'README.md').read_bytes(), content)
                # The current helper path uses the bare native endpoint alone,
                # with an explicit scope on every read and no credential file.
                control = root / 'control'; (control / 'http-1').mkdir(parents=True)
                calls.clear()
                result = setup.http_smoke(info['endpoint'], control, '1.8.14', root, 'original',
                                           scope={'index_ids': ['idx_fixture']})
                scoped = [call for call in calls if call.get('method') == 'tools/call']
                self.assertEqual(len(scoped), 3)
                self.assertTrue(all(call['params']['arguments']['index_ids'] == ['idx_fixture'] for call in scoped))
                self.assertEqual(list((control / 'http-1').iterdir()), [], 'Nothing is read or written below the control directory')
                self.assertEqual(result['status'], 'passed')
            finally:
                server.shutdown(); server.server_close(); thread.join(timeout=5)
                self.assertFalse(thread.is_alive())



class SetupFlowTests(unittest.TestCase):
    """setup() drives only `airkctl agents endpoint|manual|connect`; Python never edits configuration."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.control = self.root / 'control'
        self.control.mkdir()
        self.config = self.root / '.codex' / 'config.toml'
        self.config.parent.mkdir()
        self.config.write_bytes(b'# keep every unrelated byte\n[mcp_servers.other]\nurl = "https://example.invalid/mcp"\n')
        self.cli = self.root / 'runtime' / ('airkctl' + SUFFIX)
        self.cli.parent.mkdir()
        self.cli.write_bytes(b'fictional matched CLI')
        self.identity = {'cli': self.cli, 'cli_sha256': hashlib.sha256(self.cli.read_bytes()).hexdigest(),
                         'version': '1.8.207', 'main_sha256': 'a' * 64}
        self.endpoint = {'schema': 1, 'url': 'http://127.0.0.1:57422/mcp', 'executable_sha256': 'a' * 64}
        # Text is an opaque native fixture output, not a Python config builder.
        self.manual = {'client': 'codex', 'scope': 'project', 'file': str(self.config), 'format': 'toml',
                       'text': 'EXACT NATIVE MANUAL TEXT \" \\ \n'}
        self.calls = []
        self.scope = {'index_ids': ['idx_fixture']}

    def run_setup(self, *, outcome=None, persisted=None, manual=None, smoke=None, scope=None, root=None, **options):
        before = self.config.read_bytes()

        def native(binary, arguments):
            self.assertEqual(binary, self.cli)
            self.calls.append(list(map(str, arguments)))
            self.assertEqual(arguments[:4], ['--output', 'json', '--service-runtime-dir', self.control.resolve()])
            if arguments[4:] == ['agents', 'endpoint']:
                return {**self.endpoint, 'manual': [self.manual]}
            self.assertEqual(arguments[4:], ['agents', 'manual', '--client', 'codex', '--project', self.root])
            return manual or self.manual
        http = mock.Mock()
        http.read_endpoint.return_value = persisted or self.endpoint
        http.scope_value.side_effect = lambda **value: value
        with mock.patch.object(setup, 'verify_runtime', return_value=self.identity), \
                mock.patch.object(setup, 'local_module', return_value=http), \
                mock.patch.object(setup, 'native_json', side_effect=native), \
                mock.patch.object(setup, 'native_outcome', return_value=outcome) as edit, \
                mock.patch.object(setup.subprocess, 'Popen', side_effect=AssertionError('No process may be started directly')), \
                mock.patch.object(setup, 'http_smoke', side_effect=smoke,
                                  return_value={'status': 'passed', 'codex_client_execution': 'not_run'}) as probe:
            try:
                result = setup.setup(Path('runtime'), 'f' * 64, self.control, root or self.root,
                                     scope=self.scope if scope is None else scope, **options)
            finally:
                self.edit, self.probe = edit, probe
        self.assertEqual(self.config.read_bytes(), before, 'the script itself must never write the configuration')
        return result, edit, probe

    def test_manual_setup_reads_endpoint_and_manual_entry_and_changes_nothing(self):
        result, edit, probe = self.run_setup()
        edit.assert_not_called()
        probe.assert_called_once_with(self.endpoint, self.control.resolve(), '1.8.207', self.root, scope=self.scope)
        self.assertEqual([call[4:] for call in self.calls],
                         [['agents', 'endpoint'], ['agents', 'manual', '--client', 'codex', '--project', str(self.root)]])
        self.assertEqual(result['status'], 'manual_configuration_ready')
        self.assertEqual(result['manual'], self.manual)
        self.assertEqual(result['endpoint'], self.endpoint)
        self.assertEqual(result['read_scope'], self.scope)
        self.assertIsNone(result['native_edit'])
        self.assertFalse(result['indexed'])
        self.assertFalse(result['client_environment_loaded'])
        self.assertEqual(result['configuration_preimage_sha256'], hashlib.sha256(self.config.read_bytes()).hexdigest())
        self.assertEqual(list(self.control.iterdir()), [])
        self.assertEqual(sorted(path.name for path in self.root.iterdir()), ['.codex', 'control', 'runtime'])

    def test_apply_runs_native_connect_once_with_the_project_and_no_credential_arguments(self):
        outcome = {'exit_code': 0, 'result': {'schema': 1, 'client': 'codex', 'url': self.endpoint['url'], 'changed': True}}
        result, edit, _ = self.run_setup(outcome=outcome, apply_config=True)
        self.assertEqual(result['status'], 'configuration_saved')
        self.assertEqual(result['native_edit'], outcome)
        edit.assert_called_once()
        self.assertEqual(edit.call_args.args[0], self.cli)
        self.assertEqual(edit.call_args.args[1],
                         ['--output', 'json', '--service-runtime-dir', self.control.resolve(),
                          'agents', 'connect', '--client', 'codex', '--project', self.root])

    def test_no_native_call_names_the_removed_enrollment_flow(self):
        outcome = {'exit_code': 0, 'result': {}}
        self.run_setup(outcome=outcome, apply_config=True)
        words = [str(argument) for call in (*self.calls, edit_arguments(self.edit)) for argument in call]
        for removed in REMOVED_FLOW:
            self.assertFalse(any(removed in word for word in words), removed)

    def test_connect_refusals_and_uncertain_outcomes_are_retained_without_python_retry(self):
        for reason, status in (('automatic_policy', 'manual_configuration_required'),
                               ('conflict_retained', 'configuration_conflict_retained'),
                               ('outcome_unknown', 'configuration_outcome_unknown'), ('unlisted', 'native_setup_failed')):
            outcome = {'exit_code': 4, 'result': {'code': 'native_fixture',
                       'native': {'reason': reason, 'detail': 'complete native details', 'manual': self.manual}}}
            with self.subTest(reason=reason):
                result, edit, _ = self.run_setup(outcome=outcome, apply_config=True)
                self.assertEqual(result['status'], status)
                self.assertEqual(result['native_edit'], outcome)
                self.assertEqual(edit.call_count, 1)
        bare = {'exit_code': 1, 'result': {'code': 'operation_failed'}}
        self.assertEqual(self.run_setup(outcome=bare, apply_config=True)[0]['status'], 'native_setup_failed')
        unknown = {'exit_code': None, 'transport_outcome': 'uncertain'}
        self.assertEqual(self.run_setup(outcome=unknown, apply_config=True)[0]['status'], 'configuration_outcome_unknown')

    def test_endpoint_must_match_the_persisted_record_and_the_approved_running_image(self):
        for persisted in ({**self.endpoint, 'url': 'http://127.0.0.1:60000/mcp'}, {**self.endpoint, 'schema': 2}):
            with self.subTest(persisted=persisted), self.assertRaisesRegex(ValueError, 'Native endpoint differs'):
                self.run_setup(persisted=persisted, outcome={'exit_code': 0, 'result': {}}, apply_config=True)
        self.identity['main_sha256'] = 'b' * 64
        with self.assertRaisesRegex(ValueError, 'Native endpoint differs'):
            self.run_setup(outcome={'exit_code': 0, 'result': {}}, apply_config=True)
        self.edit.assert_not_called()

    def test_manual_entry_must_target_this_checkouts_codex_project_file(self):
        for change in ({'client': 'claude'}, {'scope': 'user'}, {'format': 'json'}, {'file': str(self.root / 'other.toml')}, {'text': None}):
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, 'manual entry target differs'):
                self.run_setup(manual={**self.manual, **change}, outcome={'exit_code': 0, 'result': {}}, apply_config=True)
            self.edit.assert_not_called()

    def test_explicit_read_scope_is_required_before_any_native_work(self):
        with self.assertRaisesRegex(ValueError, 'Explicit index'):
            self.run_setup(scope={})
        self.assertEqual(self.calls, [])

    def test_modified_airkctl_never_runs(self):
        self.identity['cli_sha256'] = 'c' * 64
        with self.assertRaisesRegex(ValueError, 'airkctl identity differs'):
            self.run_setup()
        self.assertEqual(self.calls, [])

    def test_configuration_changed_during_smoke_blocks_connect(self):
        def edit_during_smoke(*_, **__):
            self.config.write_bytes(b'changed by someone else\n')
            return {'status': 'passed'}
        with self.assertRaisesRegex(ValueError, 'changed during verification'):
            self.run_setup(smoke=edit_during_smoke, outcome={'exit_code': 0, 'result': {}}, apply_config=True)
        self.edit.assert_not_called()

    def test_home_directory_is_never_a_root(self):
        with mock.patch.object(setup.Path, 'home', return_value=self.root), self.assertRaisesRegex(ValueError, 'User-level'):
            self.run_setup()
        self.assertEqual(self.calls, [])

    def test_relative_control_directory_is_refused(self):
        with self.assertRaisesRegex(ValueError, 'absolute control'):
            setup.setup(Path('runtime'), 'f' * 64, Path('control'), self.root, scope=self.scope)

    def test_report_is_created_once_under_artifacts_local(self):
        report = self.root / 'artifacts' / 'local' / 'run' / 'agent-setup.json'
        result, _, _ = self.run_setup(report=report)
        self.assertEqual(json.loads(report.read_text(encoding='utf-8')), json.loads(json.dumps(result)))
        self.assertEqual(result['schema'], 'airk-local-agent-setup/3')
        with self.assertRaisesRegex(ValueError, 'already exists'):
            self.run_setup(report=report)
        for outside in (self.root / 'report.json', self.root / 'artifacts' / 'report.json'):
            with self.subTest(outside=outside), self.assertRaisesRegex(ValueError, 'artifacts/local'):
                self.run_setup(report=outside)
            self.assertFalse(outside.exists())

    def test_no_selection_registration_or_credential_file_is_ever_created(self):
        self.run_setup(outcome={'exit_code': 0, 'result': {}}, apply_config=True)
        names = [path.name for path in self.root.rglob('*')]
        for removed in ('selection', 'registration', 'credential', 'token', '.tmp'):
            self.assertFalse(any(removed in name for name in names), removed)


def edit_arguments(edit):
    return [str(argument) for call in edit.call_args_list for argument in call.args[1]]


class CommandLineTests(unittest.TestCase):
    RUNTIME = ['--runtime-dir', 'runtime', '--approved-sha256', 'f' * 64, '--control-dir', 'control']

    def invoke(self, arguments):
        output, errors = io.StringIO(), io.StringIO()
        with mock.patch.object(sys, 'argv', ['setup-local-test.py', *arguments]), \
                contextlib.redirect_stdout(output), contextlib.redirect_stderr(errors):
            try:
                setup.main()
                code = 0
            except SystemExit as error:
                code = error.code
        return code, output.getvalue(), errors.getvalue()

    def test_removed_flow_options_do_not_parse(self):
        for option in (['--build-record', 'record'], ['--install-dir', 'install'], ['--connection-id', 'a' * 32],
                       ['--selection-file', 'selection.json'], ['--registration-file', 'registration.json'],
                       ['--retire-obsolete-executables'], ['--retire-legacy-launchers'], ['--data-dir', 'data'],
                       ['--new-data-dir', 'data'], ['--skip-index']):
            with self.subTest(option=option), mock.patch.object(setup, 'setup') as run:
                code, _, errors = self.invoke([*self.RUNTIME, '--index', 'idx', *option])
                self.assertEqual(code, 2)
                self.assertIn('unrecognized arguments', errors)
                run.assert_not_called()

    def test_runtime_approval_and_control_directory_are_required(self):
        for omitted in ('--runtime-dir', '--approved-sha256', '--control-dir'):
            arguments = list(self.RUNTIME)
            position = arguments.index(omitted)
            del arguments[position:position + 2]
            with self.subTest(omitted=omitted), mock.patch.object(setup, 'setup') as run:
                self.assertEqual(self.invoke([*arguments, '--index', 'idx'])[0], 2)
                run.assert_not_called()

    def test_scope_arguments_map_to_the_per_call_read_scope(self):
        receipt = {'status': 'manual_configuration_ready'}
        with mock.patch.object(setup, 'setup', return_value=receipt) as run:
            code, output, _ = self.invoke([*self.RUNTIME, '--index', 'a', '--index', 'b', '--index-set', 's', '--root', str(Path.cwd())])
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(output), receipt)
        self.assertEqual(run.call_args.kwargs, {'scope': {'index_ids': ['a', 'b'], 'index_set_ids': ['s']},
                                                'apply_config': False, 'report': None})
        with mock.patch.object(setup, 'setup', return_value=receipt) as run:
            self.assertEqual(self.invoke([*self.RUNTIME, '--workspace-default', '--apply-config', '--root', str(Path.cwd())])[0], 0)
        self.assertEqual(run.call_args.kwargs['scope'], {'workspace': str(Path.cwd().resolve())})
        self.assertTrue(run.call_args.kwargs['apply_config'])

    def test_named_scope_and_workspace_default_are_mutually_exclusive(self):
        with mock.patch.object(setup, 'setup') as run:
            code, _, errors = self.invoke([*self.RUNTIME, '--workspace-default', '--index', 'a'])
        self.assertEqual(code, 1)
        self.assertIn('Choose named indexes/sets or the existing workspace default', errors)
        run.assert_not_called()

    def test_only_saved_or_manual_statuses_exit_successfully(self):
        for status, code in (('manual_configuration_ready', 0), ('configuration_saved', 0), ('manual_configuration_required', 1),
                             ('configuration_conflict_retained', 1), ('configuration_outcome_unknown', 1), ('native_setup_failed', 1)):
            with self.subTest(status=status), mock.patch.object(setup, 'setup', return_value={'status': status}):
                self.assertEqual(self.invoke([*self.RUNTIME, '--index', 'a'])[0], code)

    def test_refusal_exits_one_with_the_reason(self):
        with mock.patch.object(setup, 'setup', side_effect=ValueError('Runtime inventory approval differs')):
            code, output, errors = self.invoke([*self.RUNTIME, '--index', 'a'])
        self.assertEqual((code, output), (1, ''))
        self.assertIn('Local agent setup failed: Runtime inventory approval differs', errors)


class RemovedFlowTests(unittest.TestCase):
    def test_script_contains_no_enrollment_credential_or_selection_flow(self):
        text = SCRIPT.read_text(encoding='utf-8')
        for removed in REMOVED_FLOW:
            self.assertNotIn(removed, text, removed)
        for name in ('setup_http', 'setup_http_legacy', 'prepare_http_config', 'prepare_config', 'write_config',
                     'owned_server', 'smoke', 'prepare_vectors', 'tokenless_setup'):
            self.assertFalse(hasattr(setup, name), name)

    def test_script_runs_only_the_pairs_airkctl_agents_commands(self):
        text = SCRIPT.read_text(encoding='utf-8')
        for command in ("'agents', 'endpoint'", "'agents', 'manual'", "'agents', 'connect'"):
            self.assertIn(command, text)
        self.assertNotIn("'runtime'", text)
        self.assertNotIn("'setup'", text)


if __name__ == '__main__':
    unittest.main()
