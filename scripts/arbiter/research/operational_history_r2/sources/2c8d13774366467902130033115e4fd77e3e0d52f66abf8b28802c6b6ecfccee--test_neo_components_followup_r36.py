# SPDX-License-Identifier: Apache-2.0
"""Cheap orchestration regressions; never execute a native/Node/SSH command."""
import importlib.util
import ast
import copy
import hashlib
import json
from pathlib import Path, PurePosixPath
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

PREPARATION = Path(__file__).resolve().parent


def load_owner(name):
    path = PREPARATION / name
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ColdCheckoutOrderTests(unittest.TestCase):
    def test_actual_component_fixture_selection_fits_darwin_assistance_socket(self):
        # Read the actual existing orchestration assignment, rather than inventing
        # a replacement plan. This first runs red before a Node-only follow-up.
        tree = ast.parse((PREPARATION / 'neo-components-followup-r36.py').read_text())
        main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main')
        fixture = next(node.value for node in ast.walk(main) if isinstance(node, ast.Assign)
                       and any(isinstance(target, ast.Name) and target.id == 'fixtures' for target in node.targets))
        self.assertIsInstance(fixture, ast.BinOp)
        self.assertEqual(ast.unparse(fixture.left), 'namespace')
        self.assertIsInstance(fixture.op, ast.Div)
        self.assertEqual(fixture.right.value, 'fixtures')
        namespace = PurePosixPath('/Users/tylerjensen/airk-builds') / FOLLOWUP.APPROVED['campaign'] / 'components-followup-r36-1794219736000000000'
        base = namespace / fixture.right.value
        # explorerAssistanceHost.ts uses mkdtemp(...,'airk-explorer-') (six
        # generated characters), then the literal airk-explorer.sock leaf.
        endpoint = base / 'airk-explorer-XXXXXX' / 'airk-explorer.sock'
        self.assertGreater(len(str(endpoint).encode('utf-8')) + 1, 104)
        with self.assertRaisesRegex(ValueError, 'sun_path budget'):
            NODE.socket_budget(base)
        proof = NODE.validate_short_root('/private/tmp/airk-n36-abcdefgh',
            '/private/tmp/airk-n36-abcdefgh', 501, 501, 0o700, False)
        self.assertLessEqual(proof['bytes_including_nul'], 104)

    def test_extension_compile_precedes_python_real_consumer_on_cold_checkout(self):
        # This assertion first failed against the immutable original owner's
        # actual gate list. The corrected component plan retains that evidence.
        owner = load_owner('neo-components-followup-r36.py')
        self.assertLess(
            owner.REQUIRED_GATES.index('extension-full-compile'),
            owner.REQUIRED_GATES.index('full-python-contracts'),
            'Python real-consumer contracts need compiled packages/vscode/out/model.js first',
        )


FOLLOWUP = load_owner('neo-components-followup-r36.py')
NODE = load_owner('neo-node-followup-r36.py')


