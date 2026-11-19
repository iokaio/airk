# SPDX-License-Identifier: Apache-2.0
"""Independent bounded adapter controls; no private fixtures or native dispatch."""
import copy
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import unittest
from unittest.mock import patch
import admission as a
import run_composed as worker
from evidence_checks import search_preservation


def documents():
    policy = {'schema': 'airk.arbiter.resource-policy/2', 'deadline_ms': 120000,
              'shared_input_tokens': 8192, 'focus_pair_input_tokens': 512}
    delivery = {'schema': 'airk.arbiter.budget-cases/1', 'operation_deadline_ms': 120000,
                'transport_deadline_ms': 125000, 'default_budget': {'input_tokens': 8192}}
    root = a.ROOT / 'artifacts/composed-control'
    pin = lambda name: {'path': str(root / name), 'sha256': 'a' * 64}
    plan = {'schema': 'airk.private.composed-execution-plan/1', 'adapter': 'airk.maintained.composed-service/1',
            'whole_phase_seconds': 600, 'resource_policy_input': pin('policy.json'),
            'native_delivery_input': pin('budget.json'), 'allowed_native_images': [str(root / 'airkctl.exe')],
            'steps': [{'kind': 'controller', 'request': {'kind': 'start', 'deadline_ms': 125000, 'args_file': str(root / 'request.json')}},
                      {'kind': 'controller', 'request': {'kind': 'finish'}}]}
    packet = {'adapter': 'airk.maintained.composed-service/1', 'whole_phase_seconds': 600,
              'resource_policy_input': pin('policy.json'), 'native_delivery_input': pin('budget.json')}
    return plan, packet, policy, delivery


