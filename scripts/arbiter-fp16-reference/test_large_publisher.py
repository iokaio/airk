# SPDX-License-Identifier: Apache-2.0
"""Compare streamed BF16 publisher input with the existing independent half recipe."""

from contextlib import contextmanager
from pathlib import Path
import runpy
import unittest

import torch

HERE = Path(__file__).resolve().parent
PublisherState = runpy.run_path(str(HERE / "large_publisher.py"))["PublisherState"]
LITERAL = runpy.run_path(str(HERE / "test_reference.py"))


class LargePublisherTests(unittest.TestCase):
    def test_stream_matches_dense_logical_source_and_full_forward(self):
        config = LITERAL["config"]()
        original = {k: v.to(torch.bfloat16) for k, v in LITERAL["state"](config).items()}
        # Nontrivial signs and distinct entries exercise both projection layouts.
        original["layers.0.self_attn.q_proj.weight"] = torch.tensor([[0.3125, -1.75], [1.125, 0.625],
            [-0.1875, 2.5], [0.5, -0.0625]], dtype=torch.bfloat16)
        index = {"model." + name: ("b.safetensors" if i % 2 else "a.safetensors")
                 for i, name in enumerate(original)}
        opened = []
        active = []

        @contextmanager
        def reader(path, *, framework, device):
            self.assertEqual((framework, device, active), ("pt", "cpu", []))
            active.append(path.name)
            opened.append(path.name)
            class Shard:
                def keys(self):
                    return [k for k, shard in index.items() if shard == path.name]
                def get_tensor(self, key):
                    return original[key.removeprefix("model.")]
            yield Shard()
            active.pop()

        recipe = LITERAL["r"]
        state = PublisherState(HERE, index, torch, reader, lambda *_: None)
        dense = recipe.HalfReference(torch, config, {k: v.float() for k, v in original.items()}, False, "cpu")
        streamed = recipe.HalfReference(torch, config, state, False, "cpu")
        self.assertEqual(opened, ["a.safetensors", "b.safetensors"])
        self.assertEqual({r["parameter"]: r for r in streamed.parameter_census()},
                         {r["parameter"]: r for r in dense.parameter_census()})
        self.assertTrue(torch.equal(streamed.forward(LITERAL["native"]()), dense.forward(LITERAL["native"]())))

    def test_rejects_unexpected_header_and_dtype(self):
        index = {"model.norm.weight": "a.safetensors"}
        class Shard:
            def __init__(self, keys):
                self.names = keys
            def keys(self):
                return self.names
            def get_tensor(self, key):
                return torch.zeros(1, dtype=torch.float32)
        for keys, message in (([], "shard header"), (list(index), "CPU BF16")):
            with self.subTest(message=message):
                @contextmanager
                def reader(*_, **__):
                    yield Shard(keys)
                state = PublisherState(HERE, index, torch, reader, lambda *_: None)
                with self.assertRaisesRegex(ValueError, message):
                    list(state.items())


if __name__ == "__main__":
    torch.set_num_threads(1)
    unittest.main()
