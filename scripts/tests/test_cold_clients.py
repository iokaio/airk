# SPDX-License-Identifier: Apache-2.0
"""Cold-client harness tests exercise scheduling, refusal and sibling cleanup."""
import copy
import importlib.util
import json
from pathlib import Path
import threading
import unittest
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location('cold_clients', Path(__file__).parents[1] / 'check-service-clients.py')
CLIENTS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CLIENTS)

SELECTED = {'grant_id':'grant', 'expected_grant_revision':7, 'index_id':'index', 'project_ids':['project']}
STATUS = {'index_state':'available', 'files':0, 'publication':{'token':'empty'},
          'scope_observation':{'qualifier':'selected_subset', 'provenance':'user_reviewed_project_grant',
          'grant':{'id':'grant','revision':7}, 'members':[{'index_id':'index','memberships':[{'project_id':'project'}]}]}}

class Network:
    def __init__(self, fixture):
        self.fixture = fixture
        self.connection = Mock()
        self.closed = False
        self.replies = {}
    def send(self, request):
        self.fixture.calls.append(request['method'])
        if request['method'] == 'initialize':
            self.replies[1] = {'result':{'serverInfo':{'name':'airk'}}}
        elif request['method'] == 'tools/call':
            assert request['params'] == {'name':'status', 'arguments':{}}
            if self.fixture.barrier:
                self.fixture.barrier.wait(timeout=5)
            self.replies[2] = {'result':{'content':[{'type':'text','text':json.dumps(self.fixture.status)}]}}
    def receive(self, identifier):
        return self.replies.pop(identifier)
    def close(self):
        self.closed = True

class Fixture:
    single_image = catalog_authority = True
    scopes = {'cold':SELECTED}
    def __init__(self, *, fail_open=False):
        self.sessions = []; self.calls = []; self.status = copy.deepcopy(STATUS)
        self.fail_open = fail_open
        self.barrier = None
        self.rpc = Mock(return_value={'status':'embedding_status','loads':0})
        self.assert_alive = Mock()
    def open_http(self, name, client):
        assert name == 'cold'
        if self.fail_open and client.endswith('-1'):
            raise ValueError('injected admission failure')
        network = Network(self); self.sessions.append(network)
        return network

class ColdClientsTests(unittest.TestCase):
    def test_both_admission_modes_have_four_parallel_first_reads_and_no_warmup(self):
        for mode in ('sequential','concurrent'):
            fixture = Fixture(); fixture.barrier = threading.Barrier(4)
            result = CLIENTS.cold_first_status(fixture, mode)
            self.assertEqual(result['clients'], 4)
            self.assertEqual(len(result['observations']), 4)
            self.assertEqual(fixture.calls.count('tools/call'), 4)
            self.assertEqual(set(fixture.calls), {'initialize','notifications/initialized','tools/call'})
            self.assertEqual(len(fixture.sessions),4)
            self.assertTrue(all(session.closed for session in fixture.sessions))
            fixture.rpc.assert_called_once()
            self.assertEqual(fixture.rpc.call_args.args, ('embedding_status',))
    def test_status_keeps_original_sixty_second_budget_starting_after_admission(self):
        fixture = Fixture(); clock = {"now": 1000}
        original = fixture.open_http
        def opened(name, client):
            clock["now"] += 1
            return original(name, client)
        fixture.open_http = opened
        with patch.object(CLIENTS.time, "monotonic", side_effect=lambda: clock["now"]):
            CLIENTS.cold_first_status(fixture, "sequential")
        self.assertEqual(fixture.rpc.call_args.kwargs["deadline"], 1064)
        fixture = Fixture()
        with patch.object(CLIENTS.time, "monotonic", return_value=1000):
            CLIENTS.cold_first_status(fixture, "sequential", case_deadline=1010)
        self.assertEqual(fixture.rpc.call_args.kwargs["deadline"], 1010)

    def test_failed_concurrent_open_collects_and_closes_every_successful_sibling(self):
        fixture = Fixture(fail_open=True)
        with self.assertRaisesRegex(ValueError,'admission failure'):
            CLIENTS.cold_first_status(fixture, 'concurrent')
        self.assertEqual(len(fixture.sessions),3)
        self.assertTrue(all(session.closed for session in fixture.sessions))
        self.assertNotIn('tools/call',fixture.calls)
    def test_first_status_refuses_widened_scope_indexed_or_unknown_result(self):
        changes = [lambda s:s.update(project='legacy'),
                   lambda s:s['scope_observation'].update(qualifier='all'),
                   lambda s:s['scope_observation'].update(provenance='ambient'),
                   lambda s:s['scope_observation']['grant'].update(revision=8),
                   lambda s:s['scope_observation']['members'][0].update(index_id='other'),
                   lambda s:s['scope_observation']['members'][0]['memberships'].append({'project_id':'other'}),
                   lambda s:s.update(files=1), lambda s:s.update(index_state='unknown')]
        for mutate in changes:
            fixture=Fixture();mutate(fixture.status)
            with self.subTest(status=fixture.status), self.assertRaises(AssertionError):
                CLIENTS.cold_first_status(fixture,'sequential')
            self.assertTrue(all(session.closed for session in fixture.sessions))
    def test_zero_load_invariant_and_unavailable_empty_status(self):
        fixture=Fixture();fixture.status.update(index_state='unavailable',files=None,publication=None)
        self.assertEqual(len(CLIENTS.cold_first_status(fixture,'concurrent')['observations']),4)
        fixture=Fixture();fixture.rpc.return_value['loads']=1
        with self.assertRaises(AssertionError):
            CLIENTS.cold_first_status(fixture,'sequential')
        self.assertTrue(all(session.closed for session in fixture.sessions))

if __name__ == '__main__':
    unittest.main()
