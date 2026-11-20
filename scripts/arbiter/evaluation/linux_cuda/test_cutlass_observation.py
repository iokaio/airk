# SPDX-License-Identifier: Apache-2.0
"""Exercise the protected GNU graph result checker without opening a native runtime."""
import unittest

from cutlass_observation import LIMITS, SCIENTIFIC, confirm
from cutlass_references import CAMPAIGN, document


class GraphEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        host, _ = LIMITS["compact"]
        packet = document(CAMPAIGN / "compact-cutlass-width-graph-shared-r1/execution.json",
                          "3c6824e609c94722b815458bece068c7ddc25104c55eea0f50f3d445d2c19752")
        # Use historical typed rows as parser input only. This synthetic envelope makes no GNU observations.
        native = packet["result"].copy()
        native.update(cal=None, cal_native_forwards=0, timing=None, pressure=None)
        cls.driver = native["adapter"]["driver_image_sha256"]
        cls.sample = {"native": native, "pid": 81, "uid": 1000, "descriptor_sha256": SCIENTIFIC,
                      "private_elf_bytes": 12288, "host_limit": host, "address_space": host - 12288,
                      "control_reacquired": True, "native_forwards": 0, "qualification": False}

    def test_complete_graph_parser(self):
        confirm(self.sample, 81, 12288, self.driver, b"", "compact")

    def test_wrong_session_or_device_evidence(self):
        for key, value in [("pid", 82), ("control_reacquired", False), ("native_forwards", 1)]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                confirm({**self.sample, key: value}, 81, 12288, self.driver, b"", "compact")
        with self.assertRaises(ValueError):
            confirm(self.sample, 81, 12288, "0" * 64, b"", "compact")
        with self.assertRaises(ValueError):
            confirm(self.sample, 81, 12288, self.driver, b"SdpaKernel=EFFICIENT_ATTENTION", "compact")

    def test_route_or_graph_evidence_required(self):
        for key, value in [("route_refusal", "wrong graph"), ("score", [1]), ("cal_native_forwards", 1),
                           ("graph", {"complete": False, "rows": []}),
                           ("placement", {"complete": False, "nodes": []})]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                confirm({**self.sample, "native": {**self.sample["native"], key: value}},
                        81, 12288, self.driver, b"", "compact")

    def test_large_direct_graph_and_width_limits(self):
        packet = document(CAMPAIGN / "windows-large-direct-fp32-graph-r1/execution.json",
                          "27026771776fc7a3e0e05ef1130dbedece712767819676be79ad07857ccd4160")
        native = packet["result"]
        host, _ = LIMITS["large"]
        sample = {**self.sample, "host_limit": host, "address_space": host - 12288, "native": native}
        row = confirm(sample, 81, 12288, native["adapter"]["driver_image_sha256"], b"", "large")
        self.assertIsNone(row["score"])
        with self.assertRaises(ValueError):
            confirm(sample, 81, 12288, self.driver, b"", "compact")
        corrupted = {**native, "result": {**native["result"], "model_sha256": "0" * 64}}
        with self.assertRaises(ValueError):
            confirm({**sample, "native": corrupted}, 81, 12288, self.driver, b"", "large")
