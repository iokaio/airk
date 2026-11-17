# SPDX-License-Identifier: Apache-2.0
"""R37 cold-checkout and Unix socket orchestration contracts; no execution."""
import ast
import importlib.util
import json
from pathlib import Path, PurePosixPath
import tempfile
import unittest

HERE = Path(__file__).resolve().parent


def owner():
    spec = importlib.util.spec_from_file_location('r37_owner', HERE / 'neo-scoped-owner.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class R37PlanTests(unittest.TestCase):
    def test_generated_inputs_and_complete_test_selection_stay_frozen_after_compile(self):
        module = owner()
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory).resolve()
            out = source / 'packages/vscode/out'
            (out / 'test').mkdir(parents=True)
            (out / 'model.js').write_text('compiled consumer')
            (out / 'test/b.test.js').write_text('b')
            (out / 'test/a.test.js').write_text('a')
            frozen = module.compiled_inputs(source)
            self.assertEqual(frozen['test_files'], [str(out / 'test/a.test.js'), str(out / 'test/b.test.js')])
            module.require_compiled_inputs(source, frozen)
            (out / 'model.js').write_text('changed consumer')
            with self.assertRaisesRegex(ValueError, 'Compiled extension inputs changed'):
                module.require_compiled_inputs(source, frozen)

    def test_export_and_template_target_only_new_preparation_directory(self):
        source = (HERE / 'prepare-source.py').read_text()
        self.assertIn("root/'artifacts/local/one-airk-1.8.14/clean-break-r37-neo'", source)
        self.assertNotIn("root/'artifacts/local/one-airk-1.8.14/clean-break-r35-neo'", source)
        template = json.loads((HERE / 'approved-pins.template.json').read_text())
        self.assertIn('/clean-break-r37-neo/', template['package'])
        self.assertFalse(template['primary_reviewed'])
        self.assertFalse(template['inputs_frozen'])
    def test_order_is_clippy_native_dependency_compile_python_mocked_node(self):
        expected = ['scoped-four-crate-clippy',
            'focused-clean-break-native-tests', 'extension-locked-dependencies',
            'extension-full-compile', 'full-python-contracts', 'full-mocked-extension-tests']
        self.assertEqual(owner().REQUIRED_GATES, expected)
        tree = ast.parse((HERE / 'neo-scoped-owner.py').read_text())
        main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main')
        calls = sorted((node for node in ast.walk(main) if isinstance(node, ast.Call)
            and isinstance(node.func, ast.Name) and node.func.id == 'run'), key=lambda node: node.lineno)
        self.assertEqual([node.args[0].value for node in calls], expected + ['required-neo-cargo-clean'])

    def test_private_short_base_rejects_deep_unicode_and_foreign_roots(self):
        module = owner()
        proof = module.validate_fixture_root('/private/tmp/airk-r37-abcdefgh',
            '/private/tmp/airk-r37-abcdefgh', 501, 0o700, False)
        self.assertLessEqual(proof['socket_bytes_including_nul'], 104)
        for args in [('/private/tmp/airk-r37-abcdefgh', '/private/tmp/elsewhere', 501, 0o700, False),
            ('/private/tmp/airk-r37-abcdefgh', '/private/tmp/airk-r37-abcdefgh', 502, 0o700, False),
            ('/private/tmp/airk-r37-abcdefgh', '/private/tmp/airk-r37-abcdefgh', 501, 0o755, False),
            ('/private/tmp/airk-r37-abcdefgh', '/private/tmp/airk-r37-abcdefgh', 501, 0o700, True)]:
            with self.assertRaises(ValueError):
                module.validate_fixture_root(*args)
        with self.assertRaises(ValueError):
            module.socket_budget(PurePosixPath('/private/tmp') / ('é' * 40))

    def test_actual_main_prepares_real_short_root_before_child_environment(self):
        source = (HERE / 'neo-scoped-owner.py').read_text()
        tree = ast.parse(source)
        main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main')
        assignments = [node for node in ast.walk(main) if isinstance(node, ast.Assign)]
        fixture = next(node for node in assignments if any(isinstance(target, ast.Name) and target.id == 'fixtures' for target in node.targets))
        self.assertEqual(ast.unparse(fixture.value), 'create_fixture_root()')
        self.assertIn('TMPDIR=str(fixtures)', source)
        self.assertIn('AIRK_PRIVATE_CENSUS_BASE=str(fixtures)', source)
        self.assertIn('prepare_artifacts(source)', source)
        self.assertIn('validate_assistance_factory(source)', source)
        self.assertLess(source.index('prepare_artifacts(source)', source.index('def main(')),
            source.index("run('full-python-contracts'"))

    def test_required_cleanup_and_input_fences_survive_every_terminal_path(self):
        source = (HERE / 'neo-scoped-owner.py').read_text()
        self.assertIn("run('required-neo-cargo-clean'", source)
        self.assertIn("state['evidence_preserved_before_clean']", source)
        self.assertIn('not descendants_settled', source)
        self.assertIn("row['frozen_inputs_after'] = frozen_inputs()", source)
        self.assertIn("if target.exists()", source)
        self.assertIn("target.parent != campaign", source)
        self.assertIn("Refusing to reuse another compiler/fixture/evidence owner", source)


if __name__ == '__main__':
    unittest.main()
