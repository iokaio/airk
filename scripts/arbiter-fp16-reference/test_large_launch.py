# SPDX-License-Identifier: Apache-2.0
"""Large reference launch controls. The Windows Job control runs no tensor code."""

import json
import os
from pathlib import Path
import runpy
import sys
import time
import unittest

HERE = Path(__file__).resolve()
M = runpy.run_path(str(HERE.with_name("run_large_t32.py")))


class LargeLaunchTests(unittest.TestCase):
    def test_reject_changed_owner_and_frozen_source(self):
        with self.assertRaisesRegex(ValueError, "maintained source changed"):
            M["checked_module"](M["SUPPORT"], "0" * 64)
        self.assertIn("prepare", M["checked_module"](M["PREP"], M["PREP_SHA"]))
        self.assertIn("pinned", M["checked_module"](M["CONVERSION"], M["CONVERSION_SHA"]))

    @unittest.skipUnless(os.name == "nt", "Windows Job contract")
    def test_genuine_suspended_process_observes_both_original_caps(self):
        c = M["checked_module"](M["CONVERSION"], M["CONVERSION_SHA"])
        token = c["windows_owner"]()
        support = M["checked_module"](M["SUPPORT"], M["SUPPORT_SHA"])
        helpers = M["ROOT"] / "scripts/cuda-native-execution/helpers"
        for name, digest in c["HELPERS"].items():
            self.assertEqual(c["pin"](helpers / name)["sha256"], digest)
        sys.path.insert(0, str(helpers))
        try:
            parent = support["load_module"](helpers / "reference_parent_no_site_r2.py", "large_test_parent")
            owner = support["load_module"](helpers / "reference_owned.py", "large_test_owner")
        finally:
            sys.path.pop(0)
        self.assertEqual(Path(parent.worker.__file__), helpers / "reference_worker.py")
        image = c["pin"](sys.executable)
        deadline = time.monotonic() + 30

        def suspended(api, job, info, stamp):
            observed = parent.suspended_authentication(api, info, deadline, stamp, c["SID"], image, owner.image_proof)
            self.assertEqual(c["windows_owner"](process=info.process), token)
            self.assertEqual(owner.job_snapshot(api, job), ([int(info.pid)], 1, 1))
            return dict(observed, token=token)

        cmd = [sys.executable, "-IBS", "-c",
               "import json,runpy,sys; print(json.dumps(runpy.run_path(sys.argv[1])['own_job']()))", str(M["HERE"])]
        read_fd, write_fd = os.pipe()
        with os.fdopen(read_fd, "rb") as source, open(os.devnull, "rb") as stdin:
            with os.fdopen(write_fd, "wb") as output:
                done = owner.run_windows_owned_training(cmd, M["ROOT"], output, deadline - time.monotonic(),
                         dict(os.environ, CUDA_VISIBLE_DEVICES=""), stdin, M["HOST"], before_resume=suspended,
                         absolute_deadline=deadline, cleanup_seconds=5)
            self.assertTrue(parent.settled(done), done)
            self.assertEqual(json.loads(source.read(2048)), {"process_bytes": 32 << 30,
                             "job_bytes": 32 << 30, "active_limit": 2})
        self.assertGreater(done["peak_host_committed_bytes"], 0)
        self.assertLess(done["peak_host_committed_bytes"], M["HOST"])


if __name__ == "__main__":
    unittest.main()