def fake_prior(base):
    campaign = base / 'campaign'
    campaign.mkdir()
    source = campaign / 'source'
    source.mkdir()
    (source / 'public.txt').write_bytes(b'public frozen fixture')
    models = campaign / 'offline-model'
    models.mkdir()
    (models / 'model.bin').write_bytes(b'fictional model fixture')
    original_runner = campaign / 'neo-scoped-owner.py'
    original_runner.write_bytes(b'# inert original owner fixture\n')
    rows = [{'path': 'public.txt', 'bytes': (source / 'public.txt').stat().st_size,
             'sha256': FOLLOWUP.sha(source / 'public.txt')}]
    model_rows = [{'path': 'model.bin', 'bytes': (models / 'model.bin').stat().st_size,
                   'sha256': FOLLOWUP.sha(models / 'model.bin')}]
    approved = copy.deepcopy(FOLLOWUP.APPROVED)
    approved.update(campaign=campaign.name, source_file_count=1,
        source_roster_sha256=hashlib.sha256(FOLLOWUP.canonical(rows)).hexdigest(),
        models_roster_sha256=hashlib.sha256(FOLLOWUP.canonical(model_rows)).hexdigest(),
        original_runner_sha256=FOLLOWUP.sha(original_runner))
    manifest = {'status': 'FROZEN_PUBLIC_SOURCE', 'final_freeze_required': False,
        'native_source': approved['native'], 'source_roster_sha256': approved['source_roster_sha256'],
        'file_count': 1, 'files': rows, 'public_models_roster_sha256': approved['models_roster_sha256']}
    (campaign / 'source-manifest.json').write_text(json.dumps(manifest))
    approved['source_manifest_sha256'] = FOLLOWUP.sha(campaign / 'source-manifest.json')
    (campaign / 'public-model-transfer-manifest.json').write_text(json.dumps({
        'files': model_rows, 'model_roster_sha256': approved['models_roster_sha256']}))
    evidence = campaign / 'evidence'
    evidence.mkdir()
    junit = evidence / 'nextest-store/ci/junit.xml'
    junit.parent.mkdir(parents=True)
    junit.write_text('<testsuite>' + '<testcase name="native" classname="fixture" />' * 393 + '</testsuite>')
    timing = evidence / 'cargo-timings/retained.html'
    timing.parent.mkdir()
    timing.write_bytes(b'original retained timing fixture')
    approved['logs'] = {}
    for name in FOLLOWUP.APPROVED['logs']:
        (evidence / name).write_text('original immutable log ' + name)
        approved['logs'][name] = FOLLOWUP.sha(evidence / name)
    identity = {'native': approved['native'], 'public_roster_sha256': approved['source_roster_sha256']}
    commands = []
    for stage in ['scoped-four-crate-clippy', 'focused-clean-break-native-tests',
                  'full-python-contracts', 'required-neo-cargo-clean']:
        failed = stage == 'full-python-contracts'
        commands.append({'stage': stage, 'result': 'failed' if failed else 'passed',
            'exit_code': 1 if failed else 0, 'owned_command_started': True,
            'owned_descendants_settled': True, 'timed_out': False, 'log_oversized': False,
            'frozen_inputs_before': identity, 'frozen_inputs_after': identity})
    commands[1].update(observed_test_counts={'selected': 393, 'executed': 393, 'skipped': 0, 'failures': 0, 'errors': 0},
        test_counts={'selected': 393, 'executed': 393, 'skipped': 0,
                     'cases': [{'name': f'native-{i}', 'classname': 'fixture', 'skipped': False} for i in range(393)]},
        junit={'path': str(junit), 'bytes': junit.stat().st_size, 'sha256': FOLLOWUP.sha(junit)})
    receipt = {'schema': 1, 'result': 'failed', 'qualification_complete': False,
        'source': str(source), 'compiler_target': str(campaign / 'cargo-target'),
        'fixtures': str(campaign / 'fixtures'), 'source_identity': approved['native'],
        'source_roster_sha256': approved['source_roster_sha256'],
        'public_models_roster_sha256': approved['models_roster_sha256'],
        'source_manifest_sha256': approved['source_manifest_sha256'],
        'runner_sha256': approved['original_runner_sha256'], 'cargo_cleanup': 'completed',
        'evidence_preserved_before_clean': True, 'all_owned_command_descendants_settled': True,
        'compiler_tree_absent_or_empty': True, 'commands': commands,
        'retained_cargo_timings': [{'path': str(timing), 'bytes': timing.stat().st_size, 'sha256': FOLLOWUP.sha(timing)}]}
    rewrite_prior(campaign, approved, receipt)
    return campaign, approved, receipt


def rewrite_prior(campaign, approved, receipt):
    path = campaign / 'evidence/batch-receipt.json'
    path.write_text(json.dumps(receipt))
    approved['prior_sha256'] = FOLLOWUP.sha(path)


