# SPDX-License-Identifier: Apache-2.0
"""Private Qwen3 full-prefill DecisionSymbol preparation. No import-time NN.

Architecture and numerical execution require a separately reviewed owned release.
This is not the Qwen compiler, calibration, quality gate, or a release profile.
"""
import hashlib
import json
from pathlib import Path

MANIFEST_SHA = "4cac4fc751d996e987bb4701fb90ce8cbd600af06152151b4ea99bed8d6c664c"
REVISIONS = {"Qwen/Qwen3-1.7B": "70d244cc86ccca08cf5af4e1e306ecf908b1ad5e",
             "Qwen/Qwen3-4B": "1cfa9a7208912126459214e8b04321603b3df60c"}
INPUT_NAMES = ("input_ids", "attention_mask", "position_ids", "decision_positions",
               "candidate_positions", "option_mask")


def sha(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as stream:
        while block := stream.read(4 * 1024 * 1024):
            h.update(block)
    return h.hexdigest()


def verify_asset(path, pin):
    path = Path(path)
    if path.stat().st_size != pin["bytes"]:
        raise ValueError("publisher asset length mismatch")
    h = hashlib.sha256() if pin["sha256"] else hashlib.sha1()
    if not pin["sha256"]:
        h.update(("blob " + str(pin["bytes"]) + "\0").encode())
    with path.open("rb") as stream:
        while block := stream.read(4 * 1024 * 1024):
            h.update(block)
    if h.hexdigest() != (pin["sha256"] or pin["git_blob"]):
        raise ValueError("publisher asset digest mismatch")


def validate_config(config):
    if config.get("model_type") != "qwen3" or config.get("architectures") != ["Qwen3ForCausalLM"]:
        raise ValueError("only ordinary dense Qwen3 causal LM supported")
    if config.get("use_sliding_window") or not config.get("tie_word_embeddings"):
        raise ValueError("unsupported sliding attention or untied source head")
    if config.get("head_dim") != 128 or config.get("attention_bias"):
        raise ValueError("unsupported attention parameterization")
    if config["num_attention_heads"] % config["num_key_value_heads"]:
        raise ValueError("nonintegral GQA groups")
    if config.get("rope_scaling") or config.get("rope_theta") != 1000000:
        raise ValueError("unsupported RoPE configuration")


def authenticate(asset_root, model_id):
    """Authenticate the complete selected source closure BEFORE tensor loading."""
    root = Path(asset_root).resolve()
    manifest = root / "research-asset-pins.json"
    if sha(manifest) != MANIFEST_SHA:
        raise ValueError("unreviewed publisher manifest")
    catalog = json.loads(manifest.read_bytes())
    selected = [m for m in catalog["models"] if m["id"] == model_id]
    if len(selected) != 1 or selected[0]["revision"] != REVISIONS.get(model_id):
        raise ValueError("unreviewed model or revision")
    model = selected[0]
    folder = root / "candidate-assets" / model_id.replace("/", "--")
    expected = {a["name"] for a in model["assets"]}
    actual = {str(p.relative_to(folder)).replace("\\", "/") for p in folder.rglob("*") if p.is_file()}
    if expected != actual:
        raise ValueError("missing or additional publisher source files")
    for pin in model["assets"]:
        verify_asset(folder / pin["name"], pin)
    config = json.loads((folder / "config.json").read_bytes())
    validate_config(config)
    index = json.loads((folder / "model.safetensors.index.json").read_bytes())["weight_map"]
    if set(index.values()) != {n for n in expected if n.endswith(".safetensors")}:
        raise ValueError("index and retained shard set differ")
    return folder, config, index


def make_wrapper(config, state=None):
    """Six-input batch-one full prefill; all Qwen3 layers, sparse original LM rows.

    candidate_positions are VOCABULARY IDs for this readout, not token offsets.
    Native admission must validate shapes/ranges, no padding holes, decision at
    last attended token, <=2048 tokens, <=8 options and at least one live option.
    """
    import torch
    from torch import nn
    from transformers import Qwen3Config, Qwen3Model
    cfg = Qwen3Config.from_dict(config)
    cfg._attn_implementation = "eager"
    if state is None:
        text = Qwen3Model(cfg)
    else:
        with torch.device("meta"):
            text = Qwen3Model(cfg)
        text.load_state_dict(state, strict=True, assign=True)
        # Nonpersistent RoPE buffers are absent from the state dict and must
        # be freshly materialized from the authenticated configuration.
        from transformers.models.qwen3.modeling_qwen3 import Qwen3RotaryEmbedding
        text.rotary_emb = Qwen3RotaryEmbedding(cfg, device="cpu")
        if any(v.is_meta for v in list(text.parameters()) + list(text.buffers())):
            raise ValueError("unmaterialized model state")
    text = text.float().eval()

    class Scorer(nn.Module):
        def __init__(self):
            super().__init__()
            self.text = text

        def forward(self, input_ids, attention_mask, position_ids, decision_positions,
                    candidate_positions, option_mask):
            hidden = self.text.embed_tokens(input_ids)
            rotary = self.text.rotary_emb(hidden, position_ids)
            length = input_ids.shape[1]
            axis = torch.arange(length, device=input_ids.device)
            causal = axis[None, :] <= axis[:, None]
            allowed = causal[None, None, :, :] & attention_mask[:, None, None, :].bool()
            additive = torch.where(allowed, hidden.new_zeros(()),
                                   hidden.new_full((), torch.finfo(torch.float32).min))
            for layer in self.text.layers:
                hidden = layer(hidden, attention_mask=additive,
                               position_embeddings=rotary, position_ids=position_ids,
                               use_cache=False)
            hidden = self.text.norm(hidden)
            decision = hidden.gather(1, decision_positions[:, None, None].expand(-1, 1, cfg.hidden_size)).squeeze(1)
            rows = torch.nn.functional.embedding(candidate_positions, self.text.embed_tokens.weight)
            scores = (rows * decision[:, None, :]).sum(-1)
            return scores.masked_fill(~option_mask, -1e4), hidden

    return Scorer().eval()


def load_authenticated(asset_root, model_id):
    """No key filtering: exact index consumption, strict model state, tied head."""
    folder, config, index = authenticate(asset_root, model_id)
    import torch
    from safetensors import safe_open
    state = {}
    consumed = set()
    duplicate_head = None
    for shard in sorted(set(index.values())):
        with safe_open(folder / shard, framework="pt", device="cpu") as source:
            expected = {k for k, value in index.items() if value == shard}
            if set(source.keys()) != expected:
                raise ValueError("shard header differs from authenticated index")
            for key in source.keys():
                value = source.get_tensor(key)
                if value.dtype != torch.bfloat16:
                    raise ValueError("unexpected publisher tensor dtype")
                if key == "lm_head.weight":
                    duplicate_head = value
                elif key.startswith("model."):
                    state[key[len("model."):]] = value
                else:
                    raise ValueError("unconsumed publisher tensor")
                consumed.add(key)
    if consumed != set(index):
        raise ValueError("incomplete tensor consumption")
    if duplicate_head is not None and not torch.equal(duplicate_head, state["embed_tokens.weight"]):
        raise ValueError("declared tied LM head differs from embedding bytes")
    del duplicate_head
    return make_wrapper(config, state), config


def freeze_math():
    import torch
    torch.manual_seed(1908)
    torch.set_num_threads(2)
    torch.use_deterministic_algorithms(True)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    torch.backends.cudnn.deterministic = True


def tiny_config():
    # Includes GQA, independent head_dim, q/k norm and full rotary. Not model-shape qualification.
    return {"model_type": "qwen3", "architectures": ["Qwen3ForCausalLM"],
            "hidden_size": 48, "intermediate_size": 96, "num_hidden_layers": 2,
            "num_attention_heads": 4, "num_key_value_heads": 2, "head_dim": 16,
            "vocab_size": 256, "tie_word_embeddings": True, "attention_bias": False,
            "rope_theta": 1000000, "use_sliding_window": False, "max_position_embeddings": 2048}
