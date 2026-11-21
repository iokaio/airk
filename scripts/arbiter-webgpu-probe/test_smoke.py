# SPDX-License-Identifier: Apache-2.0
"""Controls for the WebGPU smoke test's pure helpers. No provider, model or device work."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import smoke


def receipt(tokens=4, options=2, reference=None):
    reference = [1.5, -0.25] if reference is None else reference
    return {"result": {"case": "CONTROL", "matching_fp16": None, "compiled": {
        "input_ids": list(range(tokens)), "attention_mask": [1] * tokens, "position_ids": list(range(tokens)),
        "decision_position": tokens - 1, "candidate_positions": list(range(options)),
        "option_mask": [True] * options, "input_digest": "ab" * 32},
        "original_binding": {"original_fp32": reference}}}


class Item:
    def __init__(self, name, kind, shape):
        self.name, self.type, self.shape = name, kind, shape


class Session:
    def __init__(self, items):
        self.items = items

    def get_inputs(self):
        return self.items


class FakeArray:
    def __init__(self, values, dtype, ndim=1):
        self.values, self.dtype, self.ndim = values, dtype, ndim

    def __getitem__(self, key):
        return FakeArray(self.values, self.dtype, self.ndim + 1)


class FakeNumpy:
    int64, bool_ = "int64", "bool"

    @staticmethod
    def asarray(values, dtype):
        return FakeArray(values, dtype)


ITEMS = [Item("input_ids", "tensor(int64)", [1, "tokens"]), Item("attention_mask", "tensor(int64)", [1, "tokens"]),
         Item("position_ids", "tensor(int64)", [1, "tokens"]), Item("decision_positions", "tensor(int64)", [1]),
         Item("candidate_positions", "tensor(int64)", [1, "options"]), Item("option_mask", "tensor(bool)", [1, "options"])]


class SmokeHelpers(unittest.TestCase):
    def write(self, folder, name, data):
        path = Path(folder) / name
        path.write_text(json.dumps(data), encoding="utf-8")
        return path

    def test_case_reads_inputs_and_reference(self):
        with tempfile.TemporaryDirectory() as folder:
            case = smoke.load_case(self.write(folder, "case.json", receipt()))
        self.assertEqual((case["case"], case["tokens"], case["original_fp32"]), ("CONTROL", 4, [1.5, -0.25]))
        self.assertEqual(len(case["receipt"]["sha256"]), 64)

    def test_case_refuses_inconsistent_shapes(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(ValueError):
                smoke.load_case(self.write(folder, "case.json", receipt(reference=[1.0])))

    def test_inputs_only_drops_the_reference_of_another_model(self):
        with tempfile.TemporaryDirectory() as folder:
            case = smoke.load_case(self.write(folder, "case.json", receipt(reference=[1.0])), reference=False)
        self.assertEqual((case["case"], case["original_fp32"], case["tokens"]), ("CONTROL (inputs only)", None, 4))

    def test_fixture_rows_become_cases(self):
        row = receipt()["result"]["compiled"]
        row = {key: row[key] for key in ("input_ids", "attention_mask", "position_ids", "decision_position",
                                         "candidate_positions", "option_mask")}
        doc = {"schema": "airk.private-context-native-fp32-probes/1",
               "rows": [{"id": "synthetic", "input": row, "original_fp32_all_live": [0.5, 0.25]}]}
        with tempfile.TemporaryDirectory() as folder:
            cases = smoke.load_fixture(self.write(folder, "fixture.json", doc))
            with self.assertRaises(ValueError):
                smoke.load_fixture(self.write(folder, "other.json", {"schema": "other", "rows": []}))
        self.assertEqual([(c["case"], c["tokens"], c["original_fp32"]) for c in cases], [("synthetic", 4, [0.5, 0.25])])

    def test_feeds_add_the_batch_axis_only_where_the_graph_has_one(self):
        compiled = receipt()["result"]["compiled"]
        out = smoke.feeds(Session(ITEMS), compiled, FakeNumpy)
        self.assertEqual({name: array.ndim for name, array in out.items()},
                         {"input_ids": 2, "attention_mask": 2, "position_ids": 2, "decision_positions": 1,
                          "candidate_positions": 2, "option_mask": 2})
        self.assertEqual(out["decision_positions"].values, [3])
        self.assertEqual(out["option_mask"].dtype, "bool")

    def test_feeds_refuse_an_unexpected_graph_input(self):
        compiled = receipt()["result"]["compiled"]
        with self.assertRaises(ValueError):
            smoke.feeds(Session(ITEMS + [Item("past_key", "tensor(float)", [1])]), compiled, FakeNumpy)
        with self.assertRaises(ValueError):
            smoke.feeds(Session(ITEMS[:5]), compiled, FakeNumpy)

    def test_placement_counts_each_kernel_once_per_provider(self):
        events = [
            {"cat": "Node", "name": "a_kernel_time", "args": {"provider": "WebGpuExecutionProvider", "op_name": "MatMul"}},
            {"cat": "Node", "name": "a_kernel_time", "args": {"provider": "WebGpuExecutionProvider", "op_name": "MatMul"}},
            {"cat": "Node", "name": "b_kernel_time", "args": {"provider": "CPUExecutionProvider", "op_name": "Gather"}},
            {"cat": "Node", "name": "b_fence_before", "args": {"provider": "CPUExecutionProvider", "op_name": "Gather"}},
            {"cat": "Session", "name": "model_run"}]
        with tempfile.TemporaryDirectory() as folder:
            counted = smoke.placement(self.write(folder, "profile.json", events))
        self.assertEqual(counted, {"WebGpuExecutionProvider": {"nodes": 1, "ops": {"MatMul": 1}},
                                   "CPUExecutionProvider": {"nodes": 1, "ops": {"Gather": 1}}})


if __name__ == "__main__":
    unittest.main()
