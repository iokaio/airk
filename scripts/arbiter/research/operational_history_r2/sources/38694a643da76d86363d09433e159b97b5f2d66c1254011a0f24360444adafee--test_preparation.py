# SPDX-License-Identifier: Apache-2.0
"""Local pure preparation regressions; no SSH, subprocess, source freeze or compiler."""
import importlib.util
import hashlib
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import shutil
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent


def load(name, filename):
    specification = importlib.util.spec_from_file_location(name, HERE / filename)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


OWNER = load('neo_scoped_owner', 'neo-scoped-owner.py')
TRANSFER = load('neo_scoped_transfer', 'transfer-and-launch.py')
EXPORT = load('neo_scoped_export', 'prepare-source.py')
COLLECT = load('neo_scoped_collect', 'collect.py')
DRAFT = load('neo_r35_draft', 'prepare-final-pins.py')


class PreparationContracts(unittest.TestCase):
    def test_scope_is_exactly_requested(self):
        self.assertEqual(OWNER.PACKAGES, ['airk-core', 'airk-store', 'airk-cli', 'airk-mcp'])
        self.assertEqual(OWNER.FILTER, 'test(clean_break) | test(rebuild) | test(retained_cleanup_source_roots) | test(lifecycle) | test(retry_successors) | test(http) | test(cleanup_owner_census) | test(service_owned_empty)')

    def test_launch_mapping_matches_filter_and_ordered_scoped_gates(self):
        mapping = json.loads((HERE / 'launch-plan.json').read_text(encoding='utf-8'))
        clippy, nextest, npm, compile_command, python, node, clean = mapping['commands']
        for command in (clippy, nextest):
            self.assertEqual([command[i + 1] for i, value in enumerate(command) if value == '-p'], OWNER.PACKAGES)
            self.assertEqual(command[command.index('--features') + 1], 'local-inference')
        self.assertEqual(clippy[clippy.index('--incremental') + 1], 'off')
        self.assertEqual(nextest[nextest.index('-E') + 1], OWNER.FILTER)
        self.assertEqual(nextest[nextest.index('--profile') + 1], 'ci')
        self.assertEqual(nextest[nextest.index('--test-threads') + 1], '2')
        self.assertIn('--timings', nextest)
        self.assertIn('--no-default-features', nextest)
        self.assertEqual(python[1:], ['-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v'])
        self.assertEqual(npm[:2], ['npm', 'ci'])
        self.assertEqual(compile_command[:3], ['npm', 'run', 'compile'])
        self.assertIn('--test-concurrency=2', node)
        self.assertEqual(clean, ['cargo', 'clean', '--target-dir', '<campaign>/cargo-target'])

    def test_draft_pins_require_current_inputs_and_never_invent_review(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            base = root / 'clean-break-r35-neo'
            base.mkdir()
            package = base / 'neo-frozen-transfer-r35-fixture'
            package.mkdir()
            manifest = {'status': 'FROZEN_PUBLIC_SOURCE', 'final_freeze_required': False,
                        'native_source': {'sha256': '1' * 64, 'files': 1},
                        'source_roster_sha256': '2' * 64, 'file_count': 1,
                        'public_models_roster_sha256': '3' * 64,
                        'files': [{'path': name, 'sha256': '4' * 64} for name in
                                  ('Cargo.toml', 'Cargo.lock', 'crates/airk-cli/src/main.rs', 'scripts/dev-check.py')]}
            manifest['files'].extend({'path': name, 'sha256': '5' * 64}
                                     for name in ('scripts/readme.md', 'scripts/fixture.sh'))
            (package / 'source-manifest.json').write_text(json.dumps(manifest), encoding='utf-8')
            for name in ('transfer-receipt.json', 'restore-neo-source.py'):
                (package / name).write_text('fixture', encoding='utf-8')
            for name in ('transfer-and-launch.py', 'neo-scoped-owner.py'):
                (base / name).write_text('fixture', encoding='utf-8')
            check = root / 'clean-break-r35-native-focused.json'
            check.write_text(json.dumps({'exit_code': 0, 'native_inputs_unchanged': True,
                                         'source_files': {row['path']: row['sha256'] for row in manifest['files'] if not row['path'].startswith('scripts/')},
                                         'script_sources': {'scripts/dev-check.py': '4' * 64}}), encoding='utf-8')
            with patch.object(DRAFT, 'BASE', base):
                pins = DRAFT.draft_pins(package, check)
                self.assertFalse(pins['primary_reviewed'])
                self.assertFalse(pins['independent_reviewed'])
                self.assertEqual(pins['reviewers'], {'primary': None, 'independent': None})
                self.assertEqual(pins['windows_script_source_suffixes'], ['.py', '.cjs', '.ps1', '.json'])
                with self.assertRaises(ValueError):
                    TRANSFER.validate_pins(pins)
                for revision in (36, 37, 38, 137):
                    current_check = root / f'clean-break-r{revision}-native-focused.json'
                    current_check.write_text(check.read_text(encoding='utf-8'), encoding='utf-8')
                    with self.subTest(receipt_revision=revision):
                        current_pins = DRAFT.draft_pins(package, current_check)
                        self.assertEqual(current_pins['windows_check_receipt'], str(current_check))
                        self.assertFalse(current_pins['primary_reviewed'])
                        self.assertFalse(current_pins['independent_reviewed'])
                        with self.assertRaises(ValueError):
                            TRANSFER.validate_pins(current_pins)
                        mismatched = json.loads(check.read_text(encoding='utf-8'))
                        mismatched['source_files']['Cargo.toml'] = 'f' * 64
                        current_check.write_text(json.dumps(mismatched), encoding='utf-8')
                        with self.assertRaises(ValueError):
                            DRAFT.draft_pins(package, current_check)
                for revision in ('34', '32', 'future', '37x', '035'):
                    historical = root / f'clean-break-r{revision}-native-focused.json'
                    historical.write_text(check.read_text(encoding='utf-8'), encoding='utf-8')
                    with self.subTest(rejected_revision=revision), self.assertRaises(ValueError):
                        DRAFT.draft_pins(package, historical)
                manifest['final_freeze_required'] = True
                (package / 'source-manifest.json').write_text(json.dumps(manifest), encoding='utf-8')
                with self.assertRaises(ValueError):
                    DRAFT.draft_pins(package, check)

    def test_draft_pins_reject_changed_missing_or_extra_windows_input_rosters(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            base = root / 'clean-break-r35-neo'
            package = base / 'neo-frozen-transfer-r35-fixture'
            package.mkdir(parents=True)
            native = {name: '1' * 64 for name in ('Cargo.toml', 'Cargo.lock', 'crates/airk-cli/src/main.rs')}
            scripts = {'scripts/dev-check.py': '2' * 64}
            manifest = {'status': 'FROZEN_PUBLIC_SOURCE', 'final_freeze_required': False,
                        'native_source': {'sha256': '3' * 64, 'files': 3},
                        'source_roster_sha256': '4' * 64, 'file_count': 4,
                        'public_models_roster_sha256': '5' * 64,
                        'files': [{'path': name, 'sha256': digest} for name, digest in {**native, **scripts}.items()]}
            (package / 'source-manifest.json').write_text(json.dumps(manifest), encoding='utf-8')
            for name in ('transfer-receipt.json', 'restore-neo-source.py'):
                (package / name).write_text('fixture', encoding='utf-8')
            for name in ('transfer-and-launch.py', 'neo-scoped-owner.py'):
                (base / name).write_text('fixture', encoding='utf-8')
            check = root / 'clean-break-r35-native-focused.json'
            for field, expected in (('source_files', native), ('script_sources', scripts)):
                for change in ('changed', 'missing', 'extra', 'absent'):
                    result = {'exit_code': 0, 'native_inputs_unchanged': True,
                              'source_files': dict(native), 'script_sources': dict(scripts)}
                    key = next(iter(expected))
                    if change == 'changed':
                        result[field][key] = 'f' * 64
                    elif change == 'missing':
                        del result[field][key]
                    elif change == 'extra':
                        result[field]['crates/extra.rs' if field == 'source_files' else 'scripts/extra.py'] = 'e' * 64
                    else:
                        del result[field]
                    check.write_text(json.dumps(result), encoding='utf-8')
                    with self.subTest(field=field, change=change), patch.object(DRAFT, 'BASE', base):
                        with self.assertRaises(ValueError):
                            DRAFT.draft_pins(package, check)

    def test_public_export_admits_installers_and_refuses_private_names(self):
        self.assertIn('installers', EXPORT.PUBLIC)
        self.assertEqual(EXPORT.safe_name('installers/windows/AIRK.iss'), 'installers/windows/AIRK.iss')
        for name in ['temp/source.rs', 'artifacts/result.json', '.codex/config.toml', '.mcp.json',
                     'docs/source.sqlite3', 'docs/private.key', 'scripts/.env', 'packages/node_modules/index.js']:
            with self.subTest(name=name), self.assertRaises(ValueError):
                EXPORT.safe_name(name)

    def test_no_optimized_or_unreviewed_launcher_can_proceed(self):
        template = json.loads((HERE / 'approved-pins.template.json').read_text(encoding='utf-8'))
        with patch.object(TRANSFER.subprocess, 'run', side_effect=AssertionError('No process is permitted')):
            with self.assertRaises(ValueError):
                TRANSFER.validate_pins(template)
            with self.assertRaises(ValueError):
                TRANSFER.main(['--approved-pins', str(HERE / 'approved-pins.template.json')])

    def test_mocked_launch_consumes_verified_head_and_package_roster_before_all_stages(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            package = base / 'neo-frozen-transfer-r35-fixture'
            package.mkdir()
            native = {'sha256': '1' * 64, 'files': 2}
            roster, models, head = '2' * 64, '3' * 64, '4' * 40
            manifest = {'status': 'FROZEN_PUBLIC_SOURCE', 'final_freeze_required': False,
                        'native_source': native, 'file_count': 3, 'source_roster_sha256': roster,
                        'public_models_roster_sha256': models, 'head': head}
            payloads = {'public-source.tar.gz': b'fictional bounded source archive',
                        'public-models.tar.gz': b'fictional public model archive',
                        'authentic-head.bundle': b'fictional public git bundle',
                        'source-manifest.json': json.dumps(manifest).encode(),
                        'public-model-transfer-manifest.json': b'{}',
                        'restore-neo-source.py': b'# fictional restore; never executed\n'}
            for name, data in payloads.items():
                (package / name).write_bytes(data)
            runner = base / 'neo-scoped-owner.py'
            runner.write_bytes(b'# fictional owner; never executed\n')
            transfer_receipt = {'status': 'FROZEN_PUBLIC_SOURCE', 'native_source': native,
                                'files': {name: {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
                                          for name, data in payloads.items()}}
            (package / 'transfer-receipt.json').write_text(json.dumps(transfer_receipt), encoding='utf-8')
            calls = []

            def no_network_run(argv, **kwargs):
                calls.append((argv, kwargs.get('input')))
                if argv[0] not in ('ssh', 'scp'):
                    raise AssertionError('Unexpected executable in prepared launch')
                script = kwargs.get('input') or ''
                if argv[0] == 'ssh':
                    self.assertEqual(argv[-2:], ['-I', '-'])
                    compile(script, '<mocked isolated remote snippet>', 'exec')
                if 'Authentic public source restore failed' in script:
                    self.assertIn(head, script)
                    output = json.dumps({'schema': 1, 'pid': 900, 'pgid': 900,
                                         'campaign': TRANSFER.ROOT + '/airk-clean-break-r35-final-123',
                                         'native_source': native})
                else:
                    output = '{}'
                return SimpleNamespace(returncode=0, stdout=output, stderr='')

            with patch.multiple(TRANSFER, BASE=base, RUNNER_BASE=base, PACKAGE=package,
                                RECEIPT=TRANSFER.sha(package / 'transfer-receipt.json'),
                                RESTORER=TRANSFER.sha(package / 'restore-neo-source.py'),
                                NATIVE=native, ROSTER=roster, PINNED_SOURCE_FILE_COUNT=3,
                                MODELS_ROSTER=models, RUNNERS={'neo-scoped-owner.py': TRANSFER.sha(runner)},
                                PINS_SHA256='5' * 64, create=True), \
                    patch.object(TRANSFER.time, 'time_ns', return_value=123), \
                    patch.object(TRANSFER.subprocess, 'run', side_effect=no_network_run):
                verified = TRANSFER.verify_local_package()
                self.assertEqual(verified['head'], head)
                self.assertEqual(set(verified['package_files']), set(payloads))
                TRANSFER.launch()
            receipt = json.loads((base / 'neo-transfer-r35-123/owner-receipt.json').read_text(encoding='utf-8'))
            expected_stages = ['read-only-host-readiness', 'create-owned-destinations',
                               *['copy-' + name for name in sorted(set(payloads) | {'transfer-receipt.json'})],
                               'copy-neo-scoped-owner.py', 'verify-restore-launch']
            self.assertEqual([row['stage'] for row in receipt['steps']], expected_stages)
            self.assertEqual(len(calls), len(expected_stages))
            self.assertEqual([argv[0] for argv, _script in calls].count('ssh'), 3)
            self.assertTrue(receipt['build_started'])
            self.assertEqual(receipt['source_transfer'], 'verified_and_restored')

    def test_nextest_counts_nonzero_real_cases_and_rejects_failure_or_vacuity(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'junit.xml'
            path.write_text('<testsuites><testsuite><testcase name="real"/><testcase name="skip"><skipped/></testcase></testsuite></testsuites>', encoding='utf-8')
            self.assertEqual(OWNER.require_junit(path)['executed'], 1)
            for value in ['<testsuites/>', '<testsuites><testcase name="skip"><skipped/></testcase></testsuites>',
                          '<testsuites><testcase name="failed"><failure/></testcase></testsuites>']:
                path.write_text(value, encoding='utf-8')
                with self.assertRaises(ValueError):
                    OWNER.require_junit(path)

    def test_python_and_node_counts_are_actual_not_assumed(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'suite.log'
            path.write_text('Ran 32 tests in 0.3s\n\nOK (skipped=1)\n', encoding='utf-8')
            self.assertEqual(OWNER.require_python_count(path)['selected'], 32)
            path.write_text('Ran 0 tests in 0.0s\nOK\n', encoding='utf-8')
            with self.assertRaises(ValueError):
                OWNER.require_python_count(path)
            for prefix in ('#', '\u2139'):
                path.write_text('\n'.join(prefix + ' ' + text for text in ['tests 12', 'pass 11', 'fail 0', 'skipped 1', 'cancelled 0']), encoding='utf-8')
                self.assertEqual(OWNER.require_node_count(path)['pass'], 11)
            path.write_text('# tests 0\n# pass 0\n# fail 0\n# skipped 0\n# cancelled 0\n', encoding='utf-8')
            with self.assertRaises(ValueError):
                OWNER.require_node_count(path)

    def test_collector_only_allows_explicit_reports(self):
        for name in ['batch-receipt.json', 'nextest.toml', 'nextest-store/ci/junit.xml',
                     'full-python-contracts.log', 'cargo-timings/cargo-timing-2026.html',
                     'cargo-timings/scoped-four-crate-clippy-latest.html']:
            with self.subTest(name=name):
                self.assertTrue(COLLECT.allowed(name, ['full-python-contracts']))
        for name in ['fixtures/state.sqlite3', 'issuer-rotation-private/key.pem',
                     'fixture-cleanup-receipts/0.json', '../batch-receipt.json',
                     '/batch-receipt.json', 'C:private.log', 'cargo-timings/../../private.html', 'source/Cargo.toml']:
            with self.subTest(name=name):
                self.assertFalse(COLLECT.allowed(name, []))

    def test_final_qualification_includes_cleanup_descendant_observation(self):
        def passing():
            return {'result': 'passed', 'unavailable': [], 'evidence_preserved_before_clean': True,
                    'cargo_cleanup': 'completed',
                    'commands': [{'stage': name, 'result': 'passed', 'owned_command_started': True,
                                  'owned_descendants_settled': True} for name in OWNER.REQUIRED_GATES]}
        state = passing()
        state['commands'].append({'stage': 'required-neo-cargo-clean', 'result': 'passed',
                                  'owned_command_started': True, 'owned_descendants_settled': True})
        OWNER.finalize_qualification(state)
        self.assertTrue(state['qualification_complete'])
        self.assertTrue(state['all_owned_command_descendants_settled'])
        for change in ('unsettled_cleanup', 'failed_cleanup', 'missing_gate', 'unavailable'):
            state = passing()
            if change == 'unsettled_cleanup':
                state['commands'].append({'stage': 'required-neo-cargo-clean', 'result': 'failed',
                                          'owned_command_started': True, 'owned_descendants_settled': False})
                state['cargo_cleanup'] = 'failed'
            elif change == 'failed_cleanup':
                state['cargo_cleanup'] = 'failed'
            elif change == 'missing_gate':
                state['commands'].pop()
            else:
                state['unavailable'] = [{'component': 'mocked-extension', 'outcome': 'not run'}]
            OWNER.finalize_qualification(state)
            self.assertFalse(state['qualification_complete'], change)
            if change == 'unsettled_cleanup':
                self.assertFalse(state['all_owned_command_descendants_settled'])

    def test_startup_failure_is_terminal_collectable_and_never_cleans_unproven_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            campaign = root / 'airk-clean-break-r35-final-123'
            campaign.mkdir()
            target = campaign / 'cargo-target'
            target.mkdir()
            (target / 'unrelated').write_text('preserve', encoding='utf-8')
            restore = {'status': 'RESTORED_FROZEN_PUBLIC_SOURCE', 'native_source': {'sha256': '1' * 64, 'files': 1}}
            (campaign / 'restore-receipt.json').write_text(json.dumps(restore), encoding='utf-8')
            argv = ['--campaign', str(campaign), '--native-sha256', '1' * 64]
            with patch.object(OWNER, 'ROOT', root), patch.object(OWNER.os, 'getuid', return_value=501, create=True), \
                    patch.object(OWNER, 'main', side_effect=ValueError('startup guard failed')), \
                    contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(OWNER.entrypoint(argv), 1)
            receipt = json.loads((campaign / 'evidence/batch-receipt.json').read_text(encoding='utf-8'))
            self.assertEqual(receipt['result'], 'failed')
            self.assertTrue(receipt['startup_failure'])
            self.assertFalse(receipt['qualification_complete'])
            self.assertEqual(receipt['cargo_cleanup'], 'blocked_startup_target_ownership_unproven')
            self.assertEqual((campaign / 'phase.txt').read_text(encoding='utf-8'), 'complete-failed')
            self.assertEqual((target / 'unrelated').read_text(encoding='utf-8'), 'preserve')

    def test_mocked_collector_copies_terminal_startup_receipt_and_root_owner_streams(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            campaign = root / 'airk-clean-break-r35-final-123'
            evidence = campaign / 'evidence'
            evidence.mkdir(parents=True)
            state = {'result': 'failed', 'commands': [], 'cargo_cleanup': 'no-owned-compiler-target-created',
                     'all_owned_command_descendants_settled': False, 'qualification_complete': False}
            (evidence / 'batch-receipt.json').write_text(json.dumps(state), encoding='utf-8')
            (evidence / 'owner-entry-failure.json').write_text(json.dumps({'result': 'failed'}), encoding='utf-8')
            (campaign / 'phase.txt').write_text('complete-failed', encoding='utf-8')
            (campaign / 'unsigned-owner.stdout').write_text('bounded startup output', encoding='utf-8')
            (campaign / 'unsigned-owner.stderr').write_text('bounded startup error', encoding='utf-8')
            active = root / 'active.json'
            active.write_text(json.dumps({'campaign': campaign.as_posix()}), encoding='utf-8')
            copied = []

            def no_network_run(argv, **kwargs):
                if argv[0] == 'ssh':
                    self.assertEqual(argv[-2:], ['-I', '-'])
                    capture = io.StringIO()
                    with contextlib.redirect_stdout(capture):
                        exec(compile(kwargs['input'], '<mocked read-only report inventory>', 'exec'), {})
                    return SimpleNamespace(returncode=0, stdout=capture.getvalue(), stderr='')
                self.assertEqual(argv[0], 'scp')
                source = Path(argv[-2].split(':', 1)[1])
                self.assertTrue(source.resolve().is_relative_to(root))
                shutil.copyfile(source, Path(argv[-1]))
                copied.append(source.name)
                return SimpleNamespace(returncode=0, stdout='', stderr='')

            destination = root / 'collected'
            with patch.object(COLLECT, 'HERE', root), patch.object(COLLECT.TRANSFER, 'ROOT', root.as_posix()), \
                    patch.object(COLLECT.subprocess, 'run', side_effect=no_network_run), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(COLLECT.main(['--active-receipt', str(active), '--output', str(destination), '--execute']), 0)
            self.assertIn('unsigned-owner.stdout', copied)
            self.assertIn('unsigned-owner.stderr', copied)
            self.assertEqual((destination / 'owner.stdout').read_text(encoding='utf-8'), 'bounded startup output')
            self.assertEqual((destination / 'owner.stderr').read_text(encoding='utf-8'), 'bounded startup error')
            collected = json.loads((destination / 'collection-receipt.json').read_text(encoding='utf-8'))
            self.assertTrue(collected['owner_entry_failure'])
            self.assertEqual(collected['result'], 'failed')


if __name__ == '__main__':
    unittest.main()
