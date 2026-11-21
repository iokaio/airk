# SPDX-License-Identifier: Apache-2.0
"""Controls for the WebGPU smoke test's pure helpers. No provider, model or device work.

Every expected value below is written out by hand, not computed by the script under test.
"""
import json
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest import mock

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

    def test_sampler_lap_gives_the_peak_since_the_previous_lap(self):
        sampler = smoke.Sampler(interval=0)
        readings = iter([5, None, 9, 7])

        def reading():
            value = next(readings)
            if value == 7:
                sampler._stop_event.set()
            return value

        with mock.patch.object(smoke, "gpu_used_mib", reading):
            sampler.run()
        self.assertEqual((sampler.peak, sampler.samples), (9, 3))
        self.assertEqual(sampler.lap(4), 9)
        self.assertEqual(sampler.lap(4), 4)
        self.assertIsNone(sampler.lap())
        self.assertEqual(sampler.peak, 9)


def compiled_three():
    return {"input_ids": [11, 12, 13], "attention_mask": [1, 1, 1], "position_ids": [0, 1, 2], "decision_position": 2,
            "candidate_positions": [32, 33], "option_mask": [True, True], "input_digest": "ab" * 32, "prompt": "x"}


class Padding(unittest.TestCase):
    def test_pad_length_is_the_smallest_listed_length_that_holds_the_tokens(self):
        self.assertEqual(smoke.pad_length(1, [512, 2048]), 512)
        self.assertEqual(smoke.pad_length(512, [512, 2048]), 512)
        self.assertEqual(smoke.pad_length(513, [2048, 512]), 2048)
        self.assertEqual(smoke.pad_length(2048, [8192, 2048, 512]), 2048)
        self.assertEqual(smoke.pad_length(1588, [512, 1024, 2048, 4096, 8192]), 2048)
        self.assertEqual(smoke.pad_length(8192, [512, 1024, 2048, 4096, 8192]), 8192)

    def test_pad_length_refuses_a_case_longer_than_every_length(self):
        for tokens, lengths in ((2049, [512, 2048]), (8193, [8192]), (5, [])):
            with self.assertRaises(ValueError):
                smoke.pad_length(tokens, lengths)

    def test_pad_plan_prefers_an_exact_length_then_the_ladder_then_nothing(self):
        self.assertEqual(smoke.pad_plan(100, []), (None, None))
        self.assertEqual(smoke.pad_plan(100, [512, 128]), (128, "ladder"))
        self.assertEqual(smoke.pad_plan(100, [128], 4096), (4096, "exact"))
        self.assertEqual(smoke.pad_plan(100, [], 100), (100, "exact"))
        with self.assertRaises(ValueError):
            smoke.pad_plan(100, [512], 99)
        with self.assertRaises(ValueError):
            smoke.pad_plan(600, [512])

    def test_padded_appends_pad_ids_a_zero_mask_and_continued_positions(self):
        source = compiled_three()
        self.assertEqual(smoke.padded(source, 6, 151643), {
            "input_ids": [11, 12, 13, 151643, 151643, 151643], "attention_mask": [1, 1, 1, 0, 0, 0],
            "position_ids": [0, 1, 2, 3, 4, 5], "decision_position": 2, "candidate_positions": [32, 33],
            "option_mask": [True, True]})
        self.assertEqual(source, compiled_three())

    def test_padded_to_its_own_length_keeps_the_six_inputs_and_drops_the_digest(self):
        self.assertEqual(smoke.padded(compiled_three(), 3, 7), {
            "input_ids": [11, 12, 13], "attention_mask": [1, 1, 1], "position_ids": [0, 1, 2], "decision_position": 2,
            "candidate_positions": [32, 33], "option_mask": [True, True]})

    def test_padded_continues_positions_that_do_not_start_at_zero(self):
        source = compiled_three() | {"position_ids": [7, 8, 9]}
        self.assertEqual(smoke.padded(source, 5, 0)["position_ids"], [7, 8, 9, 10, 11])

    def test_padded_refuses_a_shorter_length_and_an_empty_input(self):
        with self.assertRaises(ValueError):
            smoke.padded(compiled_three(), 2, 0)
        empty = compiled_three() | {"input_ids": [], "attention_mask": [], "position_ids": []}
        with self.assertRaises(ValueError):
            smoke.padded(empty, 4, 0)

    def test_case_argument_splits_a_trailing_exact_length(self):
        self.assertEqual(smoke.case_argument("D:/x/case.json"), (Path("D:/x/case.json"), None))
        self.assertEqual(smoke.case_argument("D:/x/case.json@4096"), (Path("D:/x/case.json"), 4096))
        self.assertEqual(smoke.case_argument("D:/x/a@b/case.json"), (Path("D:/x/a@b/case.json"), None))
        self.assertEqual(smoke.case_argument("D:/x/a@2/case.json@512"), (Path("D:/x/a@2/case.json"), 512))
        self.assertEqual(smoke.case_argument("@512"), (Path("@512"), None))
        self.assertEqual(smoke.case_argument("case.json@"), (Path("case.json@"), None))


