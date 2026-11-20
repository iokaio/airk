# SPDX-License-Identifier: Apache-2.0
"""Read authenticated Large publisher tensors as logical FP32, one parameter at a time."""

import runpy
from pathlib import Path


class PublisherState:
    """Iterator interface consumed by the Context HalfReference constructor."""

    def __init__(self, folder, index, torch, open_shard, observe):
        if not index or not all(name.startswith("model.") and len(name) > 6 for name in index):
            raise ValueError("Large publisher requires the tied embedding and exact model namespace")
        self.folder = folder
        self.torch = torch
        self.open_shard = open_shard
        self.observe = observe
        self.shards = {}
        for name, shard in index.items():
            if Path(shard).name != shard or not shard.endswith(".safetensors"):
                raise ValueError("invalid publisher shard path")
            self.shards.setdefault(shard, set()).add(name)

    def __iter__(self):
        for names in self.shards.values():
            for name in names:
                yield name.removeprefix("model.")

    def items(self):
        for shard, names in sorted(self.shards.items()):
            self.observe("open_publisher_shard", shard)
            with self.open_shard(self.folder / shard, framework="pt", device="cpu") as source:
                if set(source.keys()) != names:
                    raise ValueError("Large shard header differs from authenticated index")
                for name in sorted(names):
                    value = source.get_tensor(name)
                    if value.dtype != self.torch.bfloat16 or value.device.type != "cpu":
                        raise ValueError("Large publisher tensor must be CPU BF16")
                    # Promotion reproduces the original logical FP32 values exactly.
                    # The existing owner still verifies shape/finiteness and generates
                    # every ordinary/transposed source and rounded parameter digest.
                    yield name.removeprefix("model."), value.float()
            self.observe("consumed_publisher_shard", shard)


def build(half, torch, prep, observe):
    from safetensors import safe_open

    owner = prep["OWNER"]
    publisher = half.PUBLISHER_OWNER_SHA["context"]
    prep["pin"](prep["ROOT"] / owner, publisher)
    source = runpy.run_path(str(prep["ROOT"] / owner))
    folder, config, index = source["authenticate"](prep["PUBLISHER"], "Qwen/Qwen3-4B")
    if len(index) != 398 or len(set(index.values())) != 3 or config.get("tie_word_embeddings") is not True:
        raise ValueError("Large release tensor/shard roster differs")
    prep["reference_memory"](config)
    half.freeze_reference_math(torch)
    state = PublisherState(folder, index, torch, safe_open, observe)
    reference = half.HalfReference(torch, config, state, False, "cpu")
    return reference, {"model": "large", "revision": source["REVISIONS"]["Qwen/Qwen3-4B"],
                       "publisher_owner": {"path": str(prep["ROOT"] / owner), "sha256": publisher},
                       "logical_source_dtype": "float32", "publisher_storage_dtype": "bfloat16",
                       "candidate_ONNX_used_for_expected_values": False, "qualification": False}
