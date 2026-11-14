# SPDX-License-Identifier: Apache-2.0
"""Evidence bookkeeping controls; these do not launch or qualify a native pair."""
from contextlib import contextmanager
import importlib.util
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    'workbench_window_scope', Path(__file__).parents[1] / 'check-workbench-window-scope.py')
HARNESS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HARNESS)


class WorkbenchEvidenceTests(unittest.TestCase):
    def test_inventory_tracks_render_helpers_and_added_compiled_members(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ('scripts/peer.cjs', 'packages/vscode/out/workbenchView.js',
                         'packages/vscode/out/workbenchRender.js',
                         'packages/vscode/package.json', 'packages/vscode/package-lock.json',
                         'fixtures/split/arbiter/public_answer_cases.json', 'fixtures/split/arbiter/context_action_cases.json',
                         'packages/contracts-gen/arbiter.schema.json'):
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(name, encoding='utf-8')
            with patch.object(HARNESS, 'ROOT', root):
                before = HARNESS.input_inventory()
                render = root / 'packages/vscode/out/workbenchRender.js'
                render.write_text('changed export implementation', encoding='utf-8')
                self.assertNotEqual(before, HARNESS.input_inventory())
                render.write_text('packages/vscode/out/workbenchRender.js', encoding='utf-8')
                self.assertEqual(before, HARNESS.input_inventory())
                (root / 'packages/vscode/out/newDependency.js').write_text('new', encoding='utf-8')
                self.assertNotEqual(before, HARNESS.input_inventory())

    def qualification(self, *, cleanup_fails, answers=2, sources=2, refused=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / 'runtime'; runtime.mkdir()
            owner = root / 'owner'; owner.mkdir()
            cli = runtime / 'airkctl.exe'; cli.write_bytes(b'inert fixture; never executed')
            node = root / 'node'; node.write_bytes(b'inert node fixture; never executed')
            (runtime / 'airk-runtime.json').write_text(json.dumps({'schema': 3, 'version': '1.9.0'}))
            (runtime / 'airk-build.json').write_text(json.dumps({'source': 'original-artifact-source'}))
            report = root / 'report.json'
            events = []

            @contextmanager
            def owned():
                try:
                    yield owner
                finally:
                    events.append('directory-cleanup')
                    self.assertFalse(report.exists(), 'pass was written before directory cleanup')
                    if cleanup_fails:
                        raise OSError('observed directory cleanup failure')

            class Fixture:
                catalog_authority = True
                environment = {}
                def __init__(self, runtime, owner):
                    self.cli = cli; self.control = owner / 'control'
                    self.control.mkdir(); self.projects = {}; self.scopes = {}
                def __enter__(self): return self
                def __exit__(self, *_): events.append('native-cleanup')
                def register(self, name, source, _data):
                    self.projects[name] = {'root': str(source), 'binding': name}
                    self.scopes[name] = {'index_id': name}
                def job(self, *_): pass
                def published_status(self, *_): pass
                def assert_alive(self): pass

            clients = SimpleNamespace(owned_client_fixture=owned, Fixture=Fixture)
            package = SimpleNamespace(validate_runtime=lambda *_: ({'airkctl.exe': 'exact-original'}, None))
            def run(argv, **_):
                request = json.loads(Path(argv[-1]).read_bytes())
                self.assertEqual(request['answer_request']['schema'], 'airk.arbiter.context.request/2')
                self.assertEqual(request['answer_request']['question'], 'amber_marker')
                self.assertTrue(all(request['answer_request']['question'] in Path(file).read_text() for file in request['source_hashes']))
                Path(request['output']).write_text(json.dumps({'exactComparisons': 10, 'answerComparisons': answers, 'answerSourceOpens': sources}))
                return SimpleNamespace(returncode=0, stdout='', stderr='')
            with patch.object(HARNESS, 'module', side_effect=lambda name, _: clients if name == 'workbench_native_clients' else package), \
                    patch.object(HARNESS, 'input_inventory', return_value={'compiled': 'sealed'}), \
                    patch.object(HARNESS.subprocess, 'run', side_effect=run), \
                    patch.object(HARNESS.subprocess, 'check_output', return_value='v24.15.0\n'), \
                    patch('sys.stdout', new=io.StringIO()):
                if refused:
                    with self.assertRaises(AssertionError):
                        HARNESS.qualify(runtime, node, report, 'inert-test-target')
                    self.assertFalse(report.exists(), 'partial Answer exercise cannot leave a pass')
                elif cleanup_fails:
                    with self.assertRaisesRegex(OSError, 'observed directory cleanup failure'):
                        HARNESS.qualify(runtime, node, report, 'inert-test-target')
                    self.assertFalse(report.exists())
                else:
                    HARNESS.qualify(runtime, node, report, 'inert-test-target')
                    value = json.loads(report.read_bytes())
                    self.assertEqual(value['fixture_cleanup'], 'completed')
                    self.assertEqual(value['runtime']['source'], 'original-artifact-source')
            self.assertEqual(events, ['native-cleanup', 'directory-cleanup'])

    def test_missing_real_answer_routes_cannot_leave_a_pass_receipt(self):
        self.qualification(cleanup_fails=False, answers=0, refused=True)

    def test_missing_actual_answer_source_checks_cannot_leave_a_pass_receipt(self):
        self.qualification(cleanup_fails=False, sources=1, refused=True)

    def test_failed_owned_directory_cleanup_cannot_leave_a_pass_receipt(self):
        self.qualification(cleanup_fails=True)

    def test_pass_receipt_follows_both_cleanups_and_retains_original_artifact_identity(self):
        self.qualification(cleanup_fails=False)


if __name__ == '__main__':
    unittest.main()