class AdapterControls(unittest.TestCase):
    def test_current_policy_transport_and_focus_are_separate(self):
        plan, packet, policy, delivery = documents()
        a.validate_shape(plan, packet, policy, delivery)
        for field, value in [('deadline_ms', 30000), ('shared_input_tokens', 2048),
                             ('focus_pair_input_tokens', 8192), ('deadline_ms', True)]:
            changed = dict(policy, **{field: value})
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                a.validate_shape(plan, packet, changed, delivery)
        for value in [120000, 125001, True]:
            with self.assertRaises(ValueError):
                a.validate_shape(plan, packet, policy, dict(delivery, transport_deadline_ms=value))

    def test_missing_finish_unknown_action_foreign_binding_and_oversized_phase_refuse(self):
        original, packet, policy, delivery = documents()
        changes = [lambda p: p['steps'].pop(), lambda p: p['steps'][0]['request'].update(kind='retry'),
                   lambda p: p.update(whole_phase_seconds=1801), lambda p: p.update(whole_phase_seconds=float('nan')),
                   lambda p: p.update(adapter='historical'), lambda p: p['steps'][0]['request'].update(deadline_ms=125001)]
        for change in changes:
            p = copy.deepcopy(original);change(p)
            with self.assertRaises(ValueError):
                a.validate_shape(p, packet, policy, delivery)
        with self.assertRaises(ValueError):
            a.validate_shape(original, dict(packet, native_delivery_input={'path': '/foreign', 'sha256': 'b' * 64}), policy, delivery)

    def test_clocks_never_grant_a_minimum_or_refresh(self):
        for now in [10, 10.1, 9.9999]:
            with self.assertRaises(TimeoutError):
                a.remaining_ms(10, now)
        self.assertEqual(a.remaining_ms(10, 9.5), 500)
        step = {'kind': 'controller', 'request': {'kind': 'start', 'deadline_ms': 125000}}
        for available, expected in [(0.5, 500), (0.125, 125)]:
            result = worker.controller_request(step, 2, [], {}, None, lambda: available)
            self.assertEqual(result['deadline_ms'], expected)
            self.assertEqual(step['request']['deadline_ms'], 125000)
        with self.assertRaises(TimeoutError):
            worker.controller_request(step, 2, [], {}, None, lambda: 0.0009)

    def test_warm_watch_keeps_original_configuration_clock(self):
        step = {'request': {'kind': 'watch', 'timeout_ms': 120000},
                'warm_configuration_observation_step': 0, 'warm_deadline_seconds': 120}
        rows = [{'request': {'kind': 'native_command'}}]
        got = worker.controller_request(step, 1, rows, {0: 100}, None, lambda: 600, clock=lambda: 219.5)
        self.assertEqual(got['timeout_ms'], 500)
        self.assertTrue(got['warm_observation'])
        with self.assertRaises(TimeoutError):
            worker.controller_request(step, 1, rows, {0: 100}, None, lambda: 600, clock=lambda: 220)

    def test_parse_duplicates_and_consumed_pin_drift_refuse(self):
        with self.assertRaises(ValueError):
            a.parse(b'{"a":1,"a":2}')
        with self.assertRaises(ValueError):
            a.parse(b'{"a":NaN}')
        class MemoryFile:
            def open(self, mode):
                return io.BytesIO(b'original')
        with patch.object(a, 'regular', return_value=MemoryFile()):
            pin = {'path': '/bounded', 'sha256': hashlib.sha256(b'original').hexdigest(), 'bytes': 8}
            self.assertEqual(a.captured(pin), b'original')
            for changed in [dict(pin, sha256='f' * 64), dict(pin, bytes=7)]:
                with self.assertRaises(ValueError):
                    a.captured(changed)

    def test_private_output_refuses_tracked_and_alias_roots(self):
        for path in [a.ROOT / 'scripts/result.json', a.ROOT / 'artifacts', Path('relative')]:
            with self.assertRaises(ValueError):
                a.private_path(path)

    def test_source_sdk_and_native_seals_are_mandatory(self):
        plan, packet, policy, delivery = documents()
        root = a.ROOT / 'artifacts/composed-control'
        # Literal roster is independent of admission.SOURCE_NAMES.
        names = ['admission.py', 'prepare.py', 'run_composed.py', 'native_policy.py', 'native_delivery.py',
                 'session_controller.cjs', 'native_policy.cjs', 'native_delivery.cjs', 'native_eval_session.cjs',
                 'evidence_checks.py', 'assemble_observations.py']
        observer = a.ROOT / 'scripts/arbiter/evaluation/research_history/composed_orchestration/os_measurement--c81d48c34df4.py'
        sources = {str(a.HERE / n): 'a' * 64 for n in names}
        sources[str(observer)] = '75355bdecbeb75e436053b1118841268a0387cf96552d5b083e5b4f46a31e196'
        plan.update(source_seals=sources, controller=str(a.HERE / 'session_controller.cjs'),
                    controller_packet=str(root / 'packet.json'), output_root=str(root / 'output'), node=str(root / 'node.exe'))
        packet.update(session_helper=str(a.HERE / 'native_eval_session.cjs'),
                      session_library=str(a.ROOT / 'packages/vscode/out/airkctl/session.js'),
                      client_dependencies=[str(a.ROOT / 'packages/vscode/out/airkctl' / n) for n in ['runner.js', 'adminChunks.js']]
                      + [str(a.ROOT / 'packages/vscode/out/generated/arbiterBudget.js')])
        all_paths = [plan['controller_packet'], plan['node'], *plan['allowed_native_images'],
                     plan['resource_policy_input']['path'], plan['native_delivery_input']['path'],
                     packet['session_library'], *packet['client_dependencies'], str(root / 'request.json')]
        plan['pins'] = dict(sources, **{p: 'a' * 64 for p in all_paths})
        packet['pins'] = dict(plan['pins'])
        original_pins = dict(plan['pins'])
        def reader(pin):
            return {plan['controller_packet']: packet, plan['resource_policy_input']['path']: {'policy': policy},
                    plan['native_delivery_input']['path']: delivery}[pin['path']]
        with patch.object(a, 'private_path', side_effect=lambda p, **kw: Path(p)), patch.object(a, 'regular', side_effect=Path), patch.object(a, 'captured', return_value=b''):
            a.validate_plan(plan, reader=reader, hasher=original_pins.__getitem__)
            for key in [str(a.HERE / 'native_eval_session.cjs'), packet['client_dependencies'][-1], plan['allowed_native_images'][0]]:
                changed = copy.deepcopy(plan);changed['pins'].pop(key)
                with self.assertRaises((ValueError, KeyError)):
                    a.validate_plan(changed, reader=reader, hasher=original_pins.__getitem__)
            changed = copy.deepcopy(plan);changed['pins'][str(a.HERE / 'native_policy.py')] = 'b' * 64
            with self.assertRaises(ValueError):
                a.validate_plan(changed, reader=reader, hasher=original_pins.__getitem__)

    def test_complete_search_preserves_unknown_fields_and_sources(self):
        before = {'hits': [{'id': 'a', 'source': {'revision': 1}}, {'id': 'b'}], 'future': {'unknown': [1]}, 'truncated': False}
        after = copy.deepcopy(before);after['hits'].reverse()
        after['focus'] = {'kind': 'receipt', 'receipt': {'applied': True, 'identity': {'manifest': 'p'}, 'scored_count': 2,
                                                      'original_order': ['a', 'b'], 'ranked_order': ['b', 'a']}}
        self.assertEqual(search_preservation(before, after)['hits'], 2)
        after['hits'][1]['source']['revision'] = 2
        with self.assertRaises(ValueError):
            search_preservation(before, after)

    def test_worker_has_no_standalone_dispatch(self):
        with self.assertRaisesRegex(ValueError, 'multiprocess boundary'):
            worker.run('/not-read', '/not-created', expected_sha256='a' * 64, owned_boundary=None)

    def test_node_error_and_event_budget_controls(self):
        node = shutil.which('node')
        self.assertIsNotNone(node, 'Node is required; zero-selection is not a pass')
        code = """const assert=require('node:assert/strict');
const c=require(process.argv[1]);
const result={class:'unavailable',error:{code:'busy',retryable:true,retry_after_ms:27,native:{future:1}}};
const error={message:'busy',code:'session_busy',sent:true,outcomeUnknown:true,retryable:true,result};
assert.deepEqual(c.errorObservation(error),{message:'busy',code:'session_busy',sent:true,outcome_unknown:true,retryable:true,result,retry:false});
for(const n of [100,101,99.9])assert.throws(()=>c.waitBudget(100,n));
assert.equal(c.waitBudget(100,95),5);
assert.throws(()=>c.readPacket('/never-read','invalid'));
console.log('6 literal Node assertions pass');
"""
        result = subprocess.run([node, '-e', code, str(a.HERE / 'session_controller.cjs')], capture_output=True,
                                text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('6 literal Node assertions pass', result.stdout)


if __name__ == '__main__':
    unittest.main()
