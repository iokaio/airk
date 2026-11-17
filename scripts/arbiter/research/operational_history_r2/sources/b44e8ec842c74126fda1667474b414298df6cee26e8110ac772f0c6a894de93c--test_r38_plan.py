# SPDX-License-Identifier: Apache-2.0
"""Pure R38 component-batch contracts; no host, compiler or network execution."""
import ast
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent


def owner():
    spec = importlib.util.spec_from_file_location('r38_owner', HERE / 'neo-scoped-owner.py')
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def generated(root):
    tests = root / 'packages/vscode/src/test'
    out = root / 'packages/vscode/out'
    tests.mkdir(parents=True)
    (out / 'test').mkdir(parents=True)
    (out / 'model.js').write_text('complete generated consumer')
    for name in ('a', 'b'):
        (tests / (name + '.test.ts')).write_text(name)
        (out / 'test' / (name + '.test.js')).write_text(name)
    (out / 'test/installedEditorC8.js').write_text('also frozen despite not being a mocked test')
    return tests, out


class R38PlanTests(unittest.TestCase):
    def test_actual_main_runs_only_dependencies_compile_python_full_node_in_order(self):
        module = owner()
        expected = ['extension-locked-dependencies', 'extension-full-compile',
                    'full-python-contracts', 'full-mocked-extension-tests']
        self.assertEqual(module.REQUIRED_GATES, expected)
        tree = ast.parse((HERE / 'neo-scoped-owner.py').read_text())
        main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main')
        calls = sorted((node for node in ast.walk(main) if isinstance(node, ast.Call)
            and isinstance(node.func, ast.Name) and node.func.id == 'run'), key=lambda node:node.lineno)
        self.assertEqual([node.args[0].value for node in calls], expected)
        self.assertNotIn('cargo', [item.value for node in calls for item in ast.walk(node) if isinstance(item, ast.Constant) and isinstance(item.value, str)])
        plan = json.loads((HERE / 'launch-plan.json').read_text())
        self.assertEqual(len(plan['commands']), 4)
        self.assertEqual(plan['commands'][0][:2], ['npm','ci'])
        self.assertEqual(plan['commands'][1][:3], ['npm','run','compile'])

    def test_complete_generated_and_live_test_rosters_remain_frozen(self):
        module = owner()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            tests, out = generated(root)
            frozen = module.compiled_inputs(root, 2)
            self.assertEqual(frozen['selected_test_files'], 2)
            self.assertEqual(len(frozen['files']), 4)
            module.require_compiled_inputs(root, frozen, 2)
            (out / 'model.js').write_text('changed')
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
            (out / 'test/nested/omitted.test.js').write_text('must never omit')
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
            (tests / 'a.test.ts').write_text('new source')
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
        path = '/private/tmp/airk-r38-abcdefgh'
        self.assertLessEqual(module.validate_fixture_root(path, path, 501, 0o700, False)['socket_bytes_including_nul'],104)
        for args in [(path,path,502,0o700,False),(path,path,501,0o755,False),
                     (path,path,501,0o700,True),(path,'/private/tmp/foreign',501,0o700,False)]:
            with self.assertRaises(ValueError):
                module.validate_fixture_root(*args)
        with self.assertRaises(ValueError):
            module.socket_budget('/private/tmp/' + 'é' * 40)

    def test_actual_handoff_supplies_frozen_test_count_and_bootstraps_before_python(self):
        text = (HERE / 'neo-scoped-owner.py').read_text()
        main = text[text.index('def main('):]
        self.assertLess(main.index('fixtures = create_fixture_root()'), main.index('TMPDIR=str(fixtures)'))
        self.assertLess(main.index('state[\'artifacts_parent\'] = prepare_artifacts(source)'),main.index("run('full-python-contracts'"))
        launcher = (HERE / 'transfer-and-launch.py').read_text()
        self.assertIn("'--expected-test-files',str(cfg['expected_test_files'])",launcher)
        self.assertIn("EXPECTED_TEST_FILES=pins['test_source_files']",launcher)
        self.assertIn("root/'artifacts/local/one-airk-1.8.14/clean-break-r38-neo'",(HERE / 'prepare-source.py').read_text())

    def test_final_qualification_requires_all_gates_settled_and_no_new_target(self):
        module = owner()
        state = {'result':'passed','commands':[{'stage':stage,'result':'passed','owned_descendants_settled':True}
            for stage in module.REQUIRED_GATES], 'prior_native_qualification':{'separate':True},
            'frozen_inputs_final':{'same':True},'compiler_tree_absent_or_empty':True,
            'cargo_cleanup':'no-owned-compiler-target-created'}
        self.assertTrue(module.finalize_qualification(state)['qualification_complete'])
        for key,value in [('compiler_tree_absent_or_empty',False),('prior_native_qualification',None),('frozen_inputs_final',None)]:
            changed={**state,key:value}
            self.assertFalse(module.finalize_qualification(changed)['qualification_complete'])
            self.assertEqual(changed['result'],'failed')
        changed={**state,'commands':state['commands'][:-1]}
        self.assertFalse(module.finalize_qualification(changed)['qualification_complete'])

    def test_prior_native_evidence_requires_actual_398_count_native_identity_and_cleanup(self):
        module=owner()
        native={'sha256':'a'*64,'files':1154}
        prior={'qualification_revision':'R37','result':'passed','qualification_complete':True,
            'source_identity':native,'cargo_cleanup':'completed','compiler_tree_absent_or_empty':True,
            'all_owned_command_descendants_settled':True,'commands':[
                {'stage':stage,'result':'passed','owned_descendants_settled':True,
                 'test_counts':{'selected':398,'executed':398,'skipped':0}}
                for stage in ('scoped-four-crate-clippy','focused-clean-break-native-tests','required-neo-cargo-clean')]}
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory).resolve()/'prior.json'
            path.write_text(json.dumps(prior))
            module.PRIOR_NATIVE_RECEIPT=path
            module.PRIOR_NATIVE_SHA256=module.sha(path)
            self.assertEqual(module.prior_native_qualification(native)['selected_native_tests'],398)
            for change in ({'cargo_cleanup':'failed'},{'source_identity':{'sha256':'b'*64,'files':1154}},
                           {'all_owned_command_descendants_settled':False}):
                path.write_text(json.dumps({**prior,**change}));module.PRIOR_NATIVE_SHA256=module.sha(path)
                with self.assertRaises(ValueError):
                    module.prior_native_qualification(native)
            changed=json.loads(json.dumps(prior))
            changed['commands'][1]['test_counts']['executed']=0
            path.write_text(json.dumps(changed));module.PRIOR_NATIVE_SHA256=module.sha(path)
            with self.assertRaisesRegex(ValueError,'398-test'):
                module.prior_native_qualification(native)
            path.write_text(json.dumps(prior));module.PRIOR_NATIVE_SHA256='f'*64
            with self.assertRaises(ValueError):
                module.prior_native_qualification(native)


if __name__ == '__main__':
    unittest.main()