class PriorQualificationTests(unittest.TestCase):
    def setUp(self):
        self.owner = tempfile.TemporaryDirectory()
        self.base = Path(self.owner.name).resolve()
        self.campaign, self.approved, self.receipt = fake_prior(self.base)

    def tearDown(self):
        self.owner.cleanup()

    def validate(self):
        return FOLLOWUP.validate_prior(self.campaign, self.approved)

    def test_prior_failed_batch_retains_actual_393_native_pass_and_cleanup(self):
        observed = self.validate()
        self.assertEqual(observed['result'], 'failed')
        self.assertEqual(observed['commands'][1]['test_counts']['executed'], 393)
        FOLLOWUP.validate_frozen(self.campaign, observed, lambda: self.approved['native'], self.approved)

    def test_changed_prior_receipt_is_rejected_before_relabeling(self):
        path = self.campaign / 'evidence/batch-receipt.json'
        path.write_bytes(path.read_bytes() + b'\n')
        with self.assertRaisesRegex(ValueError, 'receipt identity'):
            self.validate()

    def test_crossed_source_namespace_is_rejected_even_with_authenticated_receipt(self):
        self.receipt['source'] = str(self.base / 'other-source')
        rewrite_prior(self.campaign, self.approved, self.receipt)
        with self.assertRaisesRegex(ValueError, 'scope is not proven'):
            self.validate()

    def test_zero_native_selection_never_reuses_qualification(self):
        self.receipt['commands'][1]['observed_test_counts']['selected'] = 0
        rewrite_prior(self.campaign, self.approved, self.receipt)
        with self.assertRaisesRegex(ValueError, 'selected count'):
            self.validate()

    def test_unsettled_old_descendants_and_uncleaned_target_refuse_followup(self):
        for field, value in [('all_owned_command_descendants_settled', False), ('cargo_cleanup', 'failed')]:
            with self.subTest(field=field):
                original = self.receipt[field]
                self.receipt[field] = value
                rewrite_prior(self.campaign, self.approved, self.receipt)
                with self.assertRaises(ValueError):
                    self.validate()
                self.receipt[field] = original
        rewrite_prior(self.campaign, self.approved, self.receipt)
        (self.campaign / 'cargo-target').mkdir()
        (self.campaign / 'cargo-target/unsettled').write_bytes(b'preserve')
        with self.assertRaisesRegex(ValueError, 'not cleaned'):
            self.validate()
        self.assertEqual((self.campaign / 'cargo-target/unsettled').read_bytes(), b'preserve')

    def test_changed_original_junit_and_log_are_refused(self):
        junit = self.campaign / 'evidence/nextest-store/ci/junit.xml'
        original = junit.read_bytes()
        junit.write_bytes(b'<testsuite/>')
        with self.assertRaisesRegex(ValueError, 'JUnit proof changed'):
            self.validate()
        junit.write_bytes(original)
        log = self.campaign / 'evidence/scoped-four-crate-clippy.log'
        log.write_text('changed')
        with self.assertRaisesRegex(ValueError, 'stage log changed'):
            self.validate()

    def test_changed_frozen_public_model_and_native_inputs_are_refused(self):
        public = self.campaign / 'source/public.txt'
        original = public.read_bytes()
        public.write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError, 'Frozen input changed'):
            FOLLOWUP.validate_frozen(self.campaign, self.receipt, lambda: self.approved['native'], self.approved)
        public.write_bytes(original)
        with self.assertRaisesRegex(ValueError, 'Frozen native inputs changed'):
            FOLLOWUP.validate_frozen(self.campaign, self.receipt, lambda: {'sha256': 'changed'}, self.approved)
        (self.campaign / 'offline-model/model.bin').write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError, 'Frozen input changed'):
            FOLLOWUP.validate_frozen(self.campaign, self.receipt, lambda: self.approved['native'], self.approved)

    def test_manifest_change_and_traversal_are_refused(self):
        manifest = self.campaign / 'source-manifest.json'
        manifest.write_bytes(manifest.read_bytes() + b'\n')
        with self.assertRaisesRegex(ValueError, 'manifest changed'):
            FOLLOWUP.validate_frozen(self.campaign, self.receipt, lambda: self.approved['native'], self.approved)
        with self.assertRaises(ValueError):
            FOLLOWUP.relative(self.campaign / 'source', '../evidence/batch-receipt.json')

    def test_fresh_namespace_never_reuses_old_fixtures_or_output(self):
        for name in ['fixtures', 'evidence', '../outside', 'components-followup-r35-1']:
            with self.assertRaises(ValueError):
                FOLLOWUP.create_namespace(self.campaign, name)
        namespace = FOLLOWUP.create_namespace(self.campaign, 'components-followup-r36-1')
        (namespace / 'preserve').write_bytes(b'owned original')
        with self.assertRaises(FileExistsError):
            FOLLOWUP.create_namespace(self.campaign, 'components-followup-r36-1')
        self.assertEqual((namespace / 'preserve').read_bytes(), b'owned original')


