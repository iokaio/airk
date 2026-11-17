# SPDX-License-Identifier: Apache-2.0
"""Pure R39 component-batch contracts; no host, compiler or network execution."""
import ast
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent


def owner():
    spec = importlib.util.spec_from_file_location('r39_owner', HERE / 'neo-scoped-owner.py')
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def generated(root):
    tests = root / 'packages/vscode/src/test'
    out = root / 'packages/vscode/out'
    tests.mkdir(parents=True)
    (out / 'test').mkdir(parents=True)
    (out / 'model.js').write_text('complete generated consumer', encoding='utf-8')
    for name in ('a', 'b'):
        (tests / (name + '.test.ts')).write_text(name, encoding='utf-8')
        (out / 'test' / (name + '.test.js')).write_text(name, encoding='utf-8')
    (out / 'test/installedEditorC8.js').write_text('also frozen despite not being a mocked test', encoding='utf-8')
    return tests, out


class R39PlanTests(unittest.TestCase):

    def test_complete_generated_and_live_test_rosters_remain_frozen(self):
        module = owner()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            tests, out = generated(root)
            frozen = module.compiled_inputs(root, 2)
            self.assertEqual(frozen['selected_test_files'], 2)
            self.assertEqual(len(frozen['files']), 4)
            module.require_compiled_inputs(root, frozen, 2)
            (out / 'model.js').write_text('changed', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'Compiled extension inputs changed'):
                module.require_compiled_inputs(root, frozen, 2)

    def test_zero_omitted_nested_and_redirected_catalogs_refuse(self):
        module = owner()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            tests, out = generated(root)
            with self.assertRaises(ValueError):
                module.compiled_inputs(root, 0)
            with patch.object(Path, 'is_symlink', lambda value:value == out):
                with self.assertRaisesRegex(ValueError, 'redirected'):
                    module.compiled_inputs(root, 2)
            (out / 'test/nested').mkdir()
            (out / 'test/nested/omitted.test.js').write_text('must never omit', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'Nested compiled test'):
                module.compiled_inputs(root, 2)
            (out / 'test/nested/omitted.test.js').unlink()
            (out / 'test/a.test.js').unlink()
            with self.assertRaises(ValueError):
                module.compiled_inputs(root, 2)

    def test_test_source_change_is_input_drift_even_without_new_compilation(self):
        module = owner()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            tests, out = generated(root)
            frozen = module.compiled_inputs(root, 2)
            (tests / 'a.test.ts').write_text('new source', encoding='utf-8')
            with self.assertRaises(ValueError):
                module.require_compiled_inputs(root, frozen, 2)

    def test_nested_source_directory_redirect_refuses_before_test_file_selection(self):
        module = owner()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            tests, out = generated(root)
            nested = tests / 'nested-redirect'
            nested.mkdir()
            # Simulate directory link metadata without requiring Windows link
            # creation privileges. The real descendant walker must observe it.
            with patch.object(Path, 'is_symlink', lambda value:value == nested):
                with self.assertRaisesRegex(ValueError, 'Redirected test source entry'):
                    module.compiled_inputs(root, 2)
            original = Path.resolve
            with patch.object(Path, 'resolve', lambda value,*args,**kwargs:
                              root if value == nested else original(value,*args,**kwargs)):
                with self.assertRaisesRegex(ValueError, 'Redirected test source entry'):
                    module.compiled_inputs(root, 2)

    def test_private_root_and_utf8_socket_budget_refuse_foreign_or_long_paths(self):
        module = owner()
        path = '/private/tmp/airk-r39-abcdefgh'
        self.assertLessEqual(module.validate_fixture_root(path, path, 501, 0o700, False)['socket_bytes_including_nul'],104)
        for args in [(path,path,502,0o700,False),(path,path,501,0o755,False),
                     (path,path,501,0o700,True),(path,'/private/tmp/foreign',501,0o700,False)]:
            with self.assertRaises(ValueError):
                module.validate_fixture_root(*args)
        with self.assertRaises(ValueError):
            module.socket_budget('/private/tmp/' + 'é' * 40)

    def test_actual_handoff_supplies_frozen_test_count_and_bootstraps_before_python(self):
        text = (HERE / 'neo-scoped-owner.py').read_text(encoding='utf-8')
        main = text[text.index('def main('):]
        self.assertLess(main.index('fixtures = create_fixture_root()'), main.index('TMPDIR=str(fixtures)'))
        self.assertLess(main.index('state[\'artifacts_parent\'] = prepare_artifacts(source)'),main.index("run('full-python-contracts'"))
        launcher = (HERE / 'transfer-and-launch.py').read_text(encoding='utf-8')
        self.assertIn("'--expected-test-files',str(cfg['expected_test_files'])",launcher)
        self.assertIn("EXPECTED_TEST_FILES=pins['test_source_files']",launcher)
        self.assertIn("root/'artifacts/local/one-airk-1.8.14/clean-break-r39-neo'",(HERE / 'prepare-source.py').read_text(encoding='utf-8'))



    def test_export_and_template_target_only_new_preparation_directory(self):
        source = (HERE / 'prepare-source.py').read_text(encoding='utf-8')
        self.assertIn("root/'artifacts/local/one-airk-1.8.14/clean-break-r39-neo'", source)
        self.assertNotIn("root/'artifacts/local/one-airk-1.8.14/clean-break-r35-neo'", source)
        template = json.loads((HERE / 'approved-pins.template.json').read_text(encoding='utf-8'))
        self.assertIn('/clean-break-r39-neo/', template['package'])
        self.assertFalse(template['primary_reviewed'])
        self.assertFalse(template['inputs_frozen'])

    def test_order_is_clippy_native_dependency_compile_python_mocked_node(self):
        expected = ['scoped-three-crate-clippy',
            'focused-canonical-budget-native-tests', 'extension-locked-dependencies',
            'extension-full-compile', 'full-python-contracts', 'full-mocked-extension-tests']
        self.assertEqual(owner().REQUIRED_GATES, expected)
        tree = ast.parse((HERE / 'neo-scoped-owner.py').read_text(encoding='utf-8'))
        main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main')
        calls = sorted((node for node in ast.walk(main) if isinstance(node, ast.Call)
            and isinstance(node.func, ast.Name) and node.func.id == 'run'), key=lambda node: node.lineno)
        self.assertEqual([node.args[0].value for node in calls], expected + ['required-neo-cargo-clean'])

    def test_required_cleanup_and_input_fences_survive_every_terminal_path(self):
        source = (HERE / 'neo-scoped-owner.py').read_text(encoding='utf-8')
        self.assertIn("run('required-neo-cargo-clean'", source)
        self.assertIn("state['evidence_preserved_before_clean']", source)
        self.assertIn('not descendants_settled', source)
        self.assertIn("row['frozen_inputs_after'] = frozen_inputs()", source)
        self.assertIn("if target.exists()", source)
        self.assertIn("target.parent != campaign", source)
        self.assertIn("Refusing to reuse another compiler/fixture/evidence owner", source)


    def test_real_clippy_nextest_calls_use_affected_crates_and_exact_transport(self):
        module = owner()
        self.assertEqual(module.PACKAGES, ['airk-cli', 'airk-mcp', 'airk-kernel'])
        exact = 'capabilities_plans_keep_complete_native_cli_and_mcp_results_above_read_frame'
        self.assertEqual(module.TRANSPORT_TEST, exact)
        for group in ('combined_frontend', 'workbench', 'actual_combined_capabilities', 'full_capability_catalog', exact):
            self.assertIn('test(' + group + ')', module.FILTER)
        text = (HERE / 'neo-scoped-owner.py').read_text(encoding='utf-8')
        self.assertIn("'--locked', '--timings', '-E', FILTER", text)
        self.assertIn("'--features', 'local-inference', '--target-dir', target, '--incremental', 'off'", text)
        plan = json.loads((HERE / 'launch-plan.json').read_text(encoding='utf-8'))
        self.assertEqual(plan['commands'][1][-1], module.FILTER)
        self.assertEqual(plan['commands'][0][2:8], ['-p', 'airk-cli', '-p', 'airk-mcp', '-p', 'airk-kernel'])

    def test_junit_requires_every_actual_group_and_nonzero_transport_execution(self):
        module = owner()
        groups = ['combined_frontend::budget', 'workbench::clipped', 'actual_combined_capabilities',
                  'full_capability_catalog', module.TRANSPORT_TEST]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'junit.xml'
            def publish(names, skipped=False, failed=False):
                body = ''.join('<testcase name="' + name + '">' +
                    ('<skipped/>' if skipped else '') + ('<failure/>' if failed else '') + '</testcase>' for name in names)
                path.write_text('<testsuite>' + body + '</testsuite>', encoding='utf-8')
            publish(groups)
            self.assertEqual(module.require_junit(path)['executed'], 5)
            for missing in range(len(groups)):
                publish(groups[:missing] + groups[missing+1:])
                with self.assertRaises(ValueError):
                    module.require_junit(path)
            for names, skipped, failed in [([], False, False), (groups, True, False), (groups, False, True)]:
                publish(names, skipped, failed)
                with self.assertRaises(ValueError):
                    module.require_junit(path)

    def test_cleanup_settlement_is_part_of_final_qualification(self):
        module = owner()
        rows = [{'stage':stage, 'result':'passed', 'owned_descendants_settled':True}
                for stage in module.REQUIRED_GATES + ['required-neo-cargo-clean']]
        state = {'result':'passed', 'commands':rows, 'unavailable':[],
                 'evidence_preserved_before_clean':True, 'cargo_cleanup':'completed',
                 'compiler_tree_absent_or_empty':True}
        self.assertTrue(module.finalize_qualification(dict(state))['qualification_complete'])
        for changed in ({**state, 'evidence_preserved_before_clean':False},
                        {**state, 'cargo_cleanup':'failed'},
                        {**state, 'commands':rows[1:]},
                        {**state, 'commands':rows[:-1] + [{**rows[-1], 'owned_descendants_settled':False}]}):
            self.assertFalse(module.finalize_qualification(changed)['qualification_complete'])

    def test_successful_clean_command_requires_actual_empty_target_proof(self):
        module = owner()
        rows = [{'stage':stage, 'result':'passed', 'owned_descendants_settled':True}
                for stage in module.REQUIRED_GATES + ['required-neo-cargo-clean']]
        state = {'result':'passed','commands':rows,'unavailable':[],
                 'evidence_preserved_before_clean':True,'cargo_cleanup':'completed'}
        for proof in (False, None):
            changed = module.finalize_qualification({**state,'compiler_tree_absent_or_empty':proof})
            self.assertFalse(changed['qualification_complete'])
            self.assertEqual(changed['result'],'failed')
        self.assertFalse(module.finalize_qualification(dict(state))['qualification_complete'])

    def test_real_collector_inventory_keeps_native_junit_and_timings_on_failed_batches(self):
        specification = importlib.util.spec_from_file_location('r39_collect',HERE/'collect.py')
        collector = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(collector)
        for name in ('nextest.toml','nextest-store/ci/junit.xml','cargo-timings/cargo-timing.html'):
            self.assertTrue(collector.allowed(name,[]),name)
        for name in ('issuer-rotation-private/secret.pem','cargo-timings/../../private.html',
                     'cargo-timings/report.json','cargo-timings/nested/report.html'):
            self.assertFalse(collector.allowed(name,[]),name)
        tree=ast.parse((HERE/'collect.py').read_text(encoding='utf-8'))
        script=next(node.value for node in ast.walk(tree) if isinstance(node,ast.Constant)
                    and isinstance(node.value,str) and 'names=set(REPORTS)' in node.value)
        with tempfile.TemporaryDirectory() as directory:
            campaign=Path(directory).resolve()
            evidence=campaign/'evidence'
            evidence.mkdir()
            (campaign/'phase.txt').write_text('complete-failed',encoding='utf-8')
            receipt={'commands':[{'stage':'focused-canonical-budget-native-tests'}],
                'result':'failed','cargo_cleanup':'completed','compiler_tree_absent_or_empty':True,
                'all_owned_command_descendants_settled':True,'evidence_preserved_before_clean':True}
            (evidence/'batch-receipt.json').write_text(json.dumps(receipt),encoding='utf-8')
            for name in ('nextest.toml','nextest-store/ci/junit.xml','cargo-timings/cargo-timing.html',
                         'focused-canonical-budget-native-tests.log'):
                path=evidence/name
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_text('retained report',encoding='utf-8')
            code=script.replace('CAMPAIGN',repr(str(campaign))).replace('ROOT_REPORTS',repr(collector.ROOT_REPORTS)).replace('REPORTS',repr(sorted(collector.REPORTS)))
            output=io.StringIO()
            with contextlib.redirect_stdout(output):
                exec(compile(code,'actual-read-only-collector-inventory','exec'),{})
            observed=json.loads(output.getvalue())
            names={row['path'] for row in observed['files']}
            self.assertTrue({'nextest.toml','nextest-store/ci/junit.xml','cargo-timings/cargo-timing.html'} <= names)
            self.assertEqual(observed['result'],'failed')
            self.assertTrue(observed['compiler_tree_absent_or_empty'])
            self.assertTrue(observed['evidence_preserved_before_clean'])
            self.assertFalse(observed['private_fixtures_copied'])

    def test_actual_launcher_distinguishes_owner_launch_from_unobserved_native_build(self):
        tree=ast.parse((HERE/'transfer-and-launch.py').read_text(encoding='utf-8'))
        calls=[node for node in ast.walk(tree) if isinstance(node,ast.Call)
               and isinstance(node.func,ast.Attribute) and node.func.attr=='update'
               and isinstance(node.func.value,ast.Name) and node.func.value.id=='receipt'
               and any(keyword.arg=='native_batch_launched' for keyword in node.keywords)]
        self.assertEqual(len(calls),1,'Review the actual post-launch receipt writer')
        fields={keyword.arg:keyword.value for keyword in calls[0].keywords}
        self.assertIs(fields['native_batch_launched'].value,True)
        self.assertIsNone(fields['native_build_started'].value)
        self.assertIsNone(fields['build_started'].value)
        self.assertIn('actual native command state',fields['build_state_qualification'].value)

    def test_child_environment_never_enables_native_python_or_mocked_node(self):
        text = (HERE / 'neo-scoped-owner.py').read_text(encoding='utf-8')
        self.assertIn("not key.startswith(('AIRK_'", text)
        self.assertIn("inherited_airk_keys_removed_from_child_only", text)
        tree = ast.parse(text)
        for node in ast.walk(tree):
            if isinstance(node, ast.keyword):
                self.assertNotIn(node.arg, ('AIRK_NATIVE_TEST_BINARY', 'AIRK_RUN_NATIVE_TESTS', 'AIRK_REQUIRE_NATIVE_TESTS'))

    def test_export_includes_public_untracked_kernel_regressions_and_pins_are_unapproved(self):
        text = (HERE / 'prepare-source.py').read_text(encoding='utf-8')
        self.assertIn("'--cached','--others','--exclude-standard'", text)
        self.assertIn("'crates'", text)
        self.assertIn("if public and parts[0] not in PUBLIC", text)
        template = json.loads((HERE / 'approved-pins.template.json').read_text(encoding='utf-8'))
        for gate in ('primary_reviewed', 'independent_reviewed', 'inputs_frozen', 'windows_checks_passed'):
            self.assertFalse(template[gate])
        self.assertIsNone(template['native_source']['sha256'])


if __name__ == '__main__':
    unittest.main()
