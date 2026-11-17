# SPDX-License-Identifier: Apache-2.0
"""Pure CPU-census adapter controls: no native launch, model, Torch or GPU."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('focus_training_census', ROOT / 'scripts/check-focus-training-census.py')
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)
H = '1' * 64
F = '2' * 64
SOURCE_NAMES = ['crates/airk-retrieval/src/arbiter/compiler.rs',
                'crates/airk-retrieval/src/arbiter/compiler/focus.rs',
                'crates/airk-retrieval/src/arbiter/compiler/focus/experiment.rs',
                'crates/airk-retrieval/src/arbiter/compiler/focus/experiment/tests/emission.rs']


def fixture_plan():
    p = {'schema': 'airk.private.focus-compiler-census-owned-plan/1',
         'selector': 'arbiter::compiler::focus::experiment::tests::emission::emit_original_human_pair_instruction_census',
         'limits': {'wall_seconds': 120, 'host_committed_bytes': 4294967296, 'log_bytes': 67108864,
                    'report_bytes': 536870912, 'cleanup_seconds': 5},
         'original_code_rows': 1955, 'fixed_replay_rows': 45,
         'human_sid': 'S-1-5-21-3678370870-3858642417-826968105-1002',
         'input_seals': {}, 'directory_censuses': {}, 'environment': {'PYTHONDONTWRITEBYTECODE': '1'},
         'repository_root': str(ROOT), 'run_directory': str(ROOT / 'artifacts/fresh-census/run'),
         'focus_source_lf_sha256': F, 'native_sources': {name: H for name in SOURCE_NAMES}}
    for name in ['runner', 'base_python', 'worker', 'parent', 'ownership', 'visibility', 'image',
                 'image_origin', 'cargo_json', 'native_build_receipt', 'native_report', 'roster', 'replay', 'tokenizer']:
        p[name] = {'path': str(ROOT / ('fixture-' + name)), 'sha256': H}
    p['runner']['path'] = str(ROOT / 'scripts/check-focus-training-census.py')
    # Literal independently reviewed helper identities, not values derived from M.SUPPORT.
    for name, digest in [('worker', '9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7'),
                         ('parent', 'fe4cc990649d92e041b79cccb3fd7ef1129979a26432b7f02b2d8d28b586ed8d'),
                         ('ownership', '2d7a31a64eaba745e32ed83adfbeedb75983ab6f9b5c4cc82714e9371c1895a1'),
                         ('visibility', '307d249868767c3c22bc6938c2024b93aada9c590a5b13c2a21165b042a30a66')]:
        p[name]['sha256'] = digest
    p['input_seals'].update({p[name]['path']: p[name]['sha256'] for name in M.PIN_FIELDS})
    p['input_seals'].update({str(ROOT / name): H for name in SOURCE_NAMES})
    return p


def build_fixture(p):
    receipt = {'schema': 'airk-readiness-command/1', 'status': 'completed', 'runner_exit_code': 0,
               'runtime_unchanged': True, 'changed_groups': [], 'passed': True, 'inputs_unchanged': True, 'runner_unchanged': True,
               'command_inputs_unchanged': True, 'exit_code': 0, 'evidence_errors': [],
               'command': ['cargo', 'test', '-p', 'airk-retrieval', '--lib', '--features', 'local-inference,test-support',
                           '--profile', 'release-local', '--no-run', '--message-format=json', '--locked', '--timings'],
               'log': Path(p['cargo_json']['path']).name,
               'groups': {'native': {'sha256': H, 'files': {name: H for name in SOURCE_NAMES}}}}
    artifact = {'reason': 'compiler-artifact', 'executable': p['image_origin']['path'],
                'target': {'name': 'airk_retrieval', 'kind': ['lib']},
                'features': ['default', 'local-inference', 'test-support'],
                'profile': {'test': True, 'opt_level': '3', 'debuginfo': 0,
                            'debug_assertions': False, 'overflow_checks': False}}
    return receipt, artifact


def cargo(artifact, success=True):
    return '\n'.join(json.dumps(x) for x in [artifact, {'reason': 'build-finished', 'success': success}])


def body_fixture(p):
    q, d = 'literal human question', 'literal reviewed function'
    qh, dh = (hashlib.sha256(x.encode()).hexdigest() for x in (q, d))
    original, replay, rows = [], [], []
    for i in range(2000):
        ident = str(i)
        role = 'TRAIN' if i < 1955 else 'REPLAY'
        pair = {'id': ident, 'label': 1, 'role': role, 'rights_scope': 'PERMITTED_PRIVATE_RESEARCH_SCOPE',
                'question': q, 'document': d, 'question_sha256': qh, 'document_sha256': dh}
        if i < 1955:
            original.append({'id': ident, 'label': 1, 'role': role, 'rights_review': pair['rights_scope'],
                             'question_sha256': qh, 'function_sha256': dh})
        else:
            replay.append({'id': ident, 'rights_review': pair['rights_scope'], 'question_sha256': qh, 'window_sha256': dh})
        rows.append({'pair': pair, 'publisher': {'kind': 'refused', 'reason': 'literal fit refusal'},
                     'software_evidence_v1': {'kind': 'refused', 'reason': 'literal fit refusal'}})
    body = {'schema': 'airk.private.focus-instruction-emission/1', 'model_requests_dispatched': False,
            'training_gradients': False, 'qualification': False, 'native_report': H, 'frozen_role_roster': H,
            'replay_roster': H, 'tokenizer': H, 'native_image': dict(p['image']),
            'native_focus_task_policy': H, 'focus_compiler_source_lf_sha256': F,
            'census': {'schema': 'airk.private.focus-instruction-census/1', 'qualification': False,
                       'model_requests_dispatched': False, 'original_denominator': 2000, 'rows': rows}}
    return body, original, replay


class FinalState:
    @staticmethod
    def final_state(receipt, started, deadline, clock):
        now = clock()
        receipt['elapsed_seconds'] = now - started
        receipt['whole_phase_expired'] = now >= deadline
        if now >= deadline:
            receipt['complete'] = False
            if receipt['failure'] is None:
                receipt['failure'] = {'type': 'TimeoutError'}


class CensusTests(unittest.TestCase):
    def test_fixed_plan(self):
        M.validate_plan(fixture_plan())
        self.assertNotIn('torch', sys.modules)

    def test_limits_selector_and_population_are_fixed(self):
        for field, value in [('selector', 'some_model_test'), ('original_code_rows', 1867), ('fixed_replay_rows', 44)]:
            p = fixture_plan(); p[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError): M.validate_plan(p)
        for bound in ['wall_seconds', 'host_committed_bytes', 'cleanup_seconds']:
            p = fixture_plan(); p['limits'][bound] += 1
            with self.subTest(bound=bound), self.assertRaises(ValueError): M.validate_plan(p)

    def test_helper_and_environment_are_closed(self):
        for edit in [lambda p: p['worker'].update(sha256=H),
                     lambda p: p['input_seals'].pop(p['image']['path']),
                     lambda p: p['environment'].update(CUDA_VISIBLE_DEVICES='0'),
                     lambda p: p.update(directory_censuses={'model': {}})]:
            p = fixture_plan(); edit(p)
            with self.assertRaises(ValueError): M.validate_plan(p)

    def test_genuine_cpu_build(self):
        p = fixture_plan(); r, a = build_fixture(p)
        result = M.validate_build(r, cargo(a), p)
        self.assertEqual(result['features'], ['default', 'local-inference', 'test-support'])

    def test_failed_stale_or_other_build_refused(self):
        for edit in [lambda r: r.update(schema='invented'), lambda r: r.update(status='running'),
                     lambda r: r.update(runner_exit_code=1), lambda r: r.update(runtime_unchanged=False),
                     lambda r: r.update(changed_groups=['native']), lambda r: r.update(passed=False), lambda r: r.update(inputs_unchanged=False),
                     lambda r: r.update(exit_code=1), lambda r: r['command'].__setitem__(3, 'airk-kernel'),
                     lambda r: r.update(log='unrelated.log')]:
            p = fixture_plan(); r, a = build_fixture(p); edit(r)
            with self.assertRaises(ValueError): M.validate_build(r, cargo(a), p)

    def test_gpu_wrong_profile_origin_and_zero_or_double_artifact_refused(self):
        for edit in [lambda a: a['features'].append('gpu-cuda'), lambda a: a['profile'].update(opt_level='0'),
                     lambda a: a.update(executable=str(ROOT / 'unrelated.exe')),
                     lambda a: a['target'].update(kind=['bin'])]:
            p = fixture_plan(); r, a = build_fixture(p); edit(a)
            with self.assertRaises(ValueError): M.validate_build(r, cargo(a), p)
        p = fixture_plan(); r, a = build_fixture(p)
        for text in [json.dumps({'reason': 'build-finished', 'success': True}), cargo(a) + '\n' + json.dumps(a), cargo(a, False)]:
            with self.assertRaises(ValueError): M.validate_build(r, text, p)

    def test_compiled_source_must_be_bound(self):
        p = fixture_plan(); r, a = build_fixture(p)
        r['groups']['native']['files'][SOURCE_NAMES[-1]] = F
        with self.assertRaises(ValueError): M.validate_build(r, cargo(a), p)

    def test_complete_2000_row_census_retains_all_refusals(self):
        p = fixture_plan(); b, r, replay = body_fixture(p)
        self.assertEqual(M.validate_body(b, p, r, replay), {
            'publisher': {'compiled': 0, 'refused': 2000}, 'software_evidence_v1': {'compiled': 0, 'refused': 2000}})

    def test_subsets_duplicates_and_human_label_changes_refused(self):
        for edit in [lambda b: b['census']['rows'].pop(),
                     lambda b: b['census']['rows'].__setitem__(1, copy.deepcopy(b['census']['rows'][0])),
                     lambda b: b['census']['rows'][0]['pair'].update(label=0),
                     lambda b: b['census']['rows'][-1]['pair'].update(document='changed'),
                     lambda b: b['census']['rows'][0]['publisher'].update(reason='')]:
            p = fixture_plan(); b, r, replay = body_fixture(p); edit(b)
            with self.assertRaises(ValueError): M.validate_body(b, p, r, replay)

    def test_model_authority_policy_or_image_changes_refused(self):
        for edit in [lambda b: b.update(qualification=True), lambda b: b.update(model_requests_dispatched=True),
                     lambda b: b.update(training_gradients=True), lambda b: b.pop('native_focus_task_policy'),
                     lambda b: b.update(focus_compiler_source_lf_sha256=H),
                     lambda b: b['native_image'].update(sha256=F)]:
            p = fixture_plan(); b, r, replay = body_fixture(p); edit(b)
            with self.assertRaises(ValueError): M.validate_body(b, p, r, replay)

    def test_literal_native_512_readout(self):
        p = fixture_plan(); b, r, replay = body_fixture(p)
        value = {'input_ids': [0] * 512, 'attention_mask': [1] * 512, 'position_ids': list(range(512)),
                 'candidate_positions': [2152, 9693], 'option_mask': [True, True], 'decision_position': 511}
        b['census']['rows'][0]['publisher'] = {'kind': 'compiled', 'input': value}
        self.assertEqual(M.validate_body(b, p, r, replay)['publisher']['compiled'], 1)
        for name, wrong in [('input_ids', [0] * 511), ('candidate_positions', [9693, 2152]), ('decision_position', 510)]:
            previous = value[name]; value[name] = wrong
            with self.assertRaises(ValueError): M.validate_body(b, p, r, replay)
            value[name] = previous

    def test_output_traversal_and_outside_private_root_refused(self):
        private = ROOT / 'artifacts/v190/final-plan'
        output = private / 'never-created-census-test-output'
        self.assertEqual(M.fresh_output(output, private / 'plan.json', ROOT), output)
        for output in [private / '../outside/run', ROOT / 'artifacts/outside/run']:
            with self.assertRaises(ValueError): M.fresh_output(output, output.parent / 'plan.json', ROOT)

    def test_interruption_after_complete_never_passes(self):
        receipt = {'complete': True, 'failure': None, 'owned': {'exit_code': 0}}
        error = KeyboardInterrupt('literal parent interruption after valid child')
        M.record_failure(receipt, error)
        self.assertFalse(receipt['complete'])
        self.assertEqual(receipt['failure']['type'], 'KeyboardInterrupt')
        self.assertEqual(receipt['owned']['exit_code'], 0)

    def test_final_clock_follows_hash_and_original_failure_is_retained(self):
        events = []
        class Visibility(FinalState):
            @staticmethod
            def finish_proofs(receipt, started, deadline, proofs, hash_file, clock):
                for _, path in proofs: hash_file(path)
                now = clock(); events.append('clock')
                receipt['elapsed_seconds'] = now - started
                if now >= deadline:
                    receipt['complete'] = False
                    if receipt['failure'] is None: receipt['failure'] = {'type': 'TimeoutError'}
        def slow_hash(path): events.append('hash'); return H
        receipt = {'complete': True, 'failure': {'type': 'OriginalOwnedFailure'}}
        M.finish(receipt, 0, 120, [('log', Path(__file__))], Visibility, hash_file=slow_hash, clock=lambda: 120)
        self.assertEqual(events, ['hash', 'clock'])
        self.assertFalse(receipt['complete'])
        self.assertEqual(receipt['failure']['type'], 'OriginalOwnedFailure')
        self.assertEqual(receipt['elapsed_seconds'], 120)

    def test_missing_or_nonregular_mandatory_proof_cannot_be_skipped(self):
        class SkipVisibility(FinalState):
            @staticmethod
            def finish_proofs(receipt, started, deadline, proofs, hash_file, clock):
                # This is the maintained helper's legitimate skip behavior.
                for label, path in proofs:
                    if path.is_file(): receipt[label] = hash_file(path)
                clock()
        for path in [ROOT / 'artifacts/never-created-mandatory-proof.json', ROOT / 'scripts']:
            for failure in [None, {'type': 'OriginalOwnedFailure'}]:
                receipt = {'complete': True, 'failure': failure}
                M.finish(receipt, 0, 120, [('native_report', path)], SkipVisibility, hash_file=lambda _: H, clock=lambda: 1)
                self.assertFalse(receipt['complete'])
                self.assertEqual(len(receipt['final_hash_errors']), 1)
                self.assertEqual(receipt['failure']['type'], 'FinalProofError' if failure is None else 'OriginalOwnedFailure')

    def test_hash_failure_forces_nonpass_without_erasing_owned_failure(self):
        class Visibility(FinalState):
            @staticmethod
            def finish_proofs(receipt, started, deadline, proofs, hash_file, clock):
                hash_file(proofs[0][1]); clock()
        def bad_hash(path): raise OSError('literal source unavailable')
        for failure in [None, {'type': 'OriginalOwnedFailure'}]:
            receipt = {'complete': True, 'failure': failure}
            M.finish(receipt, 0, 120, [('log', Path(__file__))], Visibility, hash_file=bad_hash, clock=lambda: 2)
            self.assertFalse(receipt['complete'])
            self.assertEqual(len(receipt['final_hash_errors']), 1)
            self.assertEqual(receipt['failure']['type'], 'FinalHashError' if failure is None else 'OriginalOwnedFailure')



    def test_proof_skipped_after_regular_precheck_cannot_reuse_stale_sha_or_deadline(self):
        path = Path(__file__)
        for original_failure in (None, {'type': 'OriginalOwnedFailure'}):
            hash_calls, states, bounds, clock_reads = [], [], [], []
            readings = iter((129, 131))
            def clock():
                value = next(readings)
                clock_reads.append(value)
                return value
            class DisappearingVisibility:
                @staticmethod
                def finish_proofs(receipt, started, deadline, proofs, hash_file, clock):
                    # Actual regular() already passed. Model the pinned helper's
                    # legitimate skip if the file disappears immediately after.
                    with mock.patch.object(Path, 'is_file', return_value=False):
                        for label, proof in proofs:
                            if proof.is_file():
                                receipt[label] = {'path': str(proof), 'bytes': 1, 'sha256': hash_file(proof)}
                    DisappearingVisibility.final_state(receipt, started, deadline, clock)
                @staticmethod
                def final_state(receipt, started, deadline, clock):
                    states.append(receipt['complete'])
                    bounds.append((started, deadline))
                    FinalState.final_state(receipt, started, deadline, clock)
            # A previous, non-null SHA row must not substitute for THIS final
            # hash callback. No source file is actually removed by this test.
            receipt = {'complete': True, 'failure': original_failure,
                       'native_report': {'path': str(path), 'bytes': path.stat().st_size, 'sha256': H}}
            def actual_hash(proof):
                hash_calls.append(proof)
                return H
            with mock.patch.object(M, 'regular', wraps=M.regular) as precheck:
                M.finish(receipt, 10, 130, [('native_report', path)], DisappearingVisibility,
                         hash_file=actual_hash, clock=clock)
            precheck.assert_called_once_with(path)
            self.assertEqual(hash_calls, [])
            self.assertEqual(bounds, [(10, 130), (10, 130)])
            self.assertEqual(clock_reads, [129, 131])
            self.assertEqual(states, [original_failure is None, False])
            self.assertFalse(receipt['complete'])
            self.assertTrue(receipt['whole_phase_expired'])
            self.assertEqual(receipt['elapsed_seconds'], 121)
            self.assertEqual(receipt['failure']['type'], 'FinalProofError' if original_failure is None else 'OriginalOwnedFailure')
            self.assertEqual(len(receipt['final_hash_errors']), 1)


    def test_each_required_label_needs_its_own_actual_hash_callback(self):
        path = Path(__file__)
        for skip_second in (False, True):
            calls = []
            class Visibility(FinalState):
                @staticmethod
                def finish_proofs(receipt, started, deadline, proofs, hash_file, clock):
                    for index, (label, proof) in enumerate(proofs):
                        # Even identical paths require a callback for each label.
                        sha = H if skip_second and index == 1 else hash_file(proof)
                        receipt[label] = {'path': str(proof), 'bytes': proof.stat().st_size, 'sha256': sha}
                    FinalState.final_state(receipt, started, deadline, clock)
            receipt = {'complete': True, 'failure': None}
            def observed(proof):
                calls.append(proof)
                return H
            M.finish(receipt, 0, 120, [('native_report', path), ('log', path)], Visibility,
                     hash_file=observed, clock=lambda: 1)
            self.assertEqual(len(calls), 1 if skip_second else 2)
            self.assertEqual(receipt['complete'], not skip_second)
            self.assertEqual(len(receipt['final_hash_errors']), 1 if skip_second else 0)
            if not skip_second:
                self.assertIsNone(receipt['failure'])
                self.assertEqual(receipt['log']['sha256'], H)

if __name__ == '__main__': unittest.main()