class ComponentPlanTests(unittest.TestCase):
    def execute(self, base, failure=None, changed_output=False):
        source = base / 'source'
        source.mkdir()
        namespace = base / 'components-followup-r36-1'
        namespace.mkdir()
        state = {'commands': []}
        calls = []
        def run(argv, cwd, log, timeout, environment):
            calls.append(list(argv))
            self.assertTrue((source / 'artifacts').is_dir())
            if argv[:3] == ['npm', 'run', 'compile']:
                (source / 'packages/vscode/out/test').mkdir(parents=True)
                (source / 'packages/vscode/out/model.js').write_text('compiled model fixture')
                (source / 'packages/vscode/out/test/example.test.js').write_text('fixture test')
            if '-m' in argv:
                self.assertTrue((source / 'packages/vscode/out/model.js').is_file())
                log.write_text('Ran 919 tests in 0.1s\n\nOK (skipped=22)\n')
                if changed_output:
                    (source / 'packages/vscode/out/model.js').write_text('changed compiled model')
            elif argv[0] == 'node':
                log.write_text('# tests 3\n# pass 3\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n')
            else:
                log.write_text('owned mock command\n')
            return {'exit_code': 1 if failure and failure in argv else 0, 'timed_out': False,
                    'log_oversized': False, 'owned_descendants_settled': True}
        owner = SimpleNamespace(run_owned=run, successful=lambda row: row['exit_code'] == 0 and row['owned_descendants_settled'])
        verify = lambda: {'native': 'unchanged', 'models': 'unchanged'}
        return source, namespace, owner, verify, state, calls

    def test_cold_checkout_creates_artifacts_and_compiles_before_python_without_cargo(self):
        with tempfile.TemporaryDirectory() as directory:
            args = self.execute(Path(directory).resolve())
            source, namespace, owner, verify, state, calls = args
            FOLLOWUP.run_components(source, namespace, owner, {}, verify, state, lambda: None)
            self.assertEqual([row['stage'] for row in state['commands']], FOLLOWUP.REQUIRED_GATES)
            self.assertEqual([argv[0] for argv in calls], ['npm', 'npm', FOLLOWUP.sys.executable, 'node'])
            self.assertTrue(all(row['result'] == 'passed' for row in state['commands']))
            self.assertEqual(state['commands'][3]['test_counts']['selected'], 919)
            self.assertEqual(state['commands'][4]['test_counts']['pass'], 3)

    def test_first_failed_python_stops_before_mocked_node_without_retry(self):
        with tempfile.TemporaryDirectory() as directory:
            source, namespace, owner, verify, state, calls = self.execute(Path(directory).resolve(), failure='unittest')
            with self.assertRaisesRegex(ValueError, 'command failed'):
                FOLLOWUP.run_components(source, namespace, owner, {}, verify, state, lambda: None)
            self.assertEqual(len(calls), 3)
            self.assertEqual(state['commands'][-1]['result'], 'failed')

    def test_changed_inputs_before_prepare_never_create_artifacts_or_run_commands(self):
        with tempfile.TemporaryDirectory() as directory:
            source, namespace, owner, verify, state, calls = self.execute(Path(directory).resolve())
            def changed():
                raise ValueError('frozen input changed')
            with self.assertRaisesRegex(ValueError, 'frozen input changed'):
                FOLLOWUP.run_components(source, namespace, owner, {}, changed, state, lambda: None)
            self.assertFalse((source / 'artifacts').exists())
            self.assertEqual(calls, [])

    def test_changed_compiled_input_stops_before_node(self):
        with tempfile.TemporaryDirectory() as directory:
            source, namespace, owner, verify, state, calls = self.execute(Path(directory).resolve(), changed_output=True)
            with self.assertRaisesRegex(ValueError, 'Compiled extension inputs changed'):
                FOLLOWUP.run_components(source, namespace, owner, {}, verify, state, lambda: None)
            self.assertEqual(len(calls), 3)

    def test_unsettled_owned_command_stops_before_any_later_stage(self):
        with tempfile.TemporaryDirectory() as directory:
            source, namespace, owner, verify, state, calls = self.execute(Path(directory).resolve())
            original = owner.run_owned
            def unsettled(*args):
                result = original(*args)
                result['owned_descendants_settled'] = False
                return result
            owner.run_owned = unsettled
            with self.assertRaisesRegex(ValueError, 'descendants unsettled'):
                FOLLOWUP.run_components(source, namespace, owner, {}, verify, state, lambda: None)
            self.assertEqual(len(calls), 1)
            self.assertFalse(state['commands'][-1]['owned_descendants_settled'])

    def test_existing_artifact_file_is_not_replaced_or_hardened(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory).resolve()
            (source / 'artifacts').write_bytes(b'preserve original')
            with self.assertRaises(ValueError):
                FOLLOWUP.prepare_artifacts(source)
            self.assertEqual((source / 'artifacts').read_bytes(), b'preserve original')

    def test_zero_missing_failed_or_cancelled_counts_are_rejected(self):
        for text in ['Ran 0 tests in 0.1s\nOK\n', 'Ran 919 tests in 0.1s\nFAILED (failures=1)\n', 'OK\n']:
            with self.assertRaises(ValueError):
                FOLLOWUP.python_counts(text)
        for values in [(0, 0, 0, 0), (3, 2, 1, 0), (3, 2, 0, 1)]:
            tests, passed, failed, cancelled = values
            text = f'# tests {tests}\n# pass {passed}\n# fail {failed}\n# cancelled {cancelled}\n# skipped 0\n# todo 0\n'
            with self.assertRaises(ValueError):
                FOLLOWUP.node_counts(text)


