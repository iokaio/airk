# SPDX-License-Identifier: Apache-2.0
"""Historical single-helper setup contracts; native fixtures require pre-1.4 inputs."""
import importlib.util
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib
import platform
import sys
import ssl
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import unittest
from unittest import mock
import test_http_fixture as TLS

SPEC = importlib.util.spec_from_file_location('local_setup', Path(__file__).parents[1] / 'setup-local-test.py')
setup = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(setup)


class LocalSetupTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name).resolve()
        self.helper = self.root / 'new' / 'airk.exe'
        self.storage = self.root / 'state'
        self.args = ['--root', str(self.root), '--data-dir', str(self.storage), '--collection', 'fixture', 'mcp']

    def source(self, extra='', env=''):
        return ('# keep this comment\nmodel = "unchanged"\n'
                '[mcp_servers.other]\ncommand = "other"\n'
                '[mcp_servers.airk-fixture] # preserve table comment\n'
                'command = "C:/old/airk.exe" # preserve command comment\n'
                f'args = {json.dumps(self.args)}\n{extra}'
                f'{env}').encode()

    def test_upgrade_preserves_comments_storage_policies_and_other_servers(self):
        before = self.source('enabled_tools = ["airk_status", "airk_search", "airk_read_file"]\n',
                             '[mcp_servers.airk-fixture.env]\nCUSTOM = "keep"\nAIRK_CLIENT = "unknown" # keep client comment\n')
        after, name, server = setup.prepare_config(before, self.root, self.helper)
        self.assertEqual(name, 'airk-fixture')
        self.assertEqual(server['args'], self.args)
        self.assertEqual(server['env'], {'CUSTOM': 'keep', 'AIRK_CLIENT': 'codex'})
        self.assertIn(b'# preserve command comment', after)
        self.assertIn(b'# keep client comment', after)
        self.assertEqual(tomllib.loads(after.decode())['mcp_servers']['other'], {'command': 'other'})
        self.assertEqual(setup.prepare_config(after, self.root, self.helper)[0], after)

    def test_first_setup_requires_known_storage_and_preserves_existing_model(self):
        with self.assertRaisesRegex(ValueError, 'requires --data-dir'):
            setup.prepare_config(b'model = "keep"\n', self.root, self.helper)
        after, _, server = setup.prepare_config(b'model = "keep"\n', self.root, self.helper, self.storage)
        self.assertEqual(tomllib.loads(after.decode())['model'], 'keep')
        self.assertEqual(setup.binding(server['args'])['--data-dir'], str(self.storage))

    def test_default_storage_does_not_rebind_existing_collection(self):
        _, _, server = setup.prepare_config(self.source(), self.root, self.helper, new_data_dir=self.root / 'other')
        self.assertEqual(server['args'], self.args)
        with self.assertRaisesRegex(ValueError, 'storage differs'):
            setup.prepare_config(self.source(), self.root, self.helper, data_dir=self.root / 'other')

    def test_disabled_and_filtered_tools_are_not_overridden(self):
        for extra in ('enabled = false\n', 'disabled_tools = ["airk_search"]\n', 'enabled_tools = ["airk_status"]\n'):
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                setup.prepare_config(self.source(extra), self.root, self.helper)

    def test_recording_opt_out_is_preserved(self):
        _, _, server = setup.prepare_config(self.source(env='[mcp_servers.airk-fixture.env]\nAIRK_USAGE_MODE = "disabled"\n'), self.root, self.helper)
        self.assertEqual(server['env']['AIRK_USAGE_MODE'], 'disabled')

    def test_unsupported_layout_and_duplicate_binding_fail_without_write(self):
        for before in (self.source('env = {CUSTOM = "keep"}\n'),
                       self.source() + f'[mcp_servers.airk-duplicate]\ncommand = "airk.exe"\nargs = {json.dumps(self.args)}\n'.encode(),
                       self.source().replace(b'C:/old/airk.exe', b'C:/custom/wrapper.exe')):
            with self.subTest(before=before), self.assertRaises((ValueError, tomllib.TOMLDecodeError)):
                setup.prepare_config(before, self.root, self.helper)

    def test_crlf_and_quoted_windows_paths_survive(self):
        before = self.source().replace(b'\n', b'\r\n')
        after, _, server = setup.prepare_config(before, self.root, self.helper)
        self.assertNotIn(b'\n', after.replace(b'\r\n', b''))
        self.assertEqual(server['command'], str(self.helper))

    def test_concurrent_edits_and_hardlinks_are_preserved(self):
        config = self.root / '.codex' / 'config.toml'
        setup.write_config(config, None, self.source())
        config.write_bytes(b'# concurrent edit\n')
        with self.assertRaisesRegex(ValueError, 'changed during setup'):
            setup.write_config(config, self.source(), b'new')
        self.assertEqual(config.read_bytes(), b'# concurrent edit\n')
        link = self.root / 'linked'
        os.link(config, link)
        with self.assertRaisesRegex(ValueError, 'regular file'):
            setup.read_config(config)

    def test_read_only_config_leaves_no_temporary_copy(self):
        config = self.root / 'config.toml'
        config.write_bytes(self.source())
        config.chmod(0o400)
        try:
            with self.assertRaisesRegex(ValueError, 'read-only'):
                setup.write_config(config, self.source(), b'changed')
            self.assertEqual(config.read_bytes(), self.source())
            self.assertEqual(list(self.root.iterdir()), [config])
        finally:
            config.chmod(0o600)

    def candidate(self):
        run = self.root / 'artifacts' / 'local' / 'fixture'
        helper = run / 'native' / 'stage' / 'airk.exe'
        helper.parent.mkdir(parents=True)
        helper.write_bytes(b'fictional helper')
        publish = run / 'native' / 'publish'
        publish.mkdir()
        (publish / 'manifest.json').write_text(json.dumps({'schema': 'airk-native-candidate/1', 'version': '1.1.4',
            'target': 'x86_64-pc-windows-msvc', 'payload': {'airk.exe': setup.hashlib.sha256(helper.read_bytes()).hexdigest()}}))
        (run / 'result.json').write_text(json.dumps({'schema': 'airk-local-build/1', 'vsix': 'fictional.vsix'}))
        (run / 'local-install.json').write_text(json.dumps({'schema': 'airk-local-install/1', 'version': '1.1.4', 'vsix': 'fictional.vsix'}))
        config = self.root / '.codex' / 'config.toml'
        setup.write_config(config, None, self.source())
        return run, helper, config

    def test_vector_setup_preserves_opt_out_without_download_or_install(self):
        server = {'command': str(self.helper), 'args': self.args}
        with mock.patch.object(setup.subprocess, 'check_output', return_value='{"enabled": false}'), \
                mock.patch.object(setup.subprocess, 'run') as execute:
            setup.prepare_vectors(server, self.root)
            execute.assert_not_called()

    def test_vector_setup_installs_missing_model_and_propagates_failure(self):
        server = {'command': str(self.helper), 'args': self.args}
        with mock.patch.object(setup.subprocess, 'check_output', return_value='{"enabled": true}'), \
                mock.patch.object(setup.subprocess, 'run') as execute:
            setup.prepare_vectors(server, self.root)
            self.assertEqual(execute.call_count, 1)
            self.assertEqual(execute.call_args.args[0][-3:], ['collections', 'vectors', 'install-bundled'])
            self.assertNotIn('--download', execute.call_args.args[0])
            execute.reset_mock()
            execute.side_effect = subprocess.CalledProcessError(1, 'provision')
            with self.assertRaisesRegex(RuntimeError, 'Native bundled model installation failed'):
                setup.prepare_vectors(server, self.root)
            self.assertEqual(execute.call_count, 1)

    def test_hidden_child_failure_reports_native_stderr_and_stdout(self):
        command = [setup.sys.executable, '-c',
                   'import sys; print("stage details"); '
                   'print("another process holds the writer lock", file=sys.stderr); sys.exit(1)']
        with self.assertRaises(RuntimeError) as failure:
            setup.run_setup_step('Native model installation', command, timeout=10, **setup.process_options())
        self.assertIn('Native model installation failed (exit 1)', str(failure.exception))
        self.assertIn('stage details', str(failure.exception))
        self.assertIn('another process holds the writer lock', str(failure.exception))

    def test_setup_timeout_retains_bounded_diagnostics(self):
        error = subprocess.TimeoutExpired('helper', 300, stderr=b'x' * 20000 + b'last diagnostic')
        with mock.patch.object(setup.subprocess, 'run', side_effect=error):
            with self.assertRaises(RuntimeError) as failure:
                setup.run_setup_step('Native model installation', ['helper'], timeout=300)
        self.assertIn('timed out after 300 seconds', str(failure.exception))
        self.assertTrue(str(failure.exception).endswith('last diagnostic'))
        self.assertLess(len(str(failure.exception)), 16500)

    def writer_busy(self):
        return subprocess.CalledProcessError(1, 'helper', stderr=(
            'AIRK local usage recording dropped an observation; retrieval is unaffected.\n'
            'Error: index writer locked\n\nCaused by:\n'
            '    lock acquisition failed because the operation would block\n'))

    def test_writer_contention_retries_same_command_with_remaining_deadline(self):
        clock = [100.0]
        def sleep(seconds):
            clock[0] += seconds
        with mock.patch.object(setup.time, 'monotonic', side_effect=lambda: clock[0]), \
                mock.patch.object(setup.time, 'sleep', side_effect=sleep) as pause, \
                mock.patch.object(setup.subprocess, 'run', side_effect=[self.writer_busy(),
                    subprocess.CompletedProcess(['helper', 'index'], 0, stdout='{"generation":9}')]) as execute:
            result = setup.run_setup_step('Workspace indexing', ['helper', 'index'], timeout=20, wait_for_writer=True)
        self.assertEqual(result, '{"generation":9}')
        self.assertEqual(execute.call_count, 2)
        self.assertEqual(execute.call_args_list[0].args, execute.call_args_list[1].args)
        self.assertEqual([call.kwargs['timeout'] for call in execute.call_args_list], [20, 15])
        pause.assert_called_once_with(5)

    def test_writer_wait_expires_without_restarting_timeout_or_forcing_unlock(self):
        clock = [100.0]
        def sleep(seconds):
            clock[0] += seconds
        with mock.patch.object(setup.time, 'monotonic', side_effect=lambda: clock[0]), \
                mock.patch.object(setup.time, 'sleep', side_effect=sleep) as pause, \
                mock.patch.object(setup.subprocess, 'run', side_effect=self.writer_busy()) as execute:
            with self.assertRaisesRegex(RuntimeError, 'timed out after 7 seconds waiting for the collection writer'):
                setup.run_setup_step('Workspace indexing', ['helper', 'index'], timeout=7, wait_for_writer=True)
        self.assertEqual(execute.call_count, 2)
        self.assertEqual([call.args[0] for call in pause.call_args_list], [5, 2])

    def test_other_errors_and_unrequested_wait_fail_without_retry(self):
        failures = [self.writer_busy(),
            subprocess.CalledProcessError(1, 'helper', stderr='Error: quota exceeded'),
            subprocess.CalledProcessError(1, 'helper', stderr='Error: legacy index writer locked; see recovery'),
            subprocess.CalledProcessError(1, 'helper', stderr='Error: index writer locked\nCaused by:\nAccess denied')]
        for index, failure in enumerate(failures):
            with self.subTest(index=index), mock.patch.object(setup.subprocess, 'run', side_effect=failure) as execute, \
                    mock.patch.object(setup.time, 'sleep') as pause:
                with self.assertRaises(RuntimeError):
                    setup.run_setup_step('Workspace indexing', ['helper', 'index'], timeout=20, wait_for_writer=index != 0)
                execute.assert_called_once()
                pause.assert_not_called()

    @unittest.skipUnless(os.environ.get('AIRK_LEGACY_TEST_BINARY'), 'Set AIRK_LEGACY_TEST_BINARY to a pre-1.4 standalone helper for historical writer-lock retry')
    def test_actual_native_writer_lock_is_retried_after_owner_releases_it(self):
        binary = str(Path(os.environ['AIRK_LEGACY_TEST_BINARY']).resolve())
        (self.root / 'README.md').write_text('Original lock contention fixture.\n', encoding='utf-8')
        command = [binary, *self.args[:-1]]
        subprocess.run([*command, 'collections', 'vectors', 'set', '--enabled', 'false'],
                       check=True, capture_output=True, **setup.process_options())
        with (self.storage / 'collections-1.2' / '.writer-locks' / 'fixture').open('r+b') as lock:
            if os.name == 'nt':
                import msvcrt
                msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
                def unlock():
                    lock.seek(0)
                    msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                import fcntl
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                def unlock():
                    fcntl.flock(lock, fcntl.LOCK_UN)
            released = False
            def release(_seconds):
                nonlocal released
                unlock()
                released = True
            try:
                with mock.patch.object(setup.time, 'sleep', side_effect=release) as pause:
                    result = setup.run_setup_step('Workspace indexing', [*command, 'index'],
                                                 timeout=30, wait_for_writer=True, **setup.process_options())
                pause.assert_called_once()
                self.assertEqual(json.loads(result)['indexed'], 1)
            finally:
                if not released:
                    unlock()

    def test_index_measurements_preserve_zero_without_exporting_paths_or_inventing_old_counters(self):
        report = {'generation': 7, 'embedding_inferences': 0, 'embedding_cache_hits': 12,
                  'skipped': [{'path': 'private source', 'reason': 'private diagnostic'}],
                  'collection': 'private collection'}
        result = setup.indexing_measurements(report, 1.25)
        self.assertEqual(result, {'wall_ms': 1250.0, 'generation': 7,
                                 'embedding_inferences': 0, 'embedding_cache_hits': 12})
        with self.assertRaisesRegex(ValueError, 'Invalid indexing measurement'):
            setup.indexing_measurements({'embedding_workers': -1}, 1)

    def test_vector_setup_preserves_custom_model_and_rejects_incomplete_assets(self):
        local = self.storage / 'models' / 'local'
        local.mkdir(parents=True)
        server = {'command': str(self.helper), 'args': self.args}
        with mock.patch.object(setup.subprocess, 'check_output', return_value='{"enabled": true}'), \
                mock.patch.object(setup.subprocess, 'run') as execute:
            with self.assertRaisesRegex(ValueError, 'incomplete'):
                setup.prepare_vectors(server, self.root)
            (local / 'model.json').write_text('custom manifest validated by native indexing')
            setup.prepare_vectors(server, self.root)
            execute.assert_not_called()

    def test_tampered_candidate_never_executes_or_changes_config(self):
        run, helper, config = self.candidate()
        helper.write_bytes(b'changed executable')
        with mock.patch.object(setup.subprocess, 'check_output') as execute:
            with self.assertRaisesRegex(ValueError, 'digest differs'):
                setup.setup(run, self.root, skip_index=True)
            execute.assert_not_called()
        self.assertEqual(config.read_bytes(), self.source())

    def test_failed_probe_preserves_config_and_invalidates_old_success(self):
        run, _, config = self.candidate()
        receipt = run / 'local-agent-setup.json'
        receipt.write_text('old success')
        with mock.patch.object(setup.subprocess, 'check_output', return_value='airk 1.1.4\n'), \
                mock.patch.object(setup, 'smoke', side_effect=ValueError('probe failed')):
            with self.assertRaisesRegex(ValueError, 'probe failed'):
                setup.setup(run, self.root, skip_index=True)
        self.assertEqual(config.read_bytes(), self.source())
        self.assertFalse(receipt.exists())

    def test_success_probes_proposed_and_saved_launch_before_receipt(self):
        run, helper, config = self.candidate()
        with mock.patch.object(setup.subprocess, 'check_output', return_value='airk 1.1.4\n'), \
                mock.patch.object(setup, 'smoke', return_value={'collection': 'fixture'}) as probe:
            setup.setup(run, self.root, skip_index=True)
        self.assertEqual(probe.call_count, 2)
        self.assertEqual(probe.call_args_list[0], probe.call_args_list[1])
        self.assertEqual(tomllib.loads(config.read_text())['mcp_servers']['airk-fixture']['command'], str(helper))
        self.assertTrue((run / 'local-agent-setup.json').exists())

    def test_indexed_setup_receipt_retains_native_performance_counters(self):
        run, _, _ = self.candidate()
        with mock.patch.object(setup.subprocess, 'check_output', return_value='airk 1.1.4\n'), \
                mock.patch.object(setup, 'prepare_vectors'), \
                mock.patch.object(setup, 'run_setup_step', return_value='{"embedding_workers":4,"embedding_inferences":12}'), \
                mock.patch.object(setup, 'smoke', return_value={'collection': 'fixture'}):
            receipt = setup.setup(run, self.root)
        self.assertTrue(receipt['indexed'])
        self.assertEqual(receipt['index_measurements']['embedding_workers'], 4)
        self.assertEqual(receipt['index_measurements']['embedding_inferences'], 12)
        self.assertGreaterEqual(receipt['index_measurements']['wall_ms'], 0)
        self.assertEqual(json.loads((run / 'local-agent-setup.json').read_bytes()), receipt)

    def test_edit_during_saved_probe_cannot_claim_success(self):
        run, _, config = self.candidate()
        calls = 0

        def probe(*args):
            nonlocal calls
            calls += 1
            if calls == 2:
                config.write_bytes(b'# changed during verification\n')
            return {'collection': 'fixture'}

        with mock.patch.object(setup.subprocess, 'check_output', return_value='airk 1.1.4\n'), \
                mock.patch.object(setup, 'smoke', side_effect=probe):
            with self.assertRaisesRegex(ValueError, 'changed during verification'):
                setup.setup(run, self.root, skip_index=True)
        self.assertEqual(config.read_bytes(), b'# changed during verification\n')
        self.assertFalse((run / 'local-agent-setup.json').exists())

    @unittest.skipUnless(os.environ.get('AIRK_LEGACY_TEST_BINARY') and os.environ.get('AIRK_LEGACY_TEST_MODEL_DIR'),
                         'Set AIRK_LEGACY_TEST_BINARY and AIRK_LEGACY_TEST_MODEL_DIR for historical standalone vector MCP smoke')
    def test_actual_model_default_and_explicit_vector_mcp_search(self):
        binary = str(Path(os.environ['AIRK_LEGACY_TEST_BINARY']).resolve())
        (self.root / 'README.md').write_text('# Account recovery\nReset a forgotten password with an email recovery link.\n', encoding='utf-8')
        (self.root / 'bread.txt').write_text('Bread dough combines flour and yeast before baking.\n', encoding='utf-8')
        for arguments in (['collections', 'vectors', 'install', str(Path(os.environ['AIRK_LEGACY_TEST_MODEL_DIR']).resolve())], ['index']):
            subprocess.run([binary, *self.args[:-1], *arguments], check=True, stdout=subprocess.DEVNULL,
                           **setup.process_options())
        version = subprocess.check_output([binary, '--version'], text=True, **setup.process_options()).strip().split()[1]
        result = setup.smoke({'command': binary, 'args': self.args}, version, self.root)
        self.assertEqual(result['vector_search'], 'passed')
        self.assertEqual(result['default_search'], 'passed')
        self.assertGreaterEqual(result['indexed_vectors'], 2)

    @unittest.skipUnless(os.environ.get('AIRK_LEGACY_TEST_BINARY'), 'Set AIRK_LEGACY_TEST_BINARY to a pre-1.4 standalone helper for historical MCP smoke')
    def test_actual_mcp_checks_index_revision_and_rejects_stale_source(self):
        binary = str(Path(os.environ['AIRK_LEGACY_TEST_BINARY']).resolve())
        (self.root / 'README.md').write_text('# Fixture\nUnique smoke evidence.\n', encoding='utf-8')
        env = {**os.environ, 'AIRK_USAGE_MODE': 'disabled'}
        subprocess.run([binary, *self.args[:-1], 'collections', 'vectors', 'set', '--enabled', 'false'],
                       env=env, check=True, stdout=subprocess.DEVNULL, **setup.process_options())
        subprocess.run([binary, *self.args[:-1], 'index'], env=env, check=True, stdout=subprocess.DEVNULL,
                       **setup.process_options())
        version = subprocess.check_output([binary, '--version'], text=True, **setup.process_options()).strip().split()[1]
        result = setup.smoke({'command': binary, 'args': self.args}, version, self.root)
        self.assertEqual(result['search'], 'passed')
        self.assertEqual(result['vector_search'], 'opted_out')
        subprocess.run([binary, *self.args[:-1], 'collections', 'vectors', 'set', '--enabled', 'true'],
                       env=env, check=True, stdout=subprocess.DEVNULL, **setup.process_options())
        with self.assertRaisesRegex(ValueError, 'not ready'):
            setup.smoke({'command': binary, 'args': self.args}, version, self.root)
        subprocess.run([binary, *self.args[:-1], 'collections', 'vectors', 'set', '--enabled', 'false'],
                       env=env, check=True, stdout=subprocess.DEVNULL, **setup.process_options())
        with self.assertRaisesRegex(ValueError, 'version differs'):
            setup.smoke({'command': binary, 'args': self.args}, '0.0.0', self.root)
        (self.root / 'README.md').write_text('# Changed\nDifferent live source.\n', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'does not match'):
            setup.smoke({'command': binary, 'args': self.args}, version, self.root)


class ModernLocalSetupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.config = self.root / '.codex' / 'config.toml'
        self.config.parent.mkdir()
        self.runtime = self.root / 'approved-runtime'
        self.runtime.mkdir()
        self.main_name = 'airk.exe' if os.name == 'nt' else ('AIRK.app/Contents/MacOS/airk' if sys.platform == 'darwin' else 'airk')
        binary = self.runtime / self.main_name
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_bytes(b'isolated mock executable; never run')
        self.execution = hashlib.sha256(binary.read_bytes()).hexdigest()
        operating_system = {'win32': 'windows', 'darwin': 'macos', 'linux': 'linux'}[sys.platform]
        architecture = {'amd64': 'x86_64', 'x86_64': 'x86_64', 'arm64': 'aarch64', 'aarch64': 'aarch64'}[platform.machine().lower()]
        pair = {'schema': 2, 'version': '1.8.14', 'os': operating_system, 'arch': architecture, 'airk_sha256': self.execution}
        (self.runtime / 'airk-runtime.json').write_text(json.dumps(pair))
        manifest = {'schema': 1, 'version': '1.8.14', 'os': operating_system, 'arch': architecture,
                    'files': {self.main_name: self.execution, 'airk-runtime.json': hashlib.sha256((self.runtime / 'airk-runtime.json').read_bytes()).hexdigest()}}
        raw = json.dumps(manifest).encode()
        (self.runtime / 'airk-install.json').write_bytes(raw)
        self.approved = hashlib.sha256(raw).hexdigest()
        self.install = self.root / 'install'
        installed = self.install / 'runtimes' / self.approved / self.main_name
        installed.parent.mkdir(parents=True)
        installed.write_bytes(binary.read_bytes())
        self.control = self.root / 'private-control'
        self.control.mkdir()
        self.storage = self.root / 'original-storage'
        self.storage.mkdir()
        self.identity = 'ab' * 16
        self.scope = {'kind': 'scoped', 'selection': {'namespace': 'native', 'expected_catalog_revision': 4, 'grant_id': 'unchanged', 'grant_revision': 1}}
        self.selection_file = self.root / 'selection.json'
        self.selection_file.write_text(json.dumps(self.scope))
        self.selection_file.chmod(0o600)
        self.inspection = {'payload_verified': True, 'approval': {'manifest_sha256': self.approved}, 'executable': str(installed),
                           'control_dir': str(self.control), 'registry_namespace': 'native'}
        self.info = {'schema': 1, 'connection_id': self.identity, 'selection': self.scope,
                     'endpoint': {'url': 'https://127.0.0.1:57422/mcp', 'namespace': 'native', 'executable_sha256': self.execution},
                     'ca_certificate_file': str(self.control / 'http-1' / 'ca.pem'),
                     'credential_file': str(self.control / 'http-1' / ('credential-'+self.identity+'.token'))}
        self.native_calls = []
        self.launcher_observation = {'schema':1,'active':self.approved,'launcher':None,'universal_launcher':None}

    def bundle(self):
        variable = 'AIRK_MCP_TOKEN_'+self.identity.upper()
        return {'connection': self.info, 'configuration': '[mcp_servers.airk]\nurl = '+json.dumps(self.info['endpoint']['url'])+'\nbearer_token_env_var = '+json.dumps(variable)+'\n[mcp_servers.airk.http_headers]\nX-AIRK-Response-Mode = "terse"\n',
                'environment': {'token_source_file': self.info['credential_file'], 'additional_ca_file': self.info['ca_certificate_file'],
                                'token_variable': variable, 'ca_variable': 'CODEX_CA_CERTIFICATE', 'preserve_existing_trust': True}}

    def native(self, binary, arguments):
        self.native_calls.append([str(value) for value in arguments])
        if arguments[:2] == ['runtime', 'inspect']:
            return self.inspection
        if arguments[:2] == ['runtime','inspect-legacy-launchers']:
            return self.launcher_observation
        if arguments[:2] == ['runtime','retire-legacy-launchers']:
            return {'schema':1,'active':self.approved,'legacy_launchers_retired':True,'launcher':None,'universal_launcher':None,
                    'processes':{'scope':'verified_launcher_images','remaining':0,'observations':[]}}
        if arguments[:2] == ['runtime','retire-previous']:
            return {'active':self.approved,'retired_previous':None,'retired_runtimes':[],'complete':True,
                    'selected_service_restarted':False,
                    'processes':{'scope':'verified_superseded_runtime_images','remaining':0,'observations':[]}}
        if 'setup' in arguments:
            self.assertIn('--existing-connection-id', arguments)
            self.assertNotIn('--connection-id', arguments)
            return self.bundle()
        return self.info

    def run_modern(self, **options):
        with mock.patch.object(setup, 'native_json', side_effect=self.native), \
                mock.patch.object(setup, 'https_smoke', return_value={'transport': 'https', 'status': 'passed', 'tool_catalog': [{'name': name} for name in ('status', 'search', 'read_file')]}) as smoke, \
                mock.patch.object(setup.subprocess, 'Popen', side_effect=AssertionError('Modern setup must not start a stdio frontend')):
            result = setup.setup_http(self.runtime, self.approved, self.install, self.identity, self.root, **options)
        self.assertEqual(smoke.call_count, 1)
        return result

    def test_modern_selected_grant_uses_native_enrollment_and_canonical_existing_renderer(self):
        self.config.write_text('# keep user comment\nmodel = "unchanged"\n[mcp_servers.other]\ncommand = "preserve-other"\n')
        result = self.run_modern(selection_file=self.selection_file)
        saved = tomllib.loads(self.config.read_text())
        self.assertEqual(saved['mcp_servers']['airk'], tomllib.loads(self.bundle()['configuration'])['mcp_servers']['airk'])
        self.assertEqual(saved['model'], 'unchanged')
        self.assertEqual(saved['mcp_servers']['other'], {'command': 'preserve-other'})
        self.assertIn('# keep user comment', self.config.read_text())
        self.assertEqual([call[1] for call in self.native_calls if call[0] == 'runtime'], ['inspect', 'http-connection','inspect-legacy-launchers'])
        self.assertEqual(result['connection']['selection'], self.scope)
        self.assertFalse(result['indexed'])
        self.assertFalse(result['client_environment_loaded'])
        self.assertNotIn('airk1.', json.dumps(result))

    def test_original_legacy_receipt_migration_preserves_storage_collection_and_tool_policy(self):
        launcher = self.install / ('airk-launcher.exe' if os.name == 'nt' else 'airk-launcher'); launcher.write_bytes(b'original owned launcher')
        self.launcher_observation['launcher'] = {'path':str(launcher),'sha256':hashlib.sha256(launcher.read_bytes()).hexdigest()}
        args = ['runtime','launch','--install-dir',str(self.install),'--','--root', str(self.root), '--data-dir', str(self.storage), '--collection', 'original','--service-project','cd'*32, 'mcp']
        before = ('# retain\n[mcp_servers.airk-old]\ncommand = '+json.dumps(str(launcher))+' # command comment\nargs = '+json.dumps(args)+'\nenabled = false\nenabled_tools = ["status", "search", "read_file"]\n[mcp_servers.airk-old.env]\nAIRK_CLIENT = "codex"\nAIRK_MCP_RESPONSE_MODE = "full" # response comment\n').encode()
        self.config.write_bytes(before)
        registration = {'binding': 'cd'*32, 'root': str(self.root), 'data_root': str(self.storage), 'collection': 'original'}
        file = self.root / 'registration.json'
        file.write_text(json.dumps(registration)); file.chmod(0o600)
        original = {'kind': 'imported_legacy', 'selection': {'namespace': 'native', 'registration': registration,
                    'grant': {'retained': True}, 'configuration_receipt': hashlib.sha256(before).hexdigest(), 'expected_catalog_revision': 5}}
        self.info['selection'] = original
        result = self.run_modern(registration_file=file, data_dir=self.storage,retire_legacy_launchers=True)
        migrated = tomllib.loads(self.config.read_text())['mcp_servers']['airk-old']
        self.assertFalse(migrated['enabled'])
        self.assertEqual(migrated['enabled_tools'], ['status', 'search', 'read_file'])
        self.assertEqual(migrated['http_headers']['X-AIRK-Response-Mode'], 'full')
        self.assertIn('# command comment', self.config.read_text()); self.assertIn('# response comment', self.config.read_text())
        call = next(call for call in self.native_calls if 'http-legacy-connection' in call)
        self.assertEqual(call[call.index('--configuration-receipt')+1], hashlib.sha256(before).hexdigest())
        self.assertEqual(result['connection']['selection'], original)
        self.assertTrue(result['legacy_launcher_cleanup']['complete'])
        self.assertEqual([call[1] for call in self.native_calls[-2:]],['retire-legacy-launchers','retire-previous'])
        self.assertTrue(result['superseded_runtime_cleanup']['complete'])

    def test_existing_http_idempotent_setup_does_not_reenroll_or_replace_authority(self):
        self.config.write_text(self.bundle()['configuration'])
        before = self.config.read_bytes()
        self.run_modern(selection_file=self.selection_file)
        self.assertEqual(self.config.read_bytes(), before)
        self.assertEqual([call[1] for call in self.native_calls if call[0] == 'runtime'], ['inspect', 'http-connection-info','inspect-legacy-launchers'])
        changed = copy.deepcopy(self.scope); changed['selection']['grant_id'] = 'broader'
        self.selection_file.write_text(json.dumps(changed))
        with self.assertRaisesRegex(ValueError, 'changes existing authority'):
            self.run_modern(selection_file=self.selection_file)
        self.assertEqual(self.config.read_bytes(), before)

    def owned_legacy_input(self, arguments, role='launcher'):
        directory = self.install if role == 'launcher' else self.root/'native-owned-universal'
        directory.mkdir(exist_ok=True)
        launcher = directory/('airk-launcher.exe' if os.name == 'nt' else 'airk-launcher')
        launcher.write_bytes(b'original exact native-owned legacy launcher')
        self.launcher_observation[role] = {'path':str(launcher),'sha256':hashlib.sha256(launcher.read_bytes()).hexdigest()}
        before = ('# preserve original scope\n[mcp_servers.airk]\ncommand = '+json.dumps(str(launcher))+'\nargs = '+json.dumps(arguments)+'\nenabled = false\n').encode()
        self.config.write_bytes(before)
        registration = {'binding':'cd'*32,'root':str(self.root),'data_root':str(self.storage),'collection':'original'}
        file = self.root/'original-registration.json'; file.write_text(json.dumps(registration)); file.chmod(0o600)
        self.info['selection'] = {'kind':'imported_legacy','selection':{'namespace':'native','registration':registration,
            'grant':{'retained':True},'configuration_receipt':hashlib.sha256(before).hexdigest(),'expected_catalog_revision':5}}
        return file,before,registration

    def test_split_explorer_declaration_blocks_all_setup_and_cleanup_without_native_scope_proof(self):
        file,before,_registration = self.owned_legacy_input(['--service-project','cd'*32,'mcp'])
        split = ('\n[mcp_servers.airk_explorer]\ncommand = '+json.dumps(str(self.install/'airk-launcher.exe'))+
                 '\nargs = ["explorer-assistance", "--descriptor", "unproven-private-descriptor"]\n').encode()
        for original in (before+split,split):
            self.config.write_bytes(original)
            with self.subTest(original=original), mock.patch.object(setup,'native_json') as native:
                with self.assertRaisesRegex(ValueError,'split Explorer.*cleanup blocked'):
                    setup.setup_http(self.runtime,self.approved,self.install,self.identity,self.root,
                                     registration_file=file,retire_legacy_launchers=True)
                native.assert_not_called()
            self.assertEqual(self.config.read_bytes(),original)

    def test_approved_pinned_predecessor_requires_exact_native_image_lineage_and_original_scope(self):
        file,before,registration = self.owned_legacy_input(['--service-project','cd'*32,'mcp'])
        launcher = Path(self.launcher_observation['launcher']['path'])
        pinned = self.install/'runtimes'/('e'*64)/'airk.exe'
        pinned.parent.mkdir(parents=True); pinned.write_bytes(b'genuine-shaped fixture bytes; never execute')
        self.launcher_observation['legacy_runtime_images'] = [{'path':str(pinned),
            'sha256':hashlib.sha256(pinned.read_bytes()).hexdigest(),'runtime_sha256':'e'*64}]
        before = before.replace(json.dumps(str(launcher)).encode(),json.dumps(str(pinned)).encode())
        self.config.write_bytes(before)
        self.info['selection']['selection']['configuration_receipt'] = hashlib.sha256(before).hexdigest()
        with mock.patch.object(setup,'native_json',side_effect=self.native), \
                mock.patch.object(setup,'https_smoke',return_value={'tool_catalog':[]}):
            result = setup.setup_http(self.runtime,self.approved,self.install,self.identity,self.root,
                                      registration_file=file,retire_legacy_launchers=True)
        self.assertEqual(result['connection']['selection']['selection']['registration'],registration)
        self.assertEqual(result['legacy_launcher_lineage'],self.launcher_observation)
        self.assertTrue(result['superseded_runtime_cleanup']['complete'])
        self.config.write_bytes(before); pinned.write_bytes(b'changed after approval')
        with mock.patch.object(setup,'native_json',side_effect=self.native):
            with self.assertRaisesRegex(ValueError,'changed after native inspection'):
                setup.setup_http(self.runtime,self.approved,self.install,self.identity,self.root,registration_file=file,
                                 retire_legacy_launchers=True)
        self.assertEqual(self.config.read_bytes(),before)

    def test_unique_root_only_universal_preserves_scope_and_reports_cleanup_pending(self):
        file,before,registration = self.owned_legacy_input(['--root',str(self.root),'mcp'],'universal_launcher')
        native = self.native
        def observe(binary,arguments):
            if arguments[:2] == ['runtime','source-scope']:
                self.native_calls.append(list(map(str,arguments)))
                return {'install_dir':str(self.install),'control_dir':str(self.control),'runtime_sha256':self.approved,
                    'scope':{'schema':1,'namespace':'native','root':str(self.root),'collection':None,'outcome':'registered','registration':registration}}
            return native(binary,arguments)
        with mock.patch.object(self,'native',side_effect=observe):
            result = self.run_modern(registration_file=file)
        self.assertEqual(result['connection']['selection']['selection']['registration'],registration)
        self.assertFalse(result['legacy_launcher_cleanup']['complete'])
        self.assertEqual(result['status'],'configuration_saved_cleanup_pending')
        self.assertFalse(any('retire-legacy-launchers' in call for call in self.native_calls))
        self.assertNotIn('command',tomllib.loads(self.config.read_text())['mcp_servers']['airk'])

    def test_ambiguous_root_union_is_not_narrowed_and_never_enrolls_or_retires(self):
        file,before,_registration = self.owned_legacy_input(['--root',str(self.root),'mcp'],'universal_launcher')
        native = self.native
        def observe(binary,arguments):
            if arguments[:2] == ['runtime','source-scope']:
                self.native_calls.append(list(map(str,arguments)))
                return {'install_dir':str(self.install),'control_dir':str(self.control),'runtime_sha256':self.approved,
                    'scope':{'schema':1,'namespace':'native','root':str(self.root),'collection':None,'outcome':'ambiguous','registration':None}}
            return native(binary,arguments)
        with mock.patch.object(self,'native',side_effect=observe):
            with self.assertRaisesRegex(ValueError,'ambiguous_root_scope_requires_original_union'):
                self.run_modern(registration_file=file,retire_legacy_launchers=True)
        self.assertEqual(self.config.read_bytes(),before)
        self.assertFalse(any('http-legacy-connection' in call or 'retire-legacy-launchers' in call for call in self.native_calls))

    def test_original_combined_session_migrates_whole_selection_and_refuses_subset(self):
        original = {'namespace':'native','expected_catalog_revision':4,'project_ids':['original-a','original-b'],
                    'index_ids':['index-a','index-b'],'grant_id':'frozen-union'}
        _file,before,_registration = self.owned_legacy_input(['--combined-session',json.dumps(original),'mcp'])
        self.scope = {'kind':'combined','selection':original}; self.info['selection'] = copy.deepcopy(self.scope)
        self.selection_file.write_text(json.dumps(self.scope))
        result = self.run_modern(selection_file=self.selection_file,retire_legacy_launchers=True)
        self.assertEqual(result['connection']['selection'],self.scope)
        self.assertTrue(any('http-connection' in call for call in self.native_calls))
        self.assertFalse(any('http-legacy-connection' in call for call in self.native_calls))
        self.config.write_bytes(before); self.native_calls.clear()
        changed = copy.deepcopy(self.scope); changed['selection']['project_ids'] = ['original-a']
        self.selection_file.write_text(json.dumps(changed))
        with self.assertRaisesRegex(ValueError,'multi-index authority differs'):
            self.run_modern(selection_file=self.selection_file,retire_legacy_launchers=True)
        self.assertEqual(self.config.read_bytes(),before)
        self.assertFalse(any('http-connection' in call or 'retire-legacy-launchers' in call for call in self.native_calls))

    def test_native_owned_image_hash_and_installation_must_match_before_import(self):
        args = ['runtime','launch','--install-dir',str(self.install),'--','--service-project','cd'*32,'mcp']
        file,before,_registration = self.owned_legacy_input(args)
        self.launcher_observation['launcher']['sha256'] = 'ff'*32
        with self.assertRaisesRegex(ValueError,'launcher changed'):
            self.run_modern(registration_file=file)
        self.assertEqual(self.config.read_bytes(),before)
        self.assertFalse(any('http-legacy-connection' in call for call in self.native_calls))
        self.launcher_observation['launcher'] = None
        with self.assertRaisesRegex(ValueError,'not a native-owned'):
            self.run_modern(registration_file=file)
        self.assertEqual(self.config.read_bytes(),before)

    def test_matching_basename_and_hash_do_not_adopt_a_foreign_launcher(self):
        file,_before,_registration = self.owned_legacy_input(['--service-project','cd'*32,'mcp'])
        original = Path(self.launcher_observation['launcher']['path'])
        foreign = self.root/'foreign'/original.name; foreign.parent.mkdir(); foreign.write_bytes(original.read_bytes())
        configuration = self.config.read_text().replace(json.dumps(str(original)),json.dumps(str(foreign)))
        self.config.write_text(configuration)
        with self.assertRaisesRegex(ValueError,'not a native-owned'):
            self.run_modern(registration_file=file,retire_legacy_launchers=True)
        self.assertEqual(self.config.read_text(),configuration)
        self.assertFalse(any('http-legacy-connection' in call or 'retire-legacy-launchers' in call for call in self.native_calls))

    def test_cli_cleanup_pending_is_not_a_successful_setup_exit(self):
        import contextlib
        import io
        receipt = {'status':'configuration_saved_cleanup_pending','legacy_launcher_cleanup':{'complete':False}}
        output = io.StringIO()
        with mock.patch.object(setup,'setup_http',return_value=receipt), mock.patch('sys.argv',[
            'setup-local-test.py','--runtime-dir',str(self.runtime),'--approved-sha256',self.approved,
            '--install-dir',str(self.install),'--connection-id',self.identity]), \
                contextlib.redirect_stdout(output),contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as result: setup.main()
        self.assertEqual(result.exception.code,1)
        self.assertEqual(json.loads(output.getvalue()),receipt)

    def test_cleanup_is_after_saved_cas_and_blocked_outcome_keeps_full_receipt(self):
        after = self.bundle()['configuration'].encode(); self.config.write_bytes(after)
        calls = []
        invalid = {'schema':1,'active':self.approved,'legacy_launchers_retired':False,'native_detail':{'retained':True}}
        def native(binary,arguments):
            calls.append(arguments)
            self.assertEqual(self.config.read_bytes(),after,'No cleanup before the verified save')
            return self.launcher_observation if 'inspect-legacy-launchers' in arguments else invalid
        with mock.patch.object(setup,'native_json',side_effect=native):
            result = setup.retire_after_config(self.runtime/self.main_name,self.install,self.approved,self.config,after,True)
        self.assertFalse(result['complete']); self.assertEqual(result['receipt'],invalid)
        self.assertEqual(sum('retire-legacy-launchers' in call for call in calls),1)
        self.config.write_bytes(b'# foreign edit')
        with mock.patch.object(setup,'native_json') as native:
            with self.assertRaisesRegex(ValueError,'changed before'):
                setup.retire_after_config(self.runtime/self.main_name,self.install,self.approved,self.config,after,True)
            native.assert_not_called()
    def test_superseded_cleanup_requires_launcher_completion_and_explicit_request(self):
        after = self.bundle()['configuration'].encode(); self.config.write_bytes(after)
        with mock.patch.object(setup,'native_json') as native:
            blocked = setup.retire_superseded_after_config(self.runtime/self.main_name,self.install,self.approved,self.config,after,{'complete':False},True)
            pending = setup.retire_superseded_after_config(self.runtime/self.main_name,self.install,self.approved,self.config,after,{'complete':True},False)
            native.assert_not_called()
        self.assertEqual(blocked['status'],'blocked_by_launcher_cleanup')
        self.assertEqual(pending['status'],'pending_review')
        self.assertFalse(blocked['complete']); self.assertFalse(pending['complete'])
        self.assertEqual(self.config.read_bytes(),after)

    def test_modern_to_modern_cleanup_retains_complete_native_roster_and_selected_runtime(self):
        after = self.bundle()['configuration'].encode(); self.config.write_bytes(after)
        old = 'd'*64
        receipt = {'active':self.approved,'retired_previous':old,'retired_runtimes':[old,'e'*64],
                   'complete':True,'selected_service_restarted':False,
                   'processes':{'scope':'verified_superseded_runtime_images','remaining':0,'observations':[{'full_native_proof':True}]}}
        with mock.patch.object(setup,'native_json',return_value=receipt) as native:
            result = setup.retire_superseded_after_config(self.runtime/self.main_name,self.install,self.approved,self.config,after,{'complete':True},True)
        self.assertTrue(result['complete']); self.assertEqual(result['receipt'],receipt)
        native.assert_called_once_with(self.runtime/self.main_name,['runtime','retire-previous','--install-dir',self.install,'--expected-active',self.approved])
        self.assertEqual(self.config.read_bytes(),after)
        self.assertTrue((self.runtime/self.main_name).is_file())

    def test_superseded_cleanup_rejects_partial_crossed_and_selected_runtime_receipts_without_retry(self):
        after = self.bundle()['configuration'].encode(); self.config.write_bytes(after)
        valid = {'active':self.approved,'retired_previous':None,'retired_runtimes':[],'complete':True,
                 'selected_service_restarted':False,
                 'processes':{'scope':'verified_superseded_runtime_images','remaining':0,'observations':[]}}
        for delta in ({'active':'f'*64},{'complete':False},{'retired_runtimes':[self.approved]},
                      {'retired_runtimes':['d'*64,'d'*64]},{'retired_runtimes':[{}]},
                      {'retired_previous':'e'*64},{'retired_runtimes':None},
                      {'selected_service_restarted':True},{'processes':{'remaining':0}},
                      {'processes':valid['processes']|{'remaining':True}},
                      {'processes':valid['processes']|{'observations':[{}]*21}}):
            with self.subTest(delta=delta), mock.patch.object(setup,'native_json',return_value=valid|delta) as native:
                result = setup.retire_superseded_after_config(self.runtime/self.main_name,self.install,self.approved,self.config,after,{'complete':True},True)
                self.assertFalse(result['complete']); self.assertEqual(result['receipt'],valid|delta)
                native.assert_called_once()
        with mock.patch.object(setup,'native_json',side_effect=RuntimeError('unknown process exit')) as native:
            result = setup.retire_superseded_after_config(self.runtime/self.main_name,self.install,self.approved,self.config,after,{'complete':True},True)
            self.assertFalse(result['complete']); native.assert_called_once()
        self.config.write_bytes(b'# concurrent edit')
        with mock.patch.object(setup,'native_json') as native:
            result = setup.retire_superseded_after_config(self.runtime/self.main_name,self.install,self.approved,self.config,after,{'complete':True},True)
            self.assertFalse(result['complete']); native.assert_not_called()

    def test_wrong_storage_legacy_review_and_failed_https_never_rewrite_configuration(self):
        self.config.write_text(self.bundle()['configuration'])
        before = self.config.read_bytes()
        with mock.patch.object(setup, 'native_json', side_effect=self.native), \
                mock.patch.object(setup, 'https_smoke', side_effect=ValueError('HTTPS verification refused')):
            with self.assertRaisesRegex(ValueError, 'HTTPS verification refused'):
                setup.setup_http(self.runtime, self.approved, self.install, self.identity, self.root)
        self.assertEqual(self.config.read_bytes(), before)
        with self.assertRaisesRegex(ValueError, 'replacement data directory'):
            self.run_modern(data_dir=self.root / 'other-storage')
        self.assertEqual(self.config.read_bytes(), before)

    def test_user_level_configuration_and_unapproved_payload_never_execute_or_write(self):
        self.config.write_text('model = "keep"\n')
        before = self.config.read_bytes()
        with mock.patch.object(setup.Path, 'home', return_value=self.root), mock.patch.object(setup, 'native_json') as native:
            with self.assertRaisesRegex(ValueError, 'User-level'):
                setup.setup_http(self.runtime, self.approved, self.install, self.identity, self.root)
            native.assert_not_called()
        (self.runtime / self.main_name).write_bytes(b'tampered')
        with mock.patch.object(setup, 'native_json') as native:
            with self.assertRaisesRegex(ValueError, 'seal differs'):
                setup.setup_http(self.runtime, self.approved, self.install, self.identity, self.root, selection_file=self.selection_file)
            native.assert_not_called()
        self.assertEqual(self.config.read_bytes(), before)

    def test_custom_environment_and_tool_refusal_are_preserved(self):
        canonical = self.bundle()['configuration']
        for extra in ['[mcp_servers.airk.env]\nCUSTOM = "preserve"\n', 'disabled_tools = ["search"]\n']:
            source = canonical if extra.startswith('[') else canonical.split('[mcp_servers.airk.http_headers]')[0]
            before = (source+extra).encode()
            with self.assertRaises(ValueError):
                setup.prepare_http_config(before, self.root, canonical)

    def test_known_tool_aliases_migrate_one_to_one_without_changing_policy_meaning(self):
        canonical = self.bundle()['configuration']
        source = canonical.split('[mcp_servers.airk.http_headers]')[0]
        source += 'enabled_tools = ["airk_status", "airk_search", "airk_read_file"] # preserve policy\n'
        after, _, server = setup.prepare_http_config(source.encode(), self.root, canonical,
                                                     known_tools={'status', 'search', 'read_file'})
        self.assertEqual(server['enabled_tools'], ['status', 'search', 'read_file'])
        self.assertIn('# preserve policy', after.decode())
        source = source.replace('airk_read_file', 'airk_unknown_custom')
        with self.assertRaisesRegex(ValueError, 'Unknown former tool alias'):
            setup.prepare_http_config(source.encode(), self.root, canonical,
                                      known_tools={'status', 'search', 'read_file'})


