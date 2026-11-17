# SPDX-License-Identifier: Apache-2.0
"""Read-only source relocation/import controls; no subprocesses or fixtures."""
import hashlib
import importlib.util
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('cuda_helper_relocation', ROOT / 'scripts/cuda-native-execution.py')
N = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(N)
EXPECTED = {
    'reference_owned.py': '2d7a31a64eaba745e32ed83adfbeedb75983ab6f9b5c4cc82714e9371c1895a1',
    'reference_parent_no_site_r2.py': 'fe4cc990649d92e041b79cccb3fd7ef1129979a26432b7f02b2d8d28b586ed8d',
    'reference_worker.py': '9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7',
}

class HelperSourceControls(unittest.TestCase):
    def plan(self):
        return {'ownership_helper_root': str(ROOT / 'scripts/cuda-native-execution/helpers')}

    def test_exact_historical_source_bytes_and_real_module_location(self):
        self.assertEqual(N.HELPERS, EXPECTED)
        self.assertEqual(N.MAINTAINED_HELPER_ROOT, Path(self.plan()['ownership_helper_root']))
        for name, sha in EXPECTED.items():
            self.assertEqual(hashlib.sha256((N.MAINTAINED_HELPER_ROOT / name).read_bytes()).hexdigest(), sha)

    def test_actual_imports_use_tracked_siblings_without_search_path_mutation(self):
        before = list(sys.path)
        paths = []
        original = N.load
        def observed(path, name):
            paths.append(Path(path))
            return original(path, name)
        with patch.object(N, 'load', side_effect=observed):
            parent, owner = N.helpers(self.plan())
        self.assertEqual(paths, [N.MAINTAINED_HELPER_ROOT / name for name in
                                ['reference_worker.py', 'reference_parent_no_site_r2.py', 'reference_owned.py']])
        self.assertEqual(Path(parent.worker.__file__), N.MAINTAINED_HELPER_ROOT / 'reference_worker.py')
        self.assertEqual(Path(owner.__file__), N.MAINTAINED_HELPER_ROOT / 'reference_owned.py')
        self.assertEqual(sys.path, before)

    def test_preexisting_shadow_worker_is_never_used_and_is_restored(self):
        sentinel = SimpleNamespace(__file__='foreign-worker.py')
        with patch.dict(sys.modules, {'reference_worker': sentinel}):
            parent, _ = N.helpers(self.plan())
            self.assertIsNot(parent.worker, sentinel)
            self.assertIs(sys.modules['reference_worker'], sentinel)

    def test_absent_worker_registration_remains_absent_after_import(self):
        with patch.dict(sys.modules):
            sys.modules.pop('reference_worker', None)
            N.helpers(self.plan())
            self.assertNotIn('reference_worker', sys.modules)

    def test_retained_declared_sources_are_authenticated_but_never_imported(self):
        history = ROOT / 'artifacts/literal-historical-helper-root'
        checked = []
        worker = SimpleNamespace(__file__=N.MAINTAINED_HELPER_ROOT / 'reference_worker.py')
        parent = SimpleNamespace(worker=worker)
        loaded = []
        def digest(path):
            checked.append(Path(path))
            return EXPECTED[Path(path).name]
        def load(path, name):
            loaded.append(Path(path))
            return worker if Path(path).name == 'reference_worker.py' else parent if 'parent' in Path(path).name else object()
        with patch.object(N, 'regular', side_effect=Path), patch.object(N, 'digest', side_effect=digest), patch.object(N, 'load', side_effect=load):
            N.helpers({'ownership_helper_root': str(history)})
        self.assertTrue(all(history / name in checked for name in EXPECTED))
        self.assertTrue(all(p.parent == N.MAINTAINED_HELPER_ROOT for p in loaded))

    def test_bad_maintained_bytes_refuse_before_import(self):
        with patch.object(N, 'digest', return_value='0' * 64), patch.object(N, 'load') as loaded:
            with self.assertRaisesRegex(ValueError, 'ownership primitive'):
                N.helpers(self.plan())
            loaded.assert_not_called()

    def test_import_failure_restores_existing_worker(self):
        sentinel = object()
        original = N.load
        def failed(path, name):
            if 'parent' in Path(path).name:
                raise RuntimeError('injected parent import failure')
            return original(path, name)
        with patch.dict(sys.modules, {'reference_worker': sentinel}), patch.object(N, 'load', side_effect=failed):
            with self.assertRaisesRegex(RuntimeError, 'injected parent'):
                N.helpers(self.plan())
            self.assertIs(sys.modules['reference_worker'], sentinel)

    def test_wrong_parent_worker_object_is_rejected(self):
        wrong = SimpleNamespace(worker=SimpleNamespace(__file__=N.MAINTAINED_HELPER_ROOT / 'reference_worker.py'))
        original = N.load
        def shadow(path, name):
            return wrong if 'parent' in Path(path).name else original(path, name)
        with patch.object(N, 'load', side_effect=shadow):
            with self.assertRaisesRegex(ValueError, 'shadowed'):
                N.helpers(self.plan())

    def test_real_relocated_settlement_preserves_missing_exit_refusal(self):
        parent, _ = N.helpers(self.plan())
        owned = {'exit_code': 0, 'timed_out': False, 'log_oversized': False,
                 'owned_descendants_settled': True, 'owned_job_forced_termination': False,
                 'owned_job_final_active_processes': 0, 'owned_job_final_member_pids': [],
                 'parent_assigned_to_job': False, 'child_assigned_before_resume': True,
                 'owned_total_processes': 1, 'unexpected_third_process': False,
                 'whole_process_deadline_not_refreshed': True,
                 'retained_member_exits': [{'exit_observed': True, 'exit_code': 0}],
                 'handle_close_proof': {'forced': False, 'settled': True, 'final_active': 0,
                                        'members_before': [], 'final_member_pids': []}}
        self.assertTrue(parent.settled(owned))
        owned['retained_member_exits'] = []
        self.assertFalse(parent.settled(owned))

if __name__ == '__main__':
    unittest.main()
