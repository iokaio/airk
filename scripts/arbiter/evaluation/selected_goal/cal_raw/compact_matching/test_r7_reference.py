# SPDX-License-Identifier: Apache-2.0
"""Model-free checks for R7 CPU reference admission and provenance."""
from pathlib import Path
from types import SimpleNamespace
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_reference as owner


class R7ReferenceAdmission(unittest.TestCase):
    def test_wrong_or_duplicate_worker_placement_refuses_before_loading_science(self):
        for cpus, priority in (([5, 5], 0x4000), ([5, 7], 0x20), ([5], 0x4000)):
            with (
                self.subTest(cpus=cpus, priority=priority),
                patch.object(owner, "place_optional_worker", return_value={"logical_cpus": cpus, "priority_class": priority}),
                patch.object(owner, "owners") as load,
            ):
                with self.assertRaisesRegex(ValueError, "two quiet physical cores"):
                    owner.checked_science([], "protected_matching", {}, 600.0, 17, None, {}, {})
                load.assert_not_called()

    def test_owned_cpu_probe_starts_a_180_second_absolute_deadline(self):
        for diagnostic, deadline in ((True, 207.0), (False, None)):
            with (
                self.subTest(diagnostic=diagnostic),
                patch.object(owner.time, "monotonic", return_value=27.0),
                patch.object(owner, "verified_runtime", side_effect=ValueError("clock recorded")) as runtime,
            ):
                with self.assertRaisesRegex(ValueError, "clock recorded"):
                    owner.owned(SimpleNamespace(cpu_probe=diagnostic), [], {}, {})
                runtime.assert_called_once_with(deadline)

    def test_worker_rejects_a_probe_clock_longer_than_180_seconds(self):
        argv = ["r7_reference.py", "--cpu-probe", "--first", "0", "--count", "1",
                "--role", "publisher_fp32", "--cal-pid", "17", "--source-set", "{}"]
        for name in ("projection", "compiler", "graph"):
            argv.extend(["--" + name, "unused", "--" + name + "-sha256", "unused"])
        for deadline, error in ((207.0, "worker clock accepted"), (207.01, "foreign parent source proof")):
            with (
                self.subTest(deadline=deadline),
                patch.object(sys, "argv", [*argv, "--worker-deadline", str(deadline)]),
                patch.object(owner, "inputs", return_value=([], {})),
                patch.object(owner, "source_set", return_value={}),
                patch.object(owner.time, "monotonic", return_value=27.0),
                patch.object(owner, "verified_runtime", side_effect=ValueError("worker clock accepted")),
                self.assertRaisesRegex(ValueError, error),
            ):
                owner.main()

    def test_attempt_worker_does_not_receive_a_fresh_reference_clock(self):
        with patch.object(owner.time, "monotonic", return_value=27.0):
            owner.check_deadline(27.01)
            with self.assertRaisesRegex(ValueError, "Job deadline expired"):
                owner.check_deadline(27.0)

    def test_reference_identity_preserves_original_execution_and_graph(self):
        provenance = {
            "model_sha256": "a" * 64, "weight_sha256": "b" * 64, "matching_recipe_sha256": "c" * 64,
            "projection": {"sha256": "d" * 64},
        }
        result = {
            "provenance": provenance, "qualification": False, "candidate_graph": {"sha256": "a" * 64},
            "candidate_weights": {"sha256": "b" * 64}, "matching_recipe_sha256": "c" * 64,
        }
        with patch.object(owner, "reference_identity") as historical:
            owner.verify_identity(result, provenance)
            historical.assert_called_once_with(result)
            for bad in ({"qualification": True}, {"candidate_weights": {"sha256": "e" * 64}},
                        {"provenance": provenance | {"projection": {"sha256": "f" * 64}}}):
                with self.subTest(bad=bad), self.assertRaisesRegex(ValueError, "same protected identity"):
                    owner.verify_identity(result | bad, provenance)


if __name__ == "__main__":
    unittest.main()
