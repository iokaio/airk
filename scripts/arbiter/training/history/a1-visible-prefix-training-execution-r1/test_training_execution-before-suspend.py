# SPDX-License-Identifier: Apache-2.0
"""Real owned Windows child/JobObject only; no Torch/model/GPU operation."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

import training_execution as execution


@unittest.skipUnless(os.name == 'nt', 'owning Windows training host only')
class TrainingExecutionTests(unittest.TestCase):
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
            child.write_text(script, encoding='utf-8')
            gpu = {'uuid': 'SYNTHETIC', 'name': 'SYNTHETIC', 'driver_version': 'synthetic',
                   'compute_capability': [8, 6], 'free_mib': 32000}
            seal = {'launch_log_dir': str(work / 'logs'), 'nvidia_smi': 'NOT_EXECUTED', 'gpu': gpu,
                    'limits': {'wall_seconds': deadline, 'host_committed_bytes': 1024**3,
                               'cuda_allocator_bytes': 1024**3, 'minimum_gpu_free_mib': 8192},
                    'environment': {}, 'command': [sys.executable, '-B', str(child)], 'cwd': str(work)}
            path = work / 'seal.json'
            path.write_text(json.dumps(seal), encoding='utf-8')
            args = types.SimpleNamespace(seal=path, seal_sha256=hashlib.sha256(path.read_bytes()).hexdigest(), log_dir=work / 'logs')
            with patch.object(execution, 'gpu_observation', return_value=gpu):
                try:
                    execution.launch(args)
                except RuntimeError:
                    pass
            receipt = json.loads((work / 'logs/execution-receipt.json').read_text())
            stdout = (work / 'logs/stdout.log').read_text()
            if receipt['status'] == 'failed':
                diagnostic = {'receipt': receipt, 'stdout': stdout, 'stderr': (work / 'logs/stderr.log').read_text()}
                retained = root / 'artifacts/advanced-split/models/a1-visible-prefix-training-execution-r1'
                retained.mkdir(parents=True, exist_ok=True)
                (retained / ('ownership-child-failure-' + str(deadline) + '.json')).write_text(json.dumps(diagnostic, indent=2), encoding='utf-8')
                print(json.dumps(diagnostic))
            return receipt, stdout

    def test_gate_and_actual_job_allow_only_owned_bounded_child(self):
        receipt, stdout = self.run_child('import sys\ngate=sys.stdin.readline()\nassert gate.startswith("GO ")\nprint("GATE_OBSERVED",flush=True)\n', 10)
        self.assertEqual(receipt['status'], 'completed')
        self.assertTrue(receipt['child_exit_observed'])
        self.assertFalse(receipt['other_processes_touched'])
        self.assertIn('GATE_OBSERVED', stdout)

    def test_deadline_terminates_owned_waiting_child_and_records_observed_exit(self):
        receipt, _ = self.run_child('import sys,threading\nsys.stdin.readline()\nprint("WAITING",flush=True)\nthreading.Event().wait()\n', .2)
        self.assertEqual(receipt['status'], 'deadline_exhausted')
        self.assertTrue(receipt['child_exit_observed'])
        self.assertFalse(receipt['other_processes_touched'])


if __name__ == '__main__':
    unittest.main()