def node_log(passed=1137, failed=0, skipped=33):
    return f'# tests 1170\n# pass {passed}\n# fail {failed}\n# cancelled 0\n# skipped {skipped}\n# todo 0\n'


def fake_components(base):
    campaign = base / 'campaign'
    campaign.mkdir()
    namespace = campaign / 'components-followup-r36-1'
    namespace.mkdir()
    approved = copy.deepcopy(FOLLOWUP.APPROVED)
    expected = copy.deepcopy(NODE.EXPECTED)
    expected['component'] = namespace.name
    compiled = {'sha256': 'f' * 64, 'files': [{'path': 'model.js'}, {'path': 'test/example.test.js'}],
                'test_files': [str(campaign / 'source/packages/vscode/out/test/example.test.js')]}
    expected.update(compiled_sha256=compiled['sha256'], compiled_files=2, compiled_test_files=1)
    frozen = {'native': approved['native'], 'source_roster_sha256': approved['source_roster_sha256'],
              'public_models_roster_sha256': approved['models_roster_sha256']}
    rows = [{'stage': FOLLOWUP.REQUIRED_GATES[0], 'result': 'passed'}]
    for position, stage in enumerate(FOLLOWUP.REQUIRED_GATES[1:], 1):
        rows.append({'stage': stage, 'result': 'failed' if position == 4 else 'passed',
            'exit_code': 1 if position == 4 else 0, 'owned_command_started': True,
            'owned_descendants_settled': True, 'timed_out': False, 'log_oversized': False,
            'frozen_inputs_before': frozen, 'frozen_inputs_after': frozen})
    rows[3]['test_counts'] = {'selected': 919, 'skipped': 19, 'executed': 900}
    rows[4]['argv'] = ['node', '--test', '--test-concurrency=2', *compiled['test_files']]
    expected['logs'] = {}
    for name in NODE.EXPECTED['logs']:
        text = 'prior command log\n'
        if name == 'full-python-contracts.log':
            text = 'Ran 919 tests in 1.0s\n\nOK (skipped=19)\n'
        if name == 'full-mocked-extension-tests.log':
            text = node_log(1113, 24)
        (namespace / name).write_text(text, encoding='utf-8')
        expected['logs'][name] = FOLLOWUP.sha(namespace / name)
    receipt = {'schema': 1, 'result': 'failed', 'qualification_complete': False,
        'all_owned_command_descendants_settled': True, 'namespace': str(namespace),
        'source': str(campaign / 'source'), 'runner_sha256': expected['runner_sha256'],
        'original_batch_receipt': str(campaign / 'evidence/batch-receipt.json'),
        'original_batch_sha256': approved['prior_sha256'], 'original_batch_result': 'failed',
        'previous_native_qualification': {'native_source': approved['native'], 'native_tests': 393},
        'commands': rows, 'compiled_extension_inputs': compiled}
    path = namespace / 'component-receipt.json'
    path.write_text(json.dumps(receipt))
    expected['receipt_sha256'] = FOLLOWUP.sha(path)
    component = SimpleNamespace(APPROVED=approved, REQUIRED_GATES=FOLLOWUP.REQUIRED_GATES,
        plain=FOLLOWUP.plain, read_json=FOLLOWUP.read_json, python_counts=FOLLOWUP.python_counts,
        compiled_inputs=lambda _: compiled)
    return campaign, component, expected, receipt