class Attribute:
    def __init__(self, name, value):
        self.name, self.i = name, value


class Node:
    def __init__(self, op_type, inputs, causal=None):
        self.op_type, self.input = op_type, list(inputs)
        self.attribute = [] if causal is None else [Attribute("causal", causal), Attribute("num_heads", 16)]


class Named:
    def __init__(self, name):
        self.name = name


class Graph:
    def __init__(self, nodes, inputs=("input_ids", "attention_mask", "position_ids"), stored=()):
        self.node, self.input = list(nodes), [Named(name) for name in inputs]
        self.initializer = [Named(name) for name in stored]


def attention(causal=1, past="", bias=None):
    inputs = ["q", "k", "v", past, "", "seqlens", "total"]
    return Node("GroupQueryAttention", inputs if bias is None else inputs + ["", "", "", bias], causal)


def portable_nodes():
    return [Node("Gather", ["embedding", "input_ids"]), Node("Shape", ["attention_mask"]),
            Node("Unsqueeze", ["position_ids", "axes"]), Node("Shape", ["position_ids"]),
            attention(), attention()]


class PaddingBasis(unittest.TestCase):
    def test_a_shape_only_mask_with_plain_causal_attention_supports_right_padding(self):
        self.assertEqual(smoke.padding_basis(Graph(portable_nodes())), {
            "input_readers": {"input_ids": ["Gather"], "attention_mask": ["Shape"],
                              "position_ids": ["Shape", "Unsqueeze"]},
            "attention_nodes": 2, "causal_without_past_or_bias": 2, "supports_right_padding": True})

    def test_a_stored_tensor_listed_as_a_graph_input_is_not_reported(self):
        graph = Graph(portable_nodes(), inputs=("input_ids", "attention_mask", "position_ids", "embedding"),
                      stored=("embedding",))
        self.assertEqual(sorted(smoke.padding_basis(graph)["input_readers"]),
                         ["attention_mask", "input_ids", "position_ids"])

    def test_structures_that_do_not_support_right_padding(self):
        cases = {
            "mask values are read": (portable_nodes() + [Node("Cast", ["attention_mask"])], 2, 2),
            "mask is not read": ([node for node in portable_nodes() if node.input != ["attention_mask"]], 2, 2),
            "explicit bias": (portable_nodes() + [attention(bias="bias")], 3, 2),
            "past state": (portable_nodes() + [attention(past="past_key")], 3, 2),
            "not causal": (portable_nodes() + [attention(causal=0)], 3, 2),
            "no causal attribute": (portable_nodes() + [Node("GroupQueryAttention", ["q", "k", "v"])], 3, 2),
            "no attention node": (portable_nodes()[:4], 0, 0),
        }
        for label, (nodes, total, plain) in cases.items():
            with self.subTest(label):
                basis = smoke.padding_basis(Graph(nodes))
                self.assertEqual((basis["attention_nodes"], basis["causal_without_past_or_bias"],
                                  basis["supports_right_padding"]), (total, plain, False))

    def test_padding_setup_refuses_a_missing_pad_id_and_a_bad_length_before_reading_the_graph(self):
        for lengths, pad in (([2048], None), ([2048], -1), ([0, 2048], 151643)):
            with self.assertRaises(ValueError):
                smoke.padding_setup(Path("absent.onnx"), lengths, pad)


class FakeOptions:
    def __init__(self):
        self.entries, self.intra_op_num_threads, self.devices = [], None, None

    def add_session_config_entry(self, key, value):
        self.entries.append((key, value))

    def add_provider_for_devices(self, devices, options):
        self.devices = (devices, options)


