# SPDX-License-Identifier: Apache-2.0
"""Real owned Windows child/JobObject only; no Torch/model/GPU operation."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import types
import unittest
import venv
from unittest.mock import patch

import training_execution as execution


@unittest.skipUnless(os.name == 'nt', 'owning Windows training host only')
class TrainingExecutionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        root = Path(__file__).resolve().parents[2]
        (root / 'artifacts').mkdir(exist_ok=True)
        cls.base_python = Path(sys._base_executable).resolve()
        cls.child_python = Path(sys.executable).resolve()
        if sys.prefix == sys.base_prefix:
            # The Windows venv redirector and its actual base interpreter are
            # the two owned processes under test, including from base Python.
            temporary = tempfile.TemporaryDirectory(dir=root / 'artifacts')
            cls.addClassCleanup(temporary.cleanup)
            environment = Path(temporary.name) / 'venv'
            venv.EnvBuilder(with_pip=False, symlinks=False).create(environment)
            cls.child_python = environment / 'Scripts/python.exe'

    def test_runtime_roster_includes_source_and_excludes_disabled_bytecode(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'runtime.py').write_text('pass', encoding='utf-8')
            (root / '__pycache__').mkdir()
            (root / '__pycache__/runtime.pyc').write_bytes(b'not executed')
            self.assertEqual(execution.runtime_paths(root), {os.path.normcase(str(root / 'runtime.py'))})

    def run_child(self, script, deadline):
        root = Path(__file__).resolve().parents[2]
        with tempfile.TemporaryDirectory(dir=root / 'artifacts') as directory:
            work = Path(directory)
            child = work / 'synthetic.py'
            child.write_text('import os\nprint("AIRK_TRAIN_GATE_READY:"+str(os.getpid()),flush=True)\n' + script, encoding='utf-8')
            gpu = {'uuid': 'SYNTHETIC', 'name': 'SYNTHETIC', 'driver_version': 'synthetic',
                   'compute_capability': [8, 6], 'free_mib': 32000}
            seal = {'launch_log_dir': str(work / 'logs'), 'nvidia_smi': 'NOT_EXECUTED', 'gpu': gpu,
                    'limits': {'wall_seconds': deadline, 'host_committed_bytes': 1024**3,
                               'cuda_allocator_bytes': 1024**3, 'minimum_gpu_free_mib': 8192},
                    'environment': {}, 'command': [str(self.child_python), '-B', str(child)], 'cwd': str(work),
                    'file_roles': {name: {'path': str(Path(image).resolve()), 'sha256': hashlib.sha256(Path(image).read_bytes()).hexdigest()} for name, image in [('venv_launcher', self.child_python), ('base_interpreter', self.base_python)]}}
            path = work / 'seal.json'
            path.write_text(json.dumps(seal), encoding='utf-8')
            args = types.SimpleNamespace(seal=path, seal_sha256=hashlib.sha256(path.read_bytes()).hexdigest(), log_dir=work / 'logs')
            with patch.object(execution, 'gpu_observation', return_value=gpu):
                try:
                    execution.launch(args)
                except (RuntimeError, ValueError):
                    pass
            receipt = json.loads((work / 'logs/execution-receipt.json').read_text())
            stdout = (work / 'logs/stdout.log').read_text()
            retained = root / 'artifacts/advanced-split/models/a1-visible-prefix-training-execution-r1'
            retained.mkdir(parents=True, exist_ok=True)
            witness = retained / ('suspended-child-' + str(deadline) + '-' + str(time.time_ns()) + '.json')
            witness.write_text(json.dumps(receipt, indent=2), encoding='utf-8')
            print(json.dumps({'synthetic_ownership_witness': str(witness), 'sha256': hashlib.sha256(witness.read_bytes()).hexdigest()}))
            return receipt, stdout

    def test_gate_and_actual_job_allow_only_owned_bounded_child(self):
        receipt, stdout = self.run_child('import sys\ngate=sys.stdin.readline()\nassert gate.startswith("GO ")\nprint("GATE_OBSERVED",flush=True)\n', 10)
        owned = receipt['owned']
        self.assertTrue(owned['child_assigned_before_resume'])
        self.assertEqual(owned['suspended_assignment']['active'], 1)
        self.assertEqual(owned['before_GO']['active'], 2)
        self.assertEqual(owned['before_GO']['total'], 2)
        self.assertEqual(len(owned['before_GO']['members']), 2)
        self.assertTrue(owned['before_GO']['actual_handle_membership_verified_before_GO'])
        self.assertEqual(owned['owned_job_final_member_pids'], [])
        self.assertEqual(owned['owned_job_final_active_processes'], 0)
        self.assertEqual(owned['owned_total_processes'], 2)
        self.assertFalse(owned['owned_job_forced_termination'])
        self.assertEqual(len(owned['retained_member_exits']), 2)
        self.assertTrue(all(r['exit_observed'] and r['exit_code'] == 0 for r in owned['retained_member_exits']))
        self.assertFalse(receipt['other_processes_touched'])
        self.assertIn('GATE_OBSERVED', stdout)

    def test_deadline_terminates_owned_waiting_child_and_records_observed_exit(self):
        receipt, _ = self.run_child('import sys,threading\nsys.stdin.readline()\nprint("WAITING",flush=True)\nthreading.Event().wait()\n', .2)
        self.assertTrue(receipt['owned']['timed_out'])
        self.assertTrue(receipt['owned']['owned_job_forced_termination'])
        self.assertEqual(receipt['owned']['before_GO']['active'], 2)
        self.assertFalse(receipt['other_processes_touched'])

    def test_pre_go_rejection_after_actual_base_ready_observes_both_forced_exits(self):
        real = execution.image_proof
        def reject_base(api, handle, expected):
            value = real(api, handle, expected)
            if Path(expected['path']).resolve() == self.base_python:
                raise ValueError('synthetic pre-GO rejection after actual base READY')
            return value
        with patch.object(execution, 'image_proof', side_effect=reject_base):
            receipt, stdout = self.run_child('import sys\nsys.stdin.readline()\nprint("PAYLOAD_MUST_NOT_EXECUTE",flush=True)\n', 10)
        self.assertIsNone(receipt['owned'])
        proof = receipt['failure']['owned_cleanup']
        self.assertTrue(proof['forced'])
        self.assertTrue(proof['settled'])
        self.assertEqual(proof['final_member_pids'], [])
        self.assertEqual(proof['final_active'], 0)
        expected = set(proof['members_before'])
        observed = {r['pid'] for r in proof['retained_exits'] if r['exit_observed']}
        self.assertEqual(len(expected), 2)
        self.assertEqual(observed, expected)
        self.assertNotIn('PAYLOAD_MUST_NOT_EXECUTE', stdout)

    def test_suspended_assignment_precedes_any_child_code(self):
        root = Path(__file__).resolve().parents[2]
        with tempfile.TemporaryDirectory(dir=root / 'artifacts') as directory:
            work = Path(directory)
            marker = work / 'executed.txt'
            def before(api, job, info, deadline):
                members, active, total = execution.job_snapshot(api, job)
                self.assertEqual((members, active, total), ([info.pid], 1, 1))
                self.assertFalse(marker.exists())
                return {'checked_before_code': True}
            with open(os.devnull, 'rb') as stdin, (work / 'out.log').open('wb') as log:
                result = execution.run_windows_owned_training([str(self.child_python), '-B', '-c',
                    'from pathlib import Path;Path(' + repr(str(marker)) + ').write_text("executed")'], work, log, 10,
                    dict(os.environ), stdin, 1024**3, before_resume=before)
            self.assertTrue(marker.exists())
            self.assertTrue(result['suspended_assignment']['checked_before_code'])
            self.assertTrue(result['owned_descendants_settled'])
            self.assertFalse(result['owned_job_forced_termination'])
            self.assertEqual(result['owned_total_processes'], 2)

    def test_leader_exit_zero_with_event_blocked_descendant_cannot_pass(self):
        root = Path(__file__).resolve().parents[2]
        with tempfile.TemporaryDirectory(dir=root / 'artifacts') as directory:
            work = Path(directory)
            code = 'import subprocess,sys;subprocess.Popen([sys.executable,"-B","-c","import threading;threading.Event().wait()"]);print("leader exits",flush=True)'
            with open(os.devnull, 'rb') as stdin, (work / 'out.log').open('wb') as log:
                result = execution.run_windows_owned_training([sys._base_executable, '-B', '-c', code], work, log, 10,
                    dict(os.environ), stdin, 1024**3, cleanup_seconds=.2)
            self.assertEqual(result['exit_code'], 0)
            self.assertTrue(result['owned_job_forced_termination'])
            self.assertEqual(result['owned_total_processes'], 2)
            retained = root / 'artifacts/advanced-split/models/a1-visible-prefix-training-execution-r1'
            retained.mkdir(parents=True, exist_ok=True)
            (retained / 'blocked-descendant-refusal.json').write_text(json.dumps(result, indent=2), encoding='utf-8')


if __name__ == '__main__':
    unittest.main()