class NodeOnlyFollowupTests(unittest.TestCase):
    def test_actual_main_helper_load_matches_the_retained_launch_handoff(self):
        # Exact collected-r36-components-first/launch.json argv[2], bound there
        # to helper8046...; never invent a campaign-root copy of that helper.
        campaign = PurePosixPath('/Users/tylerjensen/airk-builds') / NODE.CAMPAIGN
        launched = campaign / 'components-control-r36-1794219736000000000/neo-components-followup-r36.py'
        tree = ast.parse((PREPARATION / 'neo-node-followup-r36.py').read_text())
        main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main')
        call = next(node.value for node in ast.walk(main) if isinstance(node, ast.Assign)
                    and any(isinstance(target, ast.Name) and target.id == 'component' for target in node.targets))
        self.assertEqual(ast.unparse(call.func), 'load_verified')
        selected = call.args[0]
        self.assertIsInstance(selected, ast.Call)
        self.assertEqual(ast.unparse(selected.func), 'component_helper_path')
        self.assertEqual(ast.unparse(selected.args[0]), 'campaign')
        actual = NODE.component_helper_path(campaign)
        self.assertEqual(actual, launched, 'Actual main must borrow the exact already-executed helper path')
        self.assertEqual(sum(isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
            and node.func.id == 'component_helper_path' for node in ast.walk(main)), 2,
            'Both initial load and later identity verification must use the same exact handoff')

    def test_helper_handoff_never_adopts_matching_bytes_from_another_control(self):
        with tempfile.TemporaryDirectory() as directory:
            campaign = Path(directory).resolve()
            foreign = campaign / 'foreign-control/neo-components-followup-r36.py'
            foreign.parent.mkdir()
            foreign.write_text('SENTINEL = 1\n')
            with self.assertRaisesRegex(ValueError, 'helper identity changed'):
                NODE.load_verified(NODE.component_helper_path(campaign), FOLLOWUP.sha(foreign), 'rejected_foreign_control')
            self.assertTrue(foreign.is_file())

    def test_helper_handoff_rejects_a_redirected_control_parent_before_import(self):
        with tempfile.TemporaryDirectory() as directory:
            helper = NODE.component_helper_path(Path(directory).resolve())
            helper.parent.mkdir()
            helper.write_text('SENTINEL = 1\n')
            # Inject the filesystem observer so Windows also checks the Unix
            # redirected-parent decision without creating a privileged symlink.
            with patch.object(Path, 'is_symlink', autospec=True,
                              side_effect=lambda item: item == helper.parent):
                with self.assertRaisesRegex(ValueError, 'helper identity changed'):
                    NODE.load_verified(helper, FOLLOWUP.sha(helper), 'rejected_redirect')

    def test_helper_handoff_rejects_wrong_digest_without_replacing_the_file(self):
        with tempfile.TemporaryDirectory() as directory:
            helper = NODE.component_helper_path(Path(directory).resolve())
            helper.parent.mkdir()
            helper.write_text('SENTINEL = 1\n')
            before = helper.read_bytes()
            with self.assertRaisesRegex(ValueError, 'helper identity changed'):
                NODE.load_verified(helper, '0' * 64, 'rejected_changed_helper')
            self.assertEqual(helper.read_bytes(), before)

    def test_short_root_budget_covers_shared_host_onboarding_and_trust_factory(self):
        proof = NODE.validate_short_root('/private/tmp/airk-n36-abcdefgh',
            '/private/tmp/airk-n36-abcdefgh', 501, 501, 0o700, False)
        self.assertEqual(set(proof['shared_factory_consumers']),
            {'explorerAssistanceHost', 'explorerAssistanceTools', 'serviceOnboarding', 'trust'})
        self.assertLessEqual(proof['bytes_including_nul'], 104)

    def test_darwin_socket_budget_counts_unicode_bytes_and_nul(self):
        base = '/private/tmp/' + 'é' * 30
        self.assertLess(len(base + '/airk-explorer-XXXXXX/airk-explorer.sock'), 104)
        with self.assertRaisesRegex(ValueError, 'sun_path budget'):
            NODE.socket_budget(base)

    def test_private_root_refuses_redirect_foreign_owner_public_mode_and_old_namespace(self):
        good = ['/private/tmp/airk-n36-abcdefgh', '/private/tmp/airk-n36-abcdefgh', 501, 501, 0o700, False]
        for position, value in [(1, '/private/tmp/other'), (2, 502), (4, 0o755), (5, True),
                                (0, '/private/tmp/old-failed-fixtures')]:
            with self.subTest(position=position, value=value):
                bad = good.copy()
                bad[position] = value
                with self.assertRaises(ValueError):
                    NODE.validate_short_root(*bad)

    def test_prior_actual_python_compile_and_complete_generated_roster_are_required(self):
        with tempfile.TemporaryDirectory() as directory:
            campaign, component, expected, receipt = fake_components(Path(directory).resolve())
            self.assertEqual(NODE.validate_components(campaign, component, expected), receipt)
            component.compiled_inputs = lambda _: {'changed': True}
            with self.assertRaisesRegex(ValueError, 'compiled JavaScript'):
                NODE.validate_components(campaign, component, expected)

    def test_changed_component_receipt_or_borrowed_source_scope_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            campaign, component, expected, receipt = fake_components(Path(directory).resolve())
            path = campaign / expected['component'] / 'component-receipt.json'
            path.write_bytes(path.read_bytes() + b'\n')
            with self.assertRaisesRegex(ValueError, 'receipt changed'):
                NODE.validate_components(campaign, component, expected)
            receipt['source'] = str(campaign / 'foreign-source')
            path.write_text(json.dumps(receipt))
            expected['receipt_sha256'] = FOLLOWUP.sha(path)
            with self.assertRaisesRegex(ValueError, 'scope crossed'):
                NODE.validate_components(campaign, component, expected)

    def test_prior_python_zero_and_selected_node_subset_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            campaign, component, expected, receipt = fake_components(Path(directory).resolve())
            path = campaign / expected['component'] / 'component-receipt.json'
            receipt['commands'][3]['test_counts']['selected'] = 0
            path.write_text(json.dumps(receipt))
            expected['receipt_sha256'] = FOLLOWUP.sha(path)
            with self.assertRaisesRegex(ValueError, 'Python 919'):
                NODE.validate_components(campaign, component, expected)
            receipt['commands'][3]['test_counts']['selected'] = 919
            receipt['commands'][4]['argv'] = ['node', '--test', '--test-concurrency=2']
            path.write_text(json.dumps(receipt))
            expected['receipt_sha256'] = FOLLOWUP.sha(path)
            with self.assertRaisesRegex(ValueError, 'complete test roster'):
                NODE.validate_components(campaign, component, expected)

    def test_full_node_counts_cannot_relabel_24_failures_as_skips(self):
        self.assertEqual(NODE.require_node_pass(node_log())['pass'], 1137)
        for text in [node_log(1113, 24), node_log(1113, 0, 57), '# tests 0\n# pass 0\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n']:
            with self.assertRaises(ValueError):
                NODE.require_node_pass(text)

    def execute(self, base, invalid=False, changed=False, failed=False):
        calls, saved = [], []
        borrowed = {'commands': [{}, {}, {}, {}, {'argv': ['node', '--test', '--test-concurrency=2', 'complete.test.js']}],
                    'compiled_extension_inputs': {'test_files': ['complete.test.js']}}
        def observe(_):
            if invalid:
                raise ValueError('invalid short root')
            return NODE.socket_budget('/private/tmp/airk-n36-abcdefgh')
        def verify():
            if changed:
                raise ValueError('frozen source changed')
        def run(argv, cwd, log, timeout, environment):
            calls.append((argv, environment))
            log.write_text(node_log(1113, 24) if failed else node_log())
            return {'exit_code': 1 if failed else 0, 'timed_out': False,
                    'log_oversized': False, 'owned_descendants_settled': True}
        owner = SimpleNamespace(run_owned=run, successful=lambda row: row['exit_code'] == 0 and row['owned_descendants_settled'])
        def action():
            return NODE.run_node(base, base, Path('/private/tmp/airk-n36-abcdefgh'), None,
                borrowed, owner, {}, verify, lambda row: saved.append(dict(row)), observe)
        return action, calls, saved

    def test_no_node_action_on_invalid_root_or_changed_frozen_inputs(self):
        for invalid, changed in [(True, False), (False, True)]:
            with tempfile.TemporaryDirectory() as directory:
                action, calls, saved = self.execute(Path(directory).resolve(), invalid=invalid, changed=changed)
                with self.assertRaises(ValueError):
                    action()
                self.assertEqual(calls, [])
                self.assertEqual(saved, [])

    def test_only_one_full_node_dispatch_uses_short_root_for_all_temp_variables(self):
        with tempfile.TemporaryDirectory() as directory:
            action, calls, saved = self.execute(Path(directory).resolve())
            row = action()
            self.assertEqual(len(calls), 1)
            self.assertEqual(calls[0][0], ['node', '--test', '--test-concurrency=2', 'complete.test.js'])
            for key in ['TMPDIR', 'TEMP', 'TMP', 'AIRK_PRIVATE_CENSUS_BASE']:
                self.assertEqual(calls[0][1][key].replace('\\', '/'), '/private/tmp/airk-n36-abcdefgh')
            self.assertEqual(row['test_counts']['tests'], 1170)
            self.assertEqual(row['result'], 'passed')

    def test_failed_full_node_retains_original_result_without_retry(self):
        with tempfile.TemporaryDirectory() as directory:
            action, calls, saved = self.execute(Path(directory).resolve(), failed=True)
            with self.assertRaisesRegex(ValueError, 'Node command failed'):
                action()
            self.assertEqual(len(calls), 1)
            self.assertEqual(saved[-1]['result'], 'failed')
            self.assertTrue(saved[-1]['owned_descendants_settled'])


if __name__ == '__main__':
    unittest.main()
