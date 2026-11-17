# SPDX-License-Identifier: Apache-2.0
import copy
import json
from pathlib import Path
import unittest
import prepare_packets as owner

class RetainedBoundaryTests(unittest.TestCase):
    def test_actual_latest_closure_then_live_or_drift_refuses(self):
        actual = json.loads((owner.BASE / 'fixture-restart-r136-owned-actual/execution.json').read_bytes())
        owner.require_closed(actual)
        changed = copy.deepcopy(actual)
        changed['owned']['owned_job_final_active_processes'] = 1
        with self.assertRaises(ValueError):
            owner.require_closed(changed)
        changed = copy.deepcopy(actual)
        changed['post_input_drift'] = ['changed-source']
        with self.assertRaises(ValueError):
            owner.require_closed(changed)

    def test_actual_science_then_model_floor_or_unapproved_refuses(self):
        actual = json.loads((owner.BASE / 'CAL-capture-release.root-approved-corrected-r137.json').read_bytes())
        owner.same_science(actual, actual)
        for key, value in [('Focus_profile', 'foreign'), ('maximum_probes', 501), ('approved', None)]:
            changed = copy.deepcopy(actual)
            changed[key] = value
            with self.assertRaises(ValueError):
                owner.same_science(changed, actual)

if __name__ == '__main__':
    unittest.main()
