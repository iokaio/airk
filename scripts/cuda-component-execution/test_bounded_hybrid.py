# SPDX-License-Identifier: Apache-2.0
"""Pure controls for the owned Compact CUDA profiling launcher."""

import copy
import importlib.util
import json
import os
from pathlib import Path
import time
import unittest


source = Path(__file__).with_name("bounded_hybrid.py")
spec = importlib.util.spec_from_file_location("bounded_hybrid_launch", source)
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class Controls(unittest.TestCase):
    def test_relink_cannot_change_the_static_authority(self):
        old = {"manifest": {"sha256": "old"}, "archive": {"sha256": "same"},
               "build_receipt": {"sha256": "first"}}
        new = copy.deepcopy(old)
        new["build_receipt"]["sha256"] = "second"
        self.assertEqual(launcher.plan_binding({"static_graph_binding": new}, old), new)
        new["archive"]["sha256"] = "different"
        with self.assertRaisesRegex(ValueError, "retained /6"):
            launcher.plan_binding({"static_graph_binding": new}, old)

    def test_retained_rows_mix_with_exact_supplemental_widths(self):
        def fixture(widths):
            return {"schema": "native", "model": "compact", "compiler": "compiler",
                    "resource_policy": "policy", "original_reference_assets": ["asset"],
                    "rows": [{"input": {"input_ids": [1] * width}, "width": width}
                             for width in widths]}

        a, b = fixture([17, 128, 8192]), fixture([32, 1024])
        self.assertEqual([row["width"] for row in launcher.selected_rows(a, b)],
                         [32, 128, 1024, 8192])
        b["compiler"] = "wrong"
        with self.assertRaisesRegex(ValueError, "compiler"):
            launcher.selected_rows(a, b)

    def test_journal_requires_every_ordered_pair_and_same_expiry(self):
        value = {"pid": 15, "request": {"sha256": "req"}, "profile": "profile",
                 "native_binary_sha256": "exe", "original_expires_monotonic_ns": 1000,
                 "rows": [{"input_digest": digest, "native_call_elapsed_us": 3}
                          for digest in ("32", "128", "1024", "8192")]}
        identity = {"input_sha256": "req", "profile": "profile", "process_id": 15,
                    "native_binary_sha256": "exe"}
        events = []
        for ordinal, digest in enumerate((None, "32", "128", "1024", "8192")):
            begin = ordinal * 10 + 10
            for phase in ("started", "returned"):
                returned = phase == "returned"
                events.append({"schema": "airk.arbiter.native-operation/1", "sequence": len(events) + 1,
                               "identity": identity, "kind": "numeric" if digest else "load",
                               "ordinal": ordinal, "phase": phase, "clock": "windows_performance_counter",
                               "operation_input_digest": digest, "started_monotonic_ns": begin,
                               "observed_monotonic_ns": begin + (3 if returned else 0),
                               "expires_monotonic_ns": 1000, "native_success": True if returned else None,
                               "native_call_elapsed_us": 3 if returned else None})
        launcher.journal(events, value)
        with self.assertRaisesRegex(ValueError, "operation pairs"):
            launcher.journal(events[:-2], value)
        events[-1]["expires_monotonic_ns"] = 1001
        with self.assertRaisesRegex(ValueError, "clock"):
            launcher.journal(events, value)


    @unittest.skipUnless(os.environ.get("AIRK_TEST_RETAINED_COMPACT_CUDA_PREPARATION"),
                         "explicit retained graph preparation required")
    def test_retained_preparation_loads_the_real_frozen_owners(self):
        path = os.environ["AIRK_TEST_RETAINED_COMPACT_CUDA_PREPARATION"]
        pin, raw = launcher.bootstrap(path, launcher.PREPARATION_SHA)
        prepared = json.loads(raw)
        deadline = time.monotonic() + 30
        fresh, legacy, component, _, _, adapter = launcher.make_owners(prepared["plan"], deadline)
        plan = legacy.execution_plan(prepared, pin)
        adapter.identify(plan)
        fresh.captured(
            plan["graph_provenance_contract"]["path"], plan["graph_provenance_contract"]["sha256"],
            fresh.MAX_SOURCE, deadline)
        self.assertEqual((component.LIMITS["process_bytes"], component.LIMITS["job_bytes"]),
                         (32 << 30, 32 << 30))


if __name__ == "__main__":
    unittest.main()
