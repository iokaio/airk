# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

RUNNER = Path(__file__).with_name('benchmark-real-corpora.py')
if not RUNNER.is_file():
    RUNNER = Path(__file__).resolve().parents[1] / 'benchmark-real-corpora.py'
SPEC = importlib.util.spec_from_file_location('real_corpora', RUNNER)
BENCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCH)


def envelope(value, failed=False):
    return {'isError': failed, 'content': [{'type': 'text', 'text': json.dumps(value)}]}


class CorpusBenchmarkTests(unittest.TestCase):
    def directories(self, temporary):
        root = Path(temporary).resolve()
        a, b = root / 'a', root / 'b'
        a.mkdir(); b.mkdir()
        return root, a, b

    def test_isolation_rejects_source_nested_state_and_state_ancestor(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner, a, b = self.directories(temporary)
            for target in (a, a / 'derived', owner):
                with self.assertRaisesRegex(BENCH.CampaignError, 'state_source_overlap'):
                    BENCH.isolate([a, b], [target])
            self.assertEqual(BENCH.isolate([a, b], [owner / 'derived']), [a, b])

    def test_reparse_attribute_is_rejected_without_symlink_privilege(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner, a, b = self.directories(temporary)
            target = owner / 'state'
            target.mkdir()
            original = Path.lstat
            def lstat(path, *args, **kwargs):
                info = original(path, *args, **kwargs)
                return SimpleNamespace(st_file_attributes=0x400, st_mode=info.st_mode) if path == target else info
            with patch.object(Path, 'lstat', lstat):
                with self.assertRaisesRegex(BENCH.CampaignError, 'redirected_path'):
                    BENCH.isolate([a, b], [target])

    def test_overlapping_corpora_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner, a, b = self.directories(temporary)
            nested = a / 'nested'; nested.mkdir()
            with self.assertRaisesRegex(BENCH.CampaignError, 'overlapping_corpora'):
                BENCH.isolate([a, nested], [b])

    def test_symlinked_state_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner, a, b = self.directories(temporary)
            linked = owner / 'linked'
            try:
                linked.symlink_to(a, target_is_directory=True)
            except OSError:
                self.skipTest('symlink creation unavailable')
            with self.assertRaisesRegex(BENCH.CampaignError, 'redirected_path'):
                BENCH.isolate([a, b], [linked / 'state'])

    def test_identity_detects_tracked_untracked_and_deleted_changes_without_writes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            def git(*args):
                subprocess.run(['git', '-c', f'safe.directory={root.as_posix()}', '-C', str(root), *args], check=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            git('init')
            git('config', 'user.name', 'Local Fixture')
            git('config', 'user.email', 'fixture@example.invalid')
            path = root / 'fixture.txt'; path.write_text('original')
            git('add', 'fixture.txt'); git('commit', '-m', 'Original fixture')
            before = BENCH.source_identity(root)
            self.assertEqual(BENCH.source_identity(root), before)
            path.write_text('changed')
            with self.assertRaisesRegex(BENCH.CampaignError, 'source_changed'):
                BENCH.unchanged(root, before)
            path.write_text('original')
            extra = root / 'new.txt'; extra.write_text('untracked')
            self.assertNotEqual(BENCH.source_identity(root)['identity_sha256'], before['identity_sha256'])
            extra.unlink(); path.unlink()
            self.assertNotEqual(BENCH.source_identity(root)['identity_sha256'], before['identity_sha256'])

    def test_identity_budgets_fail_instead_of_truncating(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(BENCH, 'git', side_effect=[b'a' * 40, b'', b'one\0two\0']):
                with self.assertRaisesRegex(BENCH.CampaignError, 'source_identity_file_budget'):
                    BENCH.source_identity(root, max_files=1)

    def test_private_error_text_paths_queries_and_hits_are_redacted(self):
        secret = 'PRIVATE_SOURCE_PATH_AND_QUERY'
        value = {'index_state': secret, 'files': True, 'chunks': -1,
                 'error': {'code': secret, 'message': secret}, 'hits': [{'path': secret, 'text': secret}],
                 'semantic_readiness': {'state': secret, 'selected_artifacts': secret}}
        observed = BENCH.safe_observation(envelope(value, True))
        self.assertNotIn(secret, json.dumps(observed))
        self.assertEqual(observed['hits'], 1)
        self.assertEqual(observed['code'], 'other')
        self.assertIsNone(observed['files']); self.assertIsNone(observed['chunks'])
        self.assertIsNone(observed['selected_artifacts'])

    def test_first_status_failure_is_preserved_without_retry_or_diagnostics(self):
        calls = []
        class Session:
            initialize_ms = 12.5
            def __init__(self, *_, **kwargs): pass
            def __enter__(self): return self
            def __exit__(self, *_): pass
            def tool(self, name, arguments=None):
                calls.append(name)
                value = {'index_state': 'unknown', 'index_error': {'code': 'status_timeout'}} if len(calls) == 1 else {'index_state': 'available'}
                return envelope(value), {'elapsed_ms': 2, **BENCH.safe_observation(envelope(value))}
        result = BENCH.sample(SimpleNamespace(command_timeout=5), 'fixture', [('list_files', {})], Session)
        self.assertEqual(calls, ['status', 'list_files', 'status'])
        self.assertEqual(result['first_status']['code'], 'status_timeout')
        self.assertEqual(result['first_status']['index_state'], 'unknown')

    def test_stdio_session_uses_real_framing_and_closes_owned_python_child(self):
        import sys
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            script = root / 'fake_mcp.py'
            script.write_text("""import json, sys
for line in sys.stdin:
    message = json.loads(line)
    if 'id' not in message:
        continue
    if message['method'] == 'initialize':
        result = {'serverInfo': {'name': 'airk'}}
    else:
        assert message['method'] == 'tools/call'
        assert message['params']['name'] == 'status'
        result = {'content': [{'type': 'text', 'text': json.dumps({'index_state': 'unknown', 'project': 'binding', 'index_error': {'code': 'status_timeout', 'message': 'PRIVATE_ERROR'}})}]}
    print(json.dumps({'jsonrpc': '2.0', 'id': message['id'], 'result': result}), flush=True)
""")
            class Fixture:
                binary = Path(sys.executable)
                runtime = root
                environment = os.environ.copy()
                command_timeout = 5
                projects = {'project': {'collection': 'collection', 'binding': 'binding'}}
                def arguments(self, _): return [str(script)]
            with BENCH.Session(Fixture(), 'project') as session:
                _, observed = session.tool('status')
                self.assertEqual(observed['code'], 'status_timeout')
                self.assertNotIn('PRIVATE_ERROR', json.dumps(observed))
                child = session.child
            self.assertEqual(child.returncode, 0)

    def test_job_poll_uses_original_id_and_rejects_crossed_project(self):
        receipt = {'job': {'id': 'original', 'project': 'project'}}
        class Fixture:
            def rpc(self, method, deadline):
                self.method, self.deadline = method, deadline
                return {'job': {'id': 'original', 'project': 'other', 'state': 'succeeded'}}
        fixture = Fixture()
        with self.assertRaisesRegex(BENCH.CampaignError, 'crossed_job_receipt'):
            BENCH.terminal_job(fixture, receipt, 123)
        self.assertEqual(fixture.method, {'job': {'id': 'original'}})
        self.assertEqual(fixture.deadline, 123)

    def test_cancellation_does_not_recreate_request_when_completion_wins(self):
        calls = []
        receipt = {'job': {'id': 'original', 'project': 'binding'}}
        class Fixture:
            job_timeout = 30
            projects = {'project': {'binding': 'binding'}}
            def rpc(self, method, **kwargs):
                calls.append((method, kwargs['deadline']))
                if method == 'request_epoch': return {'namespace': 'namespace', 'epoch': 1}
                if 'enqueue' in method or 'cancel' in method: return receipt
                return {'job': {**receipt['job'], 'state': 'succeeded'}}
        result = BENCH.cancellation_restart_campaign(Fixture(), 'project')
        self.assertEqual(result, {'state': 'succeeded', 'cancel_won': False})
        self.assertEqual(len(calls), 4)
        self.assertEqual(calls[0][0], 'request_epoch')
        self.assertEqual(calls[1][0]['enqueue']['kind'], 'reconcile')
        self.assertEqual(calls[2][0], {'cancel': {'id': 'original'}})
        self.assertEqual(calls[3][0], {'job': {'id': 'original'}})
        self.assertEqual(len(set(deadline for _, deadline in calls)), 1)

    def test_percentiles_include_first_response_failures(self):
        rows = [{'first_status': {'elapsed_ms': ms, 'is_error': False, 'index_state': state}}
                for ms, state in [(2, 'available'), (2000, 'unknown'), (4, 'available')]]
        report = BENCH.timings(rows)
        self.assertEqual(report, {'samples': 3, 'first_status_p50_ms': 4, 'first_status_p95_ms': 2000, 'first_status_failures': 1})

    def args(self, owner, a, b, report):
        runtime = owner / 'runtime'
        runtime.mkdir(exist_ok=True)
        return SimpleNamespace(airk=a, bookbuilder=b, report=report, runtime_dir=runtime,
                               state_parent=owner / 'state', samples=1, command_timeout=5, job_timeout=10)

    def test_rejected_output_creates_no_source_directory_even_on_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner, a, b = self.directories(temporary)
            target = a / 'new' / 'report.json'
            args = self.args(owner, a, b, target)
            with patch.object(BENCH, 'run') as run:
                report, written = BENCH.execute(args)
            self.assertFalse(written)
            self.assertEqual(report['code'], 'state_source_overlap')
            self.assertFalse((a / 'new').exists())
            run.assert_not_called()

    def test_report_cannot_overlap_runtime_or_replace_existing_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner, a, b = self.directories(temporary)
            args = self.args(owner, a, b, owner / 'runtime' / 'new' / 'report.json')
            report, written = BENCH.execute(args)
            self.assertFalse(written)
            self.assertEqual(report['code'], 'report_runtime_overlap')
            self.assertFalse((args.runtime_dir / 'new').exists())
            args.report = owner / 'unowned.json'
            args.report.write_text('preserve user bytes')
            report, written = BENCH.execute(args)
            self.assertFalse(written)
            self.assertEqual(report['code'], 'report_already_exists')
            self.assertEqual(args.report.read_text(), 'preserve user bytes')

    def test_partial_campaign_survives_later_failure_and_errors_are_redacted(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner, a, b = self.directories(temporary)
            args = self.args(owner, a, b, owner / 'reports' / 'result.json')
            def run(_args, output):
                output['corpora'] = [{'alias': 'airk', 'sessions': [{'first_status': {'elapsed_ms': 123}}]}]
                raise OSError('PRIVATE_ERROR_SOURCE_PATH')
            with patch.object(BENCH, 'run', side_effect=run):
                report, written = BENCH.execute(args)
            self.assertTrue(written)
            self.assertFalse(report['completed'])
            self.assertEqual(report['code'], 'transport_io_failed')
            self.assertEqual(report['corpora'][0]['sessions'][0]['first_status']['elapsed_ms'], 123)
            self.assertNotIn('PRIVATE_ERROR_SOURCE_PATH', args.report.read_text())

    def test_native_paths_inventory_is_counted_after_scope_validation(self):
        expected = {'collection': 'fixture', 'binding': 'binding'}
        publication = {'collection': 'fixture', 'token': 'airk-r1:' + 'a' * 32 + ':1:' + 'b' * 64}
        value = {'collection': 'fixture', 'publication': publication, 'paths': ['private/a', 'private/b']}
        observed = BENCH.safe_observation(envelope(value), expected, 'list_files')
        self.assertEqual(observed['items'], 2)
        self.assertNotIn('private', json.dumps(observed))
        for changed in ({**value, 'collection': 'other'},
                        {**value, 'publication': {**publication, 'collection': 'other'}},
                        {**value, 'publication': {**publication, 'token': 'invalid'}}):
            with self.assertRaises(BENCH.CampaignError):
                BENCH.safe_observation(envelope(changed), expected, 'list_files')

    def test_status_accepts_scalar_publication_but_rejects_wrong_project(self):
        expected = {'collection': 'fixture', 'binding': 'binding'}
        value = {'index_state': 'available', 'collection': 'fixture', 'project': 'binding',
                 'publication': {'token': 'airk-r1:' + 'a' * 32 + ':1:' + 'b' * 64}}
        BENCH.safe_observation(envelope(value), expected, 'status')
        with self.assertRaisesRegex(BENCH.CampaignError, 'crossed_response_scope'):
            BENCH.safe_observation(envelope({**value, 'project': 'other'}), expected, 'status')
        for key in ('project', 'collection', 'publication'):
            removed = dict(value); removed.pop(key)
            with self.assertRaises(BENCH.CampaignError):
                BENCH.safe_observation(envelope(removed), expected, 'status')

    def test_actual_semantic_policies_survive_diagnostic_redaction(self):
        for policy in ('partial_hybrid', 'lexical_while_pending', 'require_complete'):
            class Session:
                def __init__(self, *_, **kwargs): pass
                def __enter__(self): return self
                def __exit__(self, *_): pass
                def tool(self, name):
                    return envelope({'semantic_readiness': {'default_search_policy': policy}}), {}
            with patch.object(BENCH, 'Session', Session):
                observed = BENCH.diagnostic_identity(SimpleNamespace(command_timeout=5), 'fixture')
            self.assertEqual(observed['default_search_policy'], policy)

    def test_first_transport_failure_is_timed_and_never_replayed(self):
        calls = []
        class Session:
            initialize_ms = 12
            def __init__(self, *_, **kwargs): pass
            def __enter__(self): return self
            def __exit__(self, *_): pass
            def tool(self, name, arguments=None):
                calls.append(name)
                raise BENCH.ObservationFailure('mcp_response_timeout', {'elapsed_ms': 25, 'is_error': True,
                    'index_state': 'unknown', 'code': 'mcp_response_timeout', 'transport_or_protocol_failure': True})
        output = {}
        result = BENCH.sample(SimpleNamespace(command_timeout=5), 'fixture', [('search', {})], Session, output=output)
        self.assertIs(result, output)
        self.assertEqual(calls, ['status'])
        self.assertTrue(result['aborted'])
        self.assertEqual(result['first_status']['elapsed_ms'], 25)
        self.assertEqual(result['initialize_ms'], 12)

    def test_shutdown_command_exception_still_kills_and_reaps_only_owned_child(self):
        child = Mock()
        child.poll.side_effect = [None, None]
        child.wait.side_effect = [subprocess.TimeoutExpired('owned-child', 5), 0]
        fixture = SimpleNamespace(child=child, run=Mock(side_effect=OSError('private shutdown path')),
                                  control='private control', log=Mock())
        original = BENCH.CampaignError('source_changed')
        self.assertTrue(BENCH.close_owned_fixture(fixture, original))
        child.kill.assert_called_once()
        self.assertEqual(child.wait.call_count, 2)
        fixture.log.close.assert_called_once()
        self.assertEqual(str(original), 'source_changed')

    def test_mcp_request_uses_remaining_campaign_budget_and_does_not_send_after_expiry(self):
        session = BENCH.Session.__new__(BENCH.Session)
        session.fixture = SimpleNamespace(command_timeout=100)
        session.deadline, session.counter = .25, 0
        session.send = Mock()
        session.responses = Mock()
        session.responses.get.return_value = {'id': 1, 'result': {'accepted': True}}
        with patch.object(BENCH.time, 'monotonic', return_value=.1):
            self.assertEqual(session.request('tools/call', {}), {'accepted': True})
        self.assertAlmostEqual(session.responses.get.call_args.kwargs['timeout'], .15)
        with patch.object(BENCH.time, 'monotonic', return_value=.26):
            with self.assertRaisesRegex(BENCH.CampaignError, 'campaign_deadline'):
                session.request('tools/call', {})
        self.assertEqual(session.send.call_count, 1)

    def runtime(self, root):
        pair = {'schema': 1, 'version': '1.8.0', 'os': 'windows' if os.name == 'nt' else 'linux', 'arch': 'x86_64'}
        for name, key in [('airk', 'airk_sha256'), ('airk-embed', 'embed_sha256')]:
            binary = root / (name + ('.exe' if os.name == 'nt' else ''))
            binary.write_bytes(b'original ' + name.encode())
            pair[key] = BENCH.digest(binary)
        (root / 'airk-runtime.json').write_text(json.dumps(pair))
        (root / 'airk-build.json').write_text(json.dumps({'schema': 1, 'runtime': pair, 'profile': 'release'}))
        files = {path.name: BENCH.digest(path) for path in root.iterdir()}
        (root / 'airk-install.json').write_text(json.dumps({'schema': 1, 'version': '1.8.0', 'files': files}))
        return BENCH.digest(root / 'airk-install.json')

    def test_approved_pair_is_bound_to_inventory_and_compiled_versions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve(); approved = self.runtime(root)
            def run(command, **_):
                name = Path(command[0]).stem
                return subprocess.CompletedProcess(command, 0, f'{name} 1.8.0'.encode(), b'')
            with patch.object(BENCH.subprocess, 'run', side_effect=run) as invoke:
                report = BENCH.verify_runtime(root, approved)
                self.assertEqual(invoke.call_count, 2)
            self.assertEqual(report['install_sha256'], approved)
            self.assertNotIn(str(root), json.dumps(report))
            (root / 'airk-runtime.json').write_text('{}')
            with self.assertRaisesRegex(BENCH.CampaignError, 'runtime_schema'):
                BENCH.verify_runtime(root, approved)

    def test_extra_payload_and_wrong_approval_fail_before_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve(); approved = self.runtime(root)
            (root / 'unexpected').write_text('unknown')
            with patch.object(BENCH.subprocess, 'run') as invoke:
                with self.assertRaisesRegex(BENCH.CampaignError, 'runtime_inventory_extra'):
                    BENCH.verify_runtime(root, approved)
                with self.assertRaisesRegex(BENCH.CampaignError, 'runtime_approval_mismatch'):
                    BENCH.verify_runtime(root, '0' * 64)
                invoke.assert_not_called()


if __name__ == '__main__':
    unittest.main()
