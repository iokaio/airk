# SPDX-License-Identifier: Apache-2.0
"""Cheap Resource20 journal and held-refresh controls, without native work."""

import importlib.util
import json
from pathlib import Path
import unittest


def load(leaf):
    path = Path(__file__).with_name(leaf + ".py")
    spec = importlib.util.spec_from_file_location(leaf, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


watch = load("resource20_watch")
launcher = load("compact_resource20")


class Controls(unittest.TestCase):
    @staticmethod
    def event(sequence, kind, ordinal, phase, start, whole):
        returned = phase == "returned"
        return {"schema": "airk.arbiter.native-operation/1", "sequence": sequence,
                "identity": {"process_id": 123}, "kind": kind, "ordinal": ordinal,
                "operation_input_digest": None if kind == "load" else "digest",
                "phase": phase, "clock": "windows_performance_counter", "started_monotonic_ns": start,
                "expires_monotonic_ns": min(start + 120_000_000_000, whole),
                "observed_monotonic_ns": start + (3_000 if returned else 0),
                "native_call_elapsed_us": 2 if returned else None, "native_success": True if returned else None}

    def test_required_operations_and_original_expiry(self):
        whole = 150_000_000_000
        state = watch.Operations({"process_id": 123}, "digest", whole)
        roster = [("load", 0), ("warmup", 0)] + [("sample", number) for number in range(1, 21)]
        raw = b""
        for index, (kind, ordinal) in enumerate(roster):
            start = 50_000_000_000 + index * 1_000_000_000
            for phase in ("started", "returned"):
                event = self.event(len(state.events) + 1, kind, ordinal, phase, start, whole)
                raw += (json.dumps(event) + "\n").encode()
                state.feed(raw, start + 3_001)
        state.prove_exit(0)
        self.assertEqual(len(state.proof()["observed_events"]), 44)
        self.assertEqual(state.expiry(), whole)
        with self.assertRaisesRegex(ValueError, "expiry reached"):
            state.check_expiry(whole)
        with self.assertRaisesRegex(ValueError, "changed prefix"):
            state.feed(raw[1:], whole - 1)

    def test_fence_refuses_renewed_return_and_missing_work(self):
        whole = 3_600_000_000_000
        state = watch.Operations({"process_id": 123}, "digest", whole)
        start = self.event(1, "load", 0, "started", 5, whole)
        state.observe(start, 6)
        returned = self.event(2, "load", 0, "returned", 5, whole)
        returned["expires_monotonic_ns"] += 1
        with self.assertRaisesRegex(ValueError, "original allowance"):
            state.observe(returned, 3_006)
        returned["expires_monotonic_ns"] -= 1
        state.observe(returned, 3_006)
        self.assertEqual(state.expiry(), 120_000_000_005)
        with self.assertRaisesRegex(ValueError, "omitted"):
            state.prove_exit(0)

    def test_frozen_refresh_adapts_exact_anchors(self):
        owner = launcher.refresh_owner()
        self.assertEqual(owner.SELF, launcher.SELF)
        self.assertIn("--resource-predecessor-packet", owner.refresh.__code__.co_consts)


if __name__ == "__main__":
    unittest.main()
