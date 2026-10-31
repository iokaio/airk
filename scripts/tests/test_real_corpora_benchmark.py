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
        self.assertEqual(report, {'requested_samples': 3, 'recorded_samples': 3, 'first_status_observations': 3, 'first_status_missing': 0, 'first_status_p50_ms': 4, 'first_status_p95_ms': 2000, 'first_status_failures': 1})

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

    def test_expired_pre_send_budget_is_recorded_without_submission_or_replay(self):
        session = BENCH.Session.__new__(BENCH.Session)
        session.fixture = SimpleNamespace(command_timeout=100, projects={'fixture': {}})
        session.deadline, session.counter = .25, 0
        session.send, session.responses = Mock(), Mock()
        with patch.object(BENCH.time, 'monotonic', return_value=.26):
            with self.assertRaises(BENCH.ObservationFailure) as caught:
                session.tool('search', {'query': 'PRIVATE_QUERY', 'mode': 'lexical'})
        observed = caught.exception.observation
        self.assertEqual(observed['code'], 'campaign_deadline')
        self.assertTrue(observed['budget_exhausted'])
        self.assertEqual(observed['submission'], 'not_submitted')
        self.assertNotIn('transport_or_protocol_failure', observed)
        self.assertEqual(session.counter, 0)
        session.send.assert_not_called()
        session.responses.get.assert_not_called()
        self.assertNotIn('PRIVATE', json.dumps(observed))
        counter = BENCH.ProbeCounters([('search', {'mode': 'lexical'})])
        counter.observe(0, observed)
        self.assertEqual(counter.report['probes'][0]['budget_exhausted'], 1)
        self.assertEqual(counter.report['probes'][0]['native_error'], 0)
        self.assertEqual(counter.report['probes'][0]['transport_protocol_failure'], 0)
        self.assertEqual(counter.report['observation_outcome'], 'budget_exhausted')
        self.assertEqual(BENCH.observation_outcome({'corpora': [{'probe_summary': counter.report}]}), 'budget_exhausted')

    def test_budget_abort_keeps_first_status_and_prior_probe_errors(self):
        session = BENCH.Session.__new__(BENCH.Session)
        session.fixture = SimpleNamespace(command_timeout=100, projects={'fixture': {}})
        session.deadline, session.counter, session.initialize_ms = .25, 0, 12
        session.send, session.responses = Mock(), Mock()
        class OpenSession:
            def __init__(self, *_, **kwargs): pass
            def __enter__(self): return session
            def __exit__(self, *_): pass
        with patch.object(BENCH.time, 'monotonic', return_value=.26):
            result = BENCH.sample(session.fixture, 'fixture', [], OpenSession)
        self.assertTrue(result['aborted'] and result['budget_exhausted'])
        self.assertEqual(result['first_status']['submission'], 'not_submitted')
        self.assertEqual(BENCH.observation_outcome({'corpora': [result]}), 'budget_exhausted')
        session.send.assert_not_called()
        session.responses.get.assert_not_called()
        counter = BENCH.ProbeCounters([('search', {'mode': 'lexical'})])
        first = {'is_error': True, 'code': 'native_tool_error'}
        counter.observe(0, first)
        counter.observe(0, result['first_status'])
        self.assertEqual(counter.report['probes'][0]['first_error']['code'], 'native_tool_error')
        self.assertEqual(counter.report['probes'][0]['native_error'], 1)
        self.assertEqual(counter.report['probes'][0]['budget_exhausted'], 1)
        self.assertEqual(counter.report['observation_outcome'], 'native_errors_observed')

    def test_response_timeout_after_send_stays_uncertain_transport_without_replay(self):
        session = BENCH.Session.__new__(BENCH.Session)
        session.fixture = SimpleNamespace(command_timeout=100, projects={'fixture': {}})
        session.deadline, session.counter = .25, 0
        session.send, session.responses = Mock(), Mock()
        session.responses.get.side_effect = BENCH.queue.Empty
        with patch.object(BENCH.time, 'monotonic', return_value=.1):
            with self.assertRaises(BENCH.ObservationFailure) as caught:
                session.tool('search', {'mode': 'lexical'})
        observed = caught.exception.observation
        self.assertEqual(observed['code'], 'mcp_response_timeout')
        self.assertTrue(observed['transport_or_protocol_failure'])
        self.assertNotIn('budget_exhausted', observed)
        self.assertNotEqual(observed.get('submission'), 'not_submitted')
        self.assertEqual(session.counter, 1)
        session.send.assert_called_once()
        session.responses.get.assert_called_once()

    def test_cleanup_grace_failure_stays_failed_after_successful_owned_reap(self):
        child = Mock()
        child.poll.side_effect = [None, None]
        child.wait.side_effect = [subprocess.TimeoutExpired('PRIVATE_CHILD', 5), 0]
        fixture = SimpleNamespace(child=child, run=Mock(), control='PRIVATE_CONTROL', log=Mock())
        primary = BENCH.CampaignError('campaign_deadline')
        detail = {}
        self.assertTrue(BENCH.close_owned_fixture(fixture, primary, detail))
        self.assertEqual(detail, {'failed_stages': ['graceful_wait'], 'owned_reap': 'succeeded'})
        self.assertEqual(str(primary), 'campaign_deadline')
        child.kill.assert_called_once()
        self.assertEqual(child.wait.call_count, 2)
        fixture.run.assert_called_once()
        fixture.log.close.assert_called_once()
        self.assertNotIn('PRIVATE', json.dumps(detail))

    def test_cleanup_detail_preserves_existing_failure_without_a_primary(self):
        child = Mock()
        child.poll.side_effect = [None, None]
        child.wait.side_effect = [subprocess.TimeoutExpired('owned', 5), 0]
        fixture = SimpleNamespace(child=child, run=Mock(), control='private', log=Mock())
        detail = {}
        with self.assertRaisesRegex(BENCH.CampaignError, 'service_cleanup_failed'):
            BENCH.close_owned_fixture(fixture, detail=detail)
        self.assertEqual(detail['failed_stages'], ['graceful_wait'])
        self.assertEqual(detail['owned_reap'], 'succeeded')

    def test_probe_report_distinguishes_modes_with_opaque_query_identity(self):
        secret = 'PRIVATE_QUERY_TEXT'
        observed = [BENCH.probe_identity(n, 'search', {'query': secret, 'mode': mode})
                    for n, mode in enumerate(('lexical', 'vector', 'hybrid'))]
        self.assertEqual([row['ordinal'] for row in observed], [0, 1, 2])
        self.assertEqual([row['mode'] for row in observed], ['lexical', 'vector', 'hybrid'])
        self.assertEqual([row['kind'] for row in observed], ['lexical', 'vector', 'hybrid'])
        self.assertEqual(len({row['query_sha256'] for row in observed}), 1)
        self.assertEqual(observed[0]['query_sha256'], BENCH.hashlib.sha256(secret.encode()).hexdigest())
        self.assertNotIn(secret, json.dumps(observed))
        self.assertEqual(BENCH.probe_identity(3, 'read_file', {'path': 'PRIVATE_PATH'})['kind'], 'excerpt')
        self.assertNotIn('PRIVATE_PATH', json.dumps(BENCH.probe_identity(3, 'read_file', {'path': 'PRIVATE_PATH'})))

    def test_environment_fields_are_minimal_and_unknown_capacity_stays_unmeasured(self):
        with patch.object(BENCH.platform, 'system', return_value='Windows'), \
             patch.object(BENCH.platform, 'release', return_value='11'), \
             patch.object(BENCH.platform, 'machine', return_value='AMD64'), \
             patch.object(BENCH.platform, 'python_version', return_value='3.13.1'), \
             patch.object(BENCH.os, 'cpu_count', return_value=64):
            observed = BENCH.environment_facts()
        self.assertEqual(observed, {'os': 'Windows', 'os_release': '11', 'machine': 'AMD64',
            'logical_cpus': 64, 'python': '3.13.1', 'storage_class': 'unmeasured',
            'cache_eviction': 'not_performed', 'antivirus_activity': 'unmeasured', 'effective_affinity': 'unmeasured'})

    def test_preindex_preparation_failure_retains_elapsed_time_without_private_error(self):
        recorded = {}
        def fail(): raise OSError('PRIVATE_SETUP_ERROR')
        with patch.object(BENCH.time, 'perf_counter', side_effect=[1.0, 1.25]):
            with self.assertRaises(OSError):
                BENCH.timed_preparation(recorded, 'model_setup_ms', fail)
        self.assertEqual(recorded, {'model_setup_ms': 250.0})
        self.assertNotIn('PRIVATE_SETUP_ERROR', json.dumps(recorded))

    def test_empty_head_read_is_scoped_unknown_without_usable_excerpt(self):
        expected = {'collection': 'fixture', 'binding': 'binding'}
        publication = {'collection': 'fixture', 'token': 'airk-r1:' + 'a' * 32 + ':1:' + 'b' * 64}
        read = {'publication': publication, 'current_publication': publication, 'historical': False,
                'status': 'unknown', 'requested_path': 'PRIVATE_README', 'resolved_path': None,
                'reason': 'PRIVATE_ABSENT_PATH_MESSAGE'}
        observed = BENCH.safe_observation(envelope({'read': read}), expected, 'read_file')
        self.assertEqual(observed['read_status'], 'unknown')
        self.assertIsNone(observed['hits'])
        self.assertFalse(observed['is_error'])
        self.assertNotIn('PRIVATE', json.dumps(observed))
        for changed in ({**read, 'publication': {**publication, 'collection': 'foreign'}},
                        {**read, 'current_publication': {**publication, 'token': 'airk-r1:' + 'c' * 32 + ':2:' + 'd' * 64}},
                        {**read, 'publication': None, 'current_publication': None},
                        {**read, 'historical': True}):
            with self.assertRaises(BENCH.CampaignError):
                BENCH.safe_observation(envelope({'read': changed}), expected, 'read_file')
        with self.assertRaisesRegex(BENCH.CampaignError, 'unexpected_conditional_payload'):
            BENCH.safe_observation(envelope({'read': read, 'hits': []}), expected, 'read_file')

    def test_scoped_conditional_payload_must_match_nested_publication(self):
        expected = {'collection': 'fixture', 'binding': 'binding'}
        publication = {'collection': 'fixture', 'token': 'airk-r1:' + 'a' * 32 + ':1:' + 'b' * 64}
        read = {'publication': publication, 'current_publication': publication, 'historical': False, 'status': 'content'}
        value = {'collection': 'fixture', 'publication': publication, 'read': read, 'hits': [{'text': 'PRIVATE_CONTENT'}]}
        self.assertEqual(BENCH.safe_observation(envelope(value), expected, 'read_file')['hits'], 1)
        crossed = {**value, 'publication': {**publication, 'token': 'airk-r1:' + 'c' * 32 + ':2:' + 'd' * 64}}
        with self.assertRaisesRegex(BENCH.CampaignError, 'inconsistent_publication'):
            BENCH.safe_observation(envelope(crossed), expected, 'read_file')

    def test_complete_plain_text_native_error_is_redacted_without_becoming_transport_failure(self):
        expected = {'collection': 'fixture', 'binding': 'binding'}
        native_error = {'isError': True, 'content': [{'type': 'text', 'text': 'PRIVATE_NATIVE_ERROR_PATH'}]}
        observed = BENCH.safe_observation(native_error, expected, 'search')
        self.assertTrue(observed['is_error'])
        self.assertEqual(observed['code'], 'native_tool_error')
        self.assertIsNone(observed['hits'])
        self.assertNotIn('PRIVATE', json.dumps(observed))
        self.assertNotIn('transport_or_protocol_failure', observed)
        with self.assertRaisesRegex(BENCH.CampaignError, 'invalid_tool_envelope'):
            BENCH.safe_observation({**native_error, 'isError': False}, expected, 'search')
        with self.assertRaisesRegex(BENCH.CampaignError, 'invalid_tool_envelope'):
            BENCH.safe_observation({'isError': True, 'content': []}, expected, 'search')
        with self.assertRaisesRegex(BENCH.CampaignError, 'crossed_response_scope'):
            BENCH.safe_observation(envelope({'collection': 'foreign', 'error': {'code': 'rejected'}}, True), expected, 'search')

    def test_status_denominators_preserve_missing_and_failed_observations(self):
        samples = [{'first_status': None, 'aborted': True},
                   {'first_status': {'elapsed_ms': 2000, 'is_error': False, 'index_state': 'unknown'}}]
        observed = BENCH.timings(samples, requested=3)
        self.assertEqual(observed['requested_samples'], 3)
        self.assertEqual(observed['recorded_samples'], 2)
        self.assertEqual(observed['first_status_observations'], 1)
        self.assertEqual(observed['first_status_missing'], 2)
        self.assertEqual(observed['first_status_failures'], 1)
        missing = BENCH.timings([{'first_status': None}], requested=2)
        self.assertEqual(missing['first_status_missing'], 2)
        self.assertEqual(missing['first_status_failures'], 0)
        self.assertIsNone(missing['first_status_p50_ms'])

    def test_probe_counters_preserve_initial_errors_then_later_usable_without_unbounded_log(self):
        probes = [('search', {'query': 'PRIVATE_QUERY', 'mode': 'vector'}), ('read_file', {'path': 'PRIVATE_PATH'})]
        counter = BENCH.ProbeCounters(probes)
        counter.observe(0, {'is_error': True, 'code': 'native_tool_error'}, 12)
        for _ in range(100):
            counter.observe(0, {'is_error': True, 'code': 'native_tool_error'}, 15)
        counter.observe(0, {'is_error': False, 'hits': 1}, 75)
        counter.observe(1, {'is_error': False, 'read_status': 'unknown', 'hits': None}, 20)
        counter.observe(1, {'is_error': False, 'read_status': 'content', 'hits': 1}, 80)
        report = counter.report
        self.assertTrue(report['all_probes_eventually_usable'])
        self.assertEqual(report['observation_outcome'], 'native_errors_observed')
        self.assertEqual(report['probes'][0]['native_error'], 101)
        self.assertEqual(report['probes'][0]['observed'], 102)
        self.assertEqual(report['probes'][0]['first_usable_ms'], 75)
        self.assertEqual(report['probes'][1]['typed_no_content'], 1)
        self.assertNotIn('PRIVATE', json.dumps(report))
        self.assertNotIn('observations', report['probes'][0])
        self.assertLess(len(json.dumps(report)), 3500)

    def test_per_phase_probe_summary_counts_missing_and_partial_errors(self):
        probes = [('list_files', {}), ('search', {'query': 'configuration', 'mode': 'lexical'})]
        samples = [{'reads': [{'ordinal': 0, 'is_error': False, 'items': 0},
                             {'ordinal': 1, 'is_error': True, 'transport_or_protocol_failure': True}]},
                   {'reads': []}]
        report = BENCH.summarize_samples(probes, samples, requested=3)
        self.assertEqual(report['probes'][0]['success_empty'], 1)
        self.assertEqual(report['probes'][0]['not_observed'], 2)
        self.assertEqual(report['probes'][1]['transport_protocol_failure'], 1)
        self.assertEqual(report['probes'][1]['not_observed'], 2)
        self.assertEqual(report['observation_outcome'], 'transport_protocol_failures_observed')

    def test_independent_availability_retains_first_errors_then_reports_later_recovery(self):
        calls = []
        probes = [('search', {'query': 'configuration', 'mode': 'vector'})]
        counter = BENCH.ProbeCounters(probes)
        counter.observe(0, {'is_error': True, 'code': 'native_tool_error'}, 10)
        class Session:
            def __init__(self, *args, **kwargs): calls.append('new_session')
            def __enter__(self): return self
            def __exit__(self, *_): pass
            def tool(self, name, arguments=None):
                calls.append(name)
                return {}, {'is_error': False, 'index_state': 'available', 'hits': 1}
        output = {}
        with patch.object(BENCH, 'Session', Session), patch.object(BENCH.time, 'sleep'):
            BENCH.independent_availability(None, 'project', probes, BENCH.time.perf_counter(), BENCH.time.monotonic() + 5, counter, output)
        self.assertEqual(calls, ['new_session', 'status', 'search'])
        self.assertEqual(output['independent_sessions'], 1)
        self.assertEqual(output['stop_reason'], 'all_probes_observed_usable')
        self.assertTrue(counter.report['all_probes_eventually_usable'])
        self.assertEqual(counter.report['observation_outcome'], 'native_errors_observed')
        self.assertEqual(counter.report['probes'][0]['native_error'], 1)
        self.assertEqual(counter.report['probes'][0]['usable_nonempty'], 1)

    def test_independent_availability_never_replays_uncertain_transport_failure(self):
        calls = []
        probes = [('search', {'query': 'configuration', 'mode': 'vector'})]
        counter = BENCH.ProbeCounters(probes)
        class Session:
            def __init__(self, *args, **kwargs): calls.append('new_session')
            def __enter__(self): return self
            def __exit__(self, *_): pass
            def tool(self, name, arguments=None):
                calls.append(name)
                if name == 'status': return {}, {'is_error': False, 'index_state': 'available'}
                raise BENCH.ObservationFailure('mcp_response_timeout', {'is_error': True, 'transport_or_protocol_failure': True, 'elapsed_ms': 500})
        output = {}
        with patch.object(BENCH, 'Session', Session), patch.object(BENCH.time, 'sleep'):
            with self.assertRaises(BENCH.ObservationFailure):
                BENCH.independent_availability(None, 'project', probes, BENCH.time.perf_counter(), BENCH.time.monotonic() + 5, counter, output)
        self.assertEqual(calls, ['new_session', 'status', 'search'])
        self.assertTrue(output['aborted'])
        self.assertEqual(counter.report['probes'][0]['transport_protocol_failure'], 1)

    def test_workflow_completion_does_not_relabel_observed_probe_errors(self):
        counter = BENCH.ProbeCounters([('search', {'query': 'configuration', 'mode': 'vector'})])
        counter.observe(0, {'is_error': True, 'code': 'native_tool_error'}, 10)
        report = {'completed': True, 'corpora': [{'reopen': {'probe_summary': counter.report}}]}
        self.assertEqual(BENCH.observation_outcome(report), 'native_errors_observed')

    def test_initial_polling_keeps_failed_observations_before_later_usable_result(self):
        probes = [('search', {'query': 'configuration', 'mode': 'vector'})]
        first = {'initialize_ms': 1, 'first_status': {'is_error': False, 'index_state': 'available'},
                 'reads': [{'ordinal': 0, 'tool': 'search', 'kind': 'vector', 'completed_ms': 2,
                            'is_error': True, 'code': 'native_tool_error'}], 'aborted': False}
        job_calls, search_calls = [], []
        class Fixture:
            job_timeout = 5
            def rpc(self, method, **kwargs):
                job_calls.append(method)
                return {'job': {'id': 'accepted', 'project': 'binding', 'state': 'running' if len(job_calls) == 1 else 'succeeded'}}
        class Session:
            def __init__(self, *args, **kwargs): pass
            def __enter__(self): return self
            def __exit__(self, *_): pass
            def tool(self, name, arguments=None):
                if name == 'status':
                    return {}, {'is_error': False, 'index_state': 'available', 'semantic_state': 'ready'}
                search_calls.append(name)
                return {}, ({'is_error': True, 'code': 'native_tool_error'} if len(search_calls) == 1 else {'is_error': False, 'hits': 1})
        def sample(*args, output, **kwargs):
            output.update(first); return output
        with patch.object(BENCH, 'sample', side_effect=sample), patch.object(BENCH, 'Session', Session), \
             patch.object(BENCH, 'enqueue_owned', return_value={'job': {'id': 'accepted', 'project': 'binding'}}), \
             patch.object(BENCH.time, 'sleep'):
            result = BENCH.initial_index_campaign(Fixture(), 'project', probes, True)
        row = result['probe_summary']['probes'][0]
        self.assertEqual(row['observed'], 3)
        self.assertEqual(row['native_error'], 2)
        self.assertEqual(row['usable_nonempty'], 1)
        self.assertEqual(result['probe_summary']['observation_outcome'], 'native_errors_observed')
        self.assertEqual(result['missing_usable_probes'], 0)
        self.assertEqual(len(job_calls), 2)

    def test_reopen_preserves_first_sample_before_availability_and_labels_single_restart(self):
        probes = [('search', {'query': 'configuration', 'mode': 'vector'})]
        events = []
        with tempfile.TemporaryDirectory() as temporary:
            fixture = SimpleNamespace(owner=Path(temporary), child=None, command_timeout=5,
                                      start=lambda: events.append('service_start'))
            def sample(*args, output, **kwargs):
                events.append('sample')
                first = events.count('sample') == 1
                output.update({'first_status': {'elapsed_ms': 10, 'is_error': False, 'index_state': 'available'},
                    'reads': [{'ordinal': 0, 'tool': 'search', 'is_error': first, 'hits': None if first else 1,
                               'completed_ms': 20}], 'aborted': False})
                return output
            def availability(f, project, selected, origin, deadline, counter, output):
                events.append('availability')
                self.assertEqual(counter.report['probes'][0]['native_error'], 1)
                counter.observe(0, {'is_error': False, 'hits': 1}, 50)
                output['probe_summary'] = counter.report
            try:
                with patch.object(BENCH, 'close_owned_fixture', side_effect=lambda f: events.append('shutdown')), \
                     patch.object(BENCH, 'sample', side_effect=sample), \
                     patch.object(BENCH, 'independent_availability', side_effect=availability):
                    result = BENCH.reopen_campaign(fixture, 'project', probes, 2)
            finally:
                fixture.log.close()
        self.assertEqual(events, ['shutdown', 'service_start', 'sample', 'availability', 'sample'])
        self.assertEqual(result['service_restarts'], 1)
        self.assertEqual(result['first_after_restart_sample_index'], 0)
        self.assertEqual(result['first_sample_status_summary']['requested_samples'], 1)
        self.assertEqual(result['later_sample_status_summary']['requested_samples'], 1)
        self.assertEqual(result['probe_summary']['probes'][0]['native_error'], 1)
        self.assertTrue(result['availability']['probe_summary']['all_probes_eventually_usable'])
        self.assertGreaterEqual(result['shutdown_and_start_ms'], 0)

    def test_overall_outcome_retains_poll_status_failure_even_when_probe_summary_is_usable(self):
        counter = BENCH.ProbeCounters([('list_files', {})])
        counter.observe(0, {'is_error': False, 'items': 1}, 10)
        report = {'corpora': [{'initial_index': {'probe_summary': counter.report,
                   'status_observations': {'observed': 2, 'available': 1, 'failed_or_unavailable': 1}}}]}
        self.assertEqual(BENCH.observation_outcome(report), 'status_failures_observed')

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
