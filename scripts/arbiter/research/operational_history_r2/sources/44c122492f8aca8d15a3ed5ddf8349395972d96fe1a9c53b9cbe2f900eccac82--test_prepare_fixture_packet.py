# SPDX-License-Identifier: Apache-2.0
import copy
import json
from pathlib import Path
import unittest
import prepare_fixture_packet as owner

class RetainedBoundaryTests(unittest.TestCase):
    def test_actual_latest_closure_then_live_or_drift_refuses(self):
        actual = json.loads((owner.BASE / 'fixture-next-corrected-r141-owned-actual/execution.json').read_bytes())
        owner.require_closed(actual)
        changed = copy.deepcopy(actual)
        changed['owned']['owned_job_final_active_processes'] = 1
        with self.assertRaises(ValueError):
            owner.require_closed(changed)
        changed = copy.deepcopy(actual)
        changed['post_input_drift'] = ['changed-source']
        with self.assertRaises(ValueError):
            owner.require_closed(changed)

if __name__ == '__main__':
    unittest.main()
