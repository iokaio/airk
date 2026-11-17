# SPDX-License-Identifier: Apache-2.0
"""Cheap artifact-owner tests: no source export, SSH, Cargo or installed mutation."""
import ast
import importlib.util
import json
from pathlib import Path
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, HERE / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


OWNER = load("r14_owner", "neo-final-owner.py")
TRANSFER = load("r14_transfer_test", "neo-transfer-and-launch-r14.py")
COLLECT = load("r14_collect_test", "collect-neo-r14.py")


class PreparationTests(unittest.TestCase):
    def junit(self, body, required=()):
        with tempfile.TemporaryDirectory(dir=HERE) as folder:
            path = Path(folder) / "fictional.xml"
            path.write_text('<testsuites><testsuite>' + body + '</testsuite></testsuites>')
            return OWNER.require_junit(path, required)

    def test_zero_tests_skipped_required_failures_and_duplicates_never_qualify(self):
        for xml, required in [
            ('', ()),
            ('<testcase name="held"><skipped/></testcase>', ()),
            ('<testcase name="held"><failure/></testcase>', ()),
            ('<testcase name="held"><error/></testcase>', ()),
            ('<testcase name="other"/>', ('held',)),
            ('<testcase name="other"/><testcase name="held"><skipped/></testcase>', ('held',)),
            ('<testcase name="held"/><testcase name="held"/>', ('held',)),
        ]:
            with self.subTest(xml=xml), self.assertRaises(ValueError):
                self.junit(xml, required)
        self.assertEqual(self.junit('<testcase name="held"/><testcase name="optional"><skipped/></testcase>', ('held',)),
                         {'executed': 1, 'skipped': 1, 'total': 2})

    def test_unreviewed_pins_fail_before_subprocess_or_output(self):
        with tempfile.TemporaryDirectory(dir=HERE) as folder:
            path = Path(folder) / 'unapproved.json'
            path.write_text(json.dumps({'status': 'PREPARED_ONLY'}))
            with patch.object(TRANSFER.subprocess, 'run', side_effect=AssertionError('No subprocess allowed')):
                with self.assertRaisesRegex(ValueError, 'review'):
                    TRANSFER.main(['--approved-pins', str(path), '--execute'])
            self.assertEqual([p.name for p in Path(folder).iterdir()], ['unapproved.json'])

    def test_signed_runner_and_stale_identity_placeholders_are_rejected(self):
        value = {'status': 'REVIEWED_READY_FOR_SINGLE_NEO_BATCH', 'primary_reviewed': True,
                 'independent_reviewed': True, 'inputs_frozen': True, 'windows_stage3_complete': True,
                 **{key: 'a' * 64 for key in ('transfer_receipt_sha256', 'restorer_sha256', 'source_roster_sha256', 'public_models_roster_sha256', 'launcher_sha256', 'windows_stage3_receipt_sha256')},
                 'native_source': {'sha256': 'b' * 64, 'files': 1}, 'source_file_count': 1,
                 'runners': {'neo-final-owner.py': 'c' * 64, 'neo-final-signed-owner.py': 'd' * 64}}
        with self.assertRaisesRegex(ValueError, 'unsigned'):
            TRANSFER.validate_pins(value)
        value['runners'] = {'neo-final-owner.py': 'c' * 64}
        value['native_source']['sha256'] = 'PENDING_FINAL_FREEZE'
        with self.assertRaisesRegex(ValueError, 'native'):
            TRANSFER.validate_pins(value)

    def test_first_compilation_is_scoped_clippy_followed_by_retirement(self):
        tree = ast.parse((HERE / 'neo-final-owner.py').read_text())
        stages = [node.args[0].value for node in ast.walk(tree)
                  if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                  and node.func.id == 'run' and node.args and isinstance(node.args[0], ast.Constant)]
        # Sort by source line to make this a contract for the actual call order.
        calls = sorted([node for node in ast.walk(tree) if isinstance(node, ast.Call)
                        and isinstance(node.func, ast.Name) and node.func.id == 'run'
                        and node.args and isinstance(node.args[0], ast.Constant)], key=lambda node: node.lineno)
        stages = [node.args[0].value for node in calls]
        self.assertEqual(stages[:5], ['verify-public-offline-models', 'fetch-locked-rust-dependencies',
                                     'rust-format', 'macos-first-scoped-cli-clippy', 'macos-upgrade-stop-and-retirement'])
        self.assertEqual(stages[-1], 'required-neo-cargo-clean')
        scoped = ast.unparse(calls[3])
        self.assertIn("'-p', 'airk-cli'", scoped)
        self.assertNotIn("'--workspace'", scoped)
        self.assertIn("'--incremental', 'off'", scoped)
        self.assertIn('uninstall_stops_owned_clients_and_prevents_relaunch_without_stopping_foreign_processes', '\n'.join(OWNER.RETIREMENT_REQUIRED))

    def test_required_retirement_names_exist_in_live_source(self):
        source = HERE.parents[3] / 'crates/airk-cli/src'
        text = '\n'.join((source / name).read_text() for name in (
            'runtime_upgrade_stop.rs', 'runtime_upgrade_stop_macos.rs', 'runtime_retirement.rs'))
        for name in OWNER.RETIREMENT_REQUIRED:
            self.assertIn('fn ' + name.rsplit('::', 1)[-1] + '(', text, name)

    def test_input_mutation_stops_work_without_preventing_owned_cleanup(self):
        # Exercise the actual nested command owner with injected fake observers;
        # no compiler, platform operation or subprocess starts.
        tree = ast.parse((HERE / 'neo-final-owner.py').read_text())
        command = next(node for node in ast.walk(tree) if isinstance(node, ast.FunctionDef) and node.name == 'run')
        wrapper = ast.parse('def factory():\n active = None\n return run\n').body[0]
        wrapper.body.insert(1, command)
        module = ast.fix_missing_locations(ast.Module(body=[wrapper], type_ignores=[]))
        with tempfile.TemporaryDirectory(dir=HERE) as folder:
            root = Path(folder)
            calls = []
            state = {'commands': [], 'source_identity': {'sha256': 'fictional'}}
            owner = SimpleNamespace(run_owned=lambda *args: calls.append(args) or {'exit_code': 0}, successful=lambda result: True)
            def changed():
                raise ValueError('Native inputs changed during batch')
            namespace = {'campaign': root, 'source': root, 'target': root / 'compiler', 'evidence': root,
                         'development': {}, 'state': state, 'save': lambda: None,
                         'frozen_inputs': changed, 'owner': owner, 'nextest_gates': {}, 'time': time}
            exec(compile(module, '<actual nested R14 command owner>', 'exec'), namespace)
            run = namespace['factory']()
            with self.assertRaisesRegex(ValueError, 'changed'):
                run('first-step', ['not-executed'])
            self.assertFalse(state['commands'][0]['owned_command_started'])
            self.assertEqual(calls, [])
            run('required-neo-cargo-clean', ['fake-owned-cleanup'])
            self.assertEqual(len(calls), 1)

    def test_collection_cannot_include_private_fixture_or_index_contents(self):
        for name in ('../private.key', 'issuer-rotation-private/data/index.sqlite',
                     'issuer-rotation-private/control/issuer.key', 'sources/file.txt',
                     'nextest-retirement-store/ci/private.json', 'cargo-timings/../index.db', '/absolute'):
            self.assertFalse(COLLECT.allowed(name, ['macos-first-scoped-cli-clippy']), name)
        for name in ('batch-receipt.json', 'macos-first-scoped-cli-clippy.log',
                     'cargo-timings/cargo-timing-20261108.html',
                     'nextest-retirement-store/ci/junit.xml',
                     'issuer-rotation-private/issuer-rotation-report.json'):
            self.assertTrue(COLLECT.allowed(name, ['macos-first-scoped-cli-clippy']), name)

    def test_fixture_snapshot_copies_only_known_cleanup_reports_and_retains_originals(self):
        with tempfile.TemporaryDirectory(dir=HERE) as folder:
            root = Path(folder)
            fixtures = root / 'fixtures'
            failed = fixtures / 'failed-owned-child'
            private = failed / 'data'
            private.mkdir(parents=True)
            (private / 'index.db').write_bytes(b'never copy index bytes')
            (failed / 'credential.key').write_bytes(b'never copy secret bytes')
            report = failed / ('managed-cleanup-error-' + 'a' * 32 + '.json')
            report.write_text(json.dumps({'fixture_preserved': True, 'cleanup': 'pending'}))
            proof = failed / 'https-lifecycle-evidence.json'
            proof.write_text(json.dumps({'managed_stop': {'phase': 'complete'}}))
            evidence = root / 'evidence'
            evidence.mkdir()
            rows = OWNER.snapshot_fixture_reports([fixtures], evidence)
            self.assertEqual(len(rows), 2)
            self.assertEqual({Path(row['remote_original']) for row in rows}, {report, proof})
            self.assertEqual((private / 'index.db').read_bytes(), b'never copy index bytes')
            self.assertEqual(len(list((evidence / 'fixture-cleanup-receipts').iterdir())), 3)
            for row in rows:
                self.assertEqual((evidence / row['path']).read_bytes(), Path(row['remote_original']).read_bytes())

    def test_fixture_report_with_secret_marker_is_kept_remote(self):
        with tempfile.TemporaryDirectory(dir=HERE) as folder:
            root = Path(folder)
            fixtures = root / 'fixtures'
            fixtures.mkdir()
            report = fixtures / 'https-lifecycle-evidence.json'
            report.write_text(json.dumps({'unexpected': 'Bearer fictional-private-value'}))
            evidence = root / 'evidence'
            evidence.mkdir()
            with self.assertRaisesRegex(ValueError, 'secret'):
                OWNER.snapshot_fixture_reports([fixtures], evidence)
            self.assertEqual(list((evidence / 'fixture-cleanup-receipts').iterdir()), [])
            self.assertTrue(report.is_file())


if __name__ == '__main__':
    unittest.main()
