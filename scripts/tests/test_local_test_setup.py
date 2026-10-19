# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest import mock

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
            self.assertEqual(execute.call_count, 2)
            self.assertIn('--download', execute.call_args_list[0].args[0])
            self.assertEqual(execute.call_args_list[1].args[0][-4:-1], ['collections', 'vectors', 'install'])
            execute.reset_mock()
            execute.side_effect = subprocess.CalledProcessError(1, 'provision')
            with self.assertRaisesRegex(RuntimeError, 'Model download and validation failed'):
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

    @unittest.skipUnless(os.environ.get('AIRK_TEST_BINARY'), 'Set AIRK_TEST_BINARY for native writer-lock retry')
    def test_actual_native_writer_lock_is_retried_after_owner_releases_it(self):
        binary = str(Path(os.environ['AIRK_TEST_BINARY']).resolve())
        (self.root / 'README.md').write_text('Original lock contention fixture.\n', encoding='utf-8')
        command = [binary, *self.args[:-1]]
        subprocess.run([*command, 'collections', 'vectors', 'set', '--enabled', 'false'],
                       check=True, capture_output=True, **setup.process_options())
        with (self.storage / 'collections' / '.writer-locks' / 'fixture').open('r+b') as lock:
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

    @unittest.skipUnless(os.environ.get('AIRK_TEST_BINARY') and os.environ.get('AIRK_TEST_MODEL_DIR'),
                         'Set AIRK_TEST_BINARY and AIRK_TEST_MODEL_DIR for real vector MCP smoke')
    def test_actual_model_default_and_explicit_vector_mcp_search(self):
        binary = str(Path(os.environ['AIRK_TEST_BINARY']).resolve())
        (self.root / 'README.md').write_text('# Account recovery\nReset a forgotten password with an email recovery link.\n', encoding='utf-8')
        (self.root / 'bread.txt').write_text('Bread dough combines flour and yeast before baking.\n', encoding='utf-8')
        for arguments in (['collections', 'vectors', 'install', str(Path(os.environ['AIRK_TEST_MODEL_DIR']).resolve())], ['index']):
            subprocess.run([binary, *self.args[:-1], *arguments], check=True, stdout=subprocess.DEVNULL,
                           **setup.process_options())
        version = subprocess.check_output([binary, '--version'], text=True, **setup.process_options()).strip().split()[1]
        result = setup.smoke({'command': binary, 'args': self.args}, version, self.root)
        self.assertEqual(result['vector_search'], 'passed')
        self.assertEqual(result['default_search'], 'passed')
        self.assertGreaterEqual(result['indexed_vectors'], 2)

    @unittest.skipUnless(os.environ.get('AIRK_TEST_BINARY'), 'Set AIRK_TEST_BINARY for real native MCP smoke')
    def test_actual_mcp_checks_index_revision_and_rejects_stale_source(self):
        binary = str(Path(os.environ['AIRK_TEST_BINARY']).resolve())
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


if __name__ == '__main__':
    unittest.main()