@unittest.skipUnless(TLS.OPENSSL, 'OpenSSL required for isolated actual TLS setup smoke')
class ModernActualTlsSmokeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        TLS.ActualHttpsFixtureTests.setUpClass()

    @classmethod
    def tearDownClass(cls):
        TLS.ActualHttpsFixtureTests.tearDownClass()

    def test_real_https_smoke_checks_version_revision_source_hash_and_closes_only_its_session(self):
        fixture = TLS.ActualHttpsFixtureTests()
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
                    if self.headers.get('Authorization') != 'Bearer '+(fixture.root / 'credential.token').read_text():
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
            context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            context.load_cert_chain(fixture.root / 'leaf.pem', fixture.root / 'leaf.key')
            server.socket = context.wrap_socket(server.socket, server_side=True)
            thread = threading.Thread(target=server.serve_forever, kwargs={'poll_interval': .02}, daemon=True)
            thread.start()
            def cert_hash(name):
                return hashlib.sha256(ssl.PEM_cert_to_DER_cert((fixture.root / name).read_text())).hexdigest()
            info = {'schema': 1, 'connection_id': fixture.identity, 'selection': {'kind': 'scoped', 'selection': {'namespace': 'fixture'}},
                    'endpoint': {'schema': 1, 'namespace': 'fixture', 'state': 'ready', 'url': f'https://127.0.0.1:{server.server_port}/mcp',
                                 'ca_sha256': cert_hash('ca.pem'), 'certificate_sha256': cert_hash('leaf.pem')},
                    'ca_certificate_file': str(fixture.root / 'ca.pem'), 'credential_file': str(fixture.root / 'credential.token')}
            try:
                with mock.patch.object(setup.subprocess, 'Popen', side_effect=AssertionError('No stdio/native process in generic TLS smoke')):
                    result = setup.https_smoke(info, fixture.root, '1.8.14', root, 'original')
                self.assertEqual(result['source_sha256'], source_hash)
                self.assertEqual(result['codex_client_execution'], 'not_run')
                self.assertEqual(result['tool_count'], 3)
                read = next(value for value in calls if value.get('params', {}).get('name') == 'read_file')
                self.assertEqual(read['params']['arguments']['revision'], 'exact-saved-revision')
                self.assertEqual(read['params']['arguments']['verification'], 'require')
                self.assertEqual(calls[-1]['method'], 'DELETE')
                self.assertEqual((root / 'README.md').read_bytes(), content)
            finally:
                server.shutdown(); server.server_close(); thread.join(timeout=5)
                self.assertFalse(thread.is_alive())


if __name__ == '__main__':
    unittest.main()