class FakeSession:
    def __init__(self, bound):
        self.bound = bound

    def get_providers(self):
        return self.bound


class FakeOrt:
    SessionOptions = FakeOptions

    def __init__(self, devices=(), bound=("WebGpuExecutionProvider", "CPUExecutionProvider")):
        self.devices, self.registered, self.sessions, self.bound = list(devices), [], [], list(bound)

    def register_execution_provider_library(self, name, path):
        self.registered.append((name, path))

    def get_ep_devices(self):
        return self.devices

    def InferenceSession(self, model, sess_options, providers=None):
        self.sessions.append((model, sess_options, providers))
        return FakeSession(self.bound if providers is None else providers)


def device(ep_name="WebGpuExecutionProvider"):
    hardware = types.SimpleNamespace(vendor="NVIDIA", vendor_id=4318, device_id=8752, metadata={"Description": "A6000"})
    return types.SimpleNamespace(ep_name=ep_name, ep_metadata={"version": "0.6.0"}, device=hardware)


PLUGIN = types.SimpleNamespace(get_library_path=lambda: "plugin.dll", get_ep_name=lambda: "WebGpuExecutionProvider")


class SessionSetup(unittest.TestCase):
    def test_cpu_session_is_prepared_first_and_opened_only_when_called(self):
        ort, receipt = FakeOrt(), {}
        create = smoke.prepare_session(ort, Path("graph.onnx"), "cpu", {"ignored": "1"}, {"a": "b"}, 8, receipt,
                                       lambda path: {"pinned": path},
                                       lambda options: options.add_session_config_entry("c", "d"))
        self.assertEqual((ort.sessions, receipt), ([], {"intra_threads": 8}))
        self.assertEqual(create().get_providers(), ["CPUExecutionProvider"])
        model, options, providers = ort.sessions[0]
        self.assertEqual((model, providers, len(ort.sessions)), ("graph.onnx", ["CPUExecutionProvider"], 1))
        self.assertEqual((options.entries, options.intra_op_num_threads, options.devices),
                         ([("a", "b"), ("c", "d")], 8, None))
        self.assertEqual(ort.registered, [])

    def test_webgpu_session_binds_the_one_plugin_device_and_records_it(self):
        chosen = device()
        ort, receipt = FakeOrt([device("OtherProvider"), chosen]), {}
        with mock.patch.dict(sys.modules, {"onnxruntime_ep_webgpu": PLUGIN}):
            create = smoke.prepare_session(ort, Path("graph.onnx"), "webgpu", {"storageBufferCacheMode": "simple"},
                                           {}, None, receipt, lambda path: {"pinned": path})
        self.assertEqual(receipt, {"plugin": {"version": "0.6.0", "library": {"pinned": "plugin.dll"},
                                              "adapter": {"vendor": "NVIDIA", "vendor_id": 4318, "device_id": 8752,
                                                          "Description": "A6000"}}})
        self.assertEqual((ort.registered, ort.sessions), ([("webgpu", "plugin.dll")], []))
        self.assertEqual(create().get_providers(), ["WebGpuExecutionProvider", "CPUExecutionProvider"])
        model, options, providers = ort.sessions[0]
        self.assertEqual((model, providers, options.intra_op_num_threads), ("graph.onnx", None, None))
        self.assertEqual(options.devices, ([chosen], {"storageBufferCacheMode": "simple"}))

    def test_webgpu_session_that_fell_back_to_the_cpu_provider_is_refused(self):
        ort = FakeOrt([device()], bound=["CPUExecutionProvider"])
        with mock.patch.dict(sys.modules, {"onnxruntime_ep_webgpu": PLUGIN}):
            create = smoke.prepare_session(ort, Path("graph.onnx"), "webgpu", {"storageBufferCacheMode": "notAMode"},
                                           {}, None, {}, str)
        with self.assertRaises(ValueError):
            create()
        self.assertEqual(len(ort.sessions), 1)

    def test_webgpu_session_refuses_no_device_and_two_devices(self):
        for devices in ([], [device(), device()]):
            with mock.patch.dict(sys.modules, {"onnxruntime_ep_webgpu": PLUGIN}), self.assertRaises(ValueError):
                smoke.prepare_session(FakeOrt(devices), Path("graph.onnx"), "webgpu", {}, {}, None, {}, str)


if __name__ == "__main__":
    unittest.main()
