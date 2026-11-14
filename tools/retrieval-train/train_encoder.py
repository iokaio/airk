# SPDX-License-Identifier: Apache-2.0
"""Bounded local MiniLM contrastive training on reviewed original QASPER labels."""
import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path
import random
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--additional-train", type=Path)
    parser.add_argument("--training-recipe", choices=("paragraph-prefix-v1", "visible-prefix-mining-v1"),
                        default="paragraph-prefix-v1")
    parser.add_argument("--training-master", type=Path)
    parser.add_argument("--training-seal", type=Path)
    parser.add_argument("--training-seal-sha256")
    args = parser.parse_args()
    execution = None
    if args.training_recipe == "visible-prefix-mining-v1":
        if args.training_master is None or args.training_seal is None or args.training_seal_sha256 is None:
            parser.error("candidate requires frozen315 master and reviewed complete execution seal")
        import training_execution as execution
        execution.verify_seal(args.training_seal, args.training_seal_sha256, wait_for_gate=True)
    import numpy as np
    import torch
    from safetensors.torch import load as load_tensors
    from transformers import BertConfig, BertModel
    from tokenizers import Tokenizer
    if execution is not None:
        execution.verify_torch_runtime(torch, args.training_seal)
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT / "artifacts"):
        parser.error("fresh private output required")
    retained = {path: path.read_bytes() for path in (Path(__file__), args.corpus, HERE / "a0-freeze.json",
        args.source / "config.json", args.source / "tokenizer.json", args.source / "model.safetensors")}
    if sha(retained[args.corpus]) != "6e9d8ade6f799ed2a5e1903de69482db9fcd3cb1731a5fee4a504adede5fca6f":
        raise ValueError("unreviewed training corpus")
    recipe = None
    if args.training_recipe == "visible-prefix-mining-v1":
        import training_recipe as recipe
        import training_recipe_audit as recipe_source
        retained[Path(recipe.__file__)] = Path(recipe.__file__).read_bytes()
        retained[Path(recipe_source.__file__)] = Path(recipe_source.__file__).read_bytes()
        corpus = recipe.train_only_corpus(retained[args.corpus])
    else:
        corpus = json.loads(retained[args.corpus])
    additional_count = 0
    if args.additional_train:
        retained[args.additional_train] = args.additional_train.read_bytes()
        if sha(retained[args.additional_train]) != "916b398de0fc9774041fe174d0e585c862bc68e9aa18fa04028c3126f693a18f":
            raise ValueError("unreviewed documentation training source")
        extra = json.loads(retained[args.additional_train])
        for task in extra["tasks"]:
            if task["split"] != "train" or task["dataset"] != "requests-original-docstrings":
                raise ValueError("documentation source escaped its train-only scope")
        existing_ids = {d["id"] for d in corpus["documents"]}
        for document in extra["documents"]:
            if document["id"] in existing_ids or sha(document["text"].encode()) != document["paragraph_sha256"]:
                raise ValueError("documentation duplicate or source drift")
            existing_ids.add(document["id"])
        additional_count = len(extra["tasks"])
        corpus["documents"].extend(extra["documents"])
        corpus["tasks"].extend(extra["tasks"])
    if sha(retained[args.source / "model.safetensors"]) != "53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db":
        raise ValueError("MiniLM source differs")
    random.seed(1908); np.random.seed(1908); torch.manual_seed(1908); torch.cuda.manual_seed_all(1908)
    torch.set_num_threads(2)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.use_deterministic_algorithms(True)
    device = torch.device("cuda")
    model = BertModel(BertConfig.from_dict(json.loads(retained[args.source / "config.json"])), add_pooling_layer=False)
    tensors = load_tensors(retained[args.source / "model.safetensors"])
    # Pooler is outside the approved attention-mask-mean-L2 embedding function.
    tensors = {k: v for k, v in tensors.items() if not k.startswith("pooler.") and k != "embeddings.position_ids"}
    model.load_state_dict(tensors, strict=True)
    model.to(device)
    tokenizer = Tokenizer.from_str(retained[args.source / "tokenizer.json"].decode())
    tokenizer.enable_truncation(max_length=256)
    tokenizer.enable_padding(pad_id=0, pad_token="[PAD]")
    documents = {row["id"]: row for row in corpus["documents"]}
    train_tasks = [t for t in corpus["tasks"] if t["split"] == "train" and t["dataset"] in ("QASPER", "requests-original-docstrings")]
    train_groups = {t["source_group"] for t in train_tasks}
    allowed = sorted(d["id"] for d in documents.values() if d["source_group"] in train_groups and d["dataset"] in ("QASPER", "requests-original-docstrings"))
    visible_documents = ({d: recipe.visible_prefix(tokenizer, documents[d]["text"]) for d in allowed}
                         if recipe is not None else None)
    examples = []
    known_positives = {}
    for task in train_tasks:
        positives = sorted({location for a in task["human_alternatives"] if not a["unanswerable"] and a["alignment"] == "all_original_evidence_maps_to_allowed_text"
            for evidence in a["evidence"] for location in evidence["allowed_locations"]})
        if not positives:
            continue
        known_positives[task["id"]] = set(positives)
        query_words = set(task["question"].lower().split())
        if recipe is None:
            negatives = sorted((d for d in allowed if d not in positives), key=lambda d: (-len(query_words & set(documents[d]["text"].lower().split())), d))
        else:
            negatives = recipe.negative_order(recipe.visible_prefix(tokenizer, task["question"]), allowed, positives, visible_documents)
        for positive in positives:
            examples.append({"task_id": task["id"], "question": task["question"], "positive": positive, "distractor": negatives[0]})
    if not examples:
        raise ValueError("no original human training positives")
    if recipe is not None:
        retained[args.training_master] = args.training_master.read_bytes()
        if sha(retained[args.training_master]) != "314825dccad822874d3405305df789317c8e90a0a0e4cdcdb590acd27a555c48":
            raise ValueError("frozen315 candidate training master changed")
        recipe.assert_training_rows(examples, json.loads(retained[args.training_master]))
    output.mkdir(parents=True)
    snapshots = output / "input-snapshots"
    snapshots.mkdir()
    for raw in retained.values():
        path = snapshots / sha(raw)
        if not path.exists():
            path.write_bytes(raw)
    (output / "training-rows.private.json").write_text(json.dumps(examples, indent=2) + "\n")
    def encode(texts):
        encoded = tokenizer.encode_batch(texts)
        ids = torch.tensor([e.ids for e in encoded], device=device)
        masks = torch.tensor([e.attention_mask for e in encoded], device=device)
        types = torch.tensor([e.type_ids for e in encoded], device=device)
        hidden = model(input_ids=ids, attention_mask=masks, token_type_ids=types).last_hidden_state
        pooled = (hidden * masks.unsqueeze(-1)).sum(1) / masks.sum(1, keepdim=True).clamp_min(1)
        return torch.nn.functional.normalize(pooled.float(), p=2, dim=1)
    optimizer = torch.optim.AdamW(model.parameters(), lr=2e-5, weight_decay=.01)
    started = time.monotonic()
    epochs = []
    torch.cuda.reset_peak_memory_stats()
    for epoch in range(3):
        order = list(examples)
        random.Random(1908 + epoch).shuffle(order)
        losses = []
        model.train()
        for offset in range(0, len(order), 8):
            rows = order[offset:offset+8]
            optimizer.zero_grad(set_to_none=True)
            queries = encode([r["question"] for r in rows])
            passages = encode([documents[r["positive"]]["text"] for r in rows] + [documents[r["distractor"]]["text"] for r in rows])
            logits = queries @ passages.T / .05
            # Any other original annotated positive for this question is never
            # forced negative, including a different supporting paragraph or
            # another row's hard distractor.
            for i, row in enumerate(rows):
                passage_ids = [r["positive"] for r in rows] + [r["distractor"] for r in rows]
                for j, identifier in enumerate(passage_ids):
                    if i != j and identifier in known_positives[row["task_id"]]:
                        logits[i, j] = -1e4
            loss = torch.nn.functional.cross_entropy(logits, torch.arange(len(rows), device=device))
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.)
            optimizer.step()
            losses.append(float(loss.detach().cpu()))
        checkpoint = output / ("epoch-" + str(epoch+1))
        model.eval(); model.save_pretrained(checkpoint, safe_serialization=True)
        (checkpoint / "tokenizer.json").write_bytes(retained[args.source / "tokenizer.json"])
        epochs.append({"epoch": epoch+1, "steps": len(losses), "mean_loss": sum(losses)/len(losses),
            "model_sha256": sha((checkpoint / "model.safetensors").read_bytes())})
        print(json.dumps(epochs[-1]), flush=True)
    for path, raw in retained.items():
        if path.read_bytes() != raw:
            raise ValueError("training source drift")
    report = {"schema": "airk.development-minilm-training/1", "seed": 1908, "epochs": epochs,
        "training_examples": len(examples), "source_groups": len(train_groups), "source_corpus_sha256": sha(retained[args.corpus]),
        "additional_documentation_tasks": additional_count,
        "elapsed_seconds": time.monotonic()-started, "peak_cuda_allocated_bytes": torch.cuda.max_memory_allocated(),
        "device": torch.cuda.get_device_name(0), "versions": {n: importlib.metadata.version(n) for n in ("torch","transformers","safetensors","tokenizers")},
        "input_seals": {str(p): sha(raw) for p,raw in retained.items()},
        "objective": "InfoNCE with in-batch and lexical hard distractors, batch8, temperature0.05, AdamW2e-5 weightdecay0.01,3epochs, FP32,256tokens; negatives are unjudged distractors, not claimed human irrelevant labels",
        "scope": "Actual training on original QASPER evidence annotations plus explicitly approved historical original developer documentation pairs if requested. Mechanical docstring associations are not independent human context/sufficiency gold. No Hotpot or held-out questions/passages used for mining/training. Development-only checkpoint selection and export precede final evaluation; not a promoted model."}
    if recipe is not None:
        report["schema"] = "airk.development-minilm-training/2"
        report["training_recipe"] = "visible-prefix-mining-v1"
        report["mining_input"] = "Exact configured256-token visible query/passage prefixes; tokenizer state and actual encoded training inputs unchanged"
        report["training_master_sha256"] = sha(retained[args.training_master])
        report["training_seal_sha256"] = args.training_seal_sha256
        execution.verify_seal(args.training_seal, args.training_seal_sha256)
    (output / "receipt.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps({"training_examples":len(examples),"elapsed_seconds":report["elapsed_seconds"],"peak_cuda_allocated_bytes":report["peak_cuda_allocated_bytes"]}))


if __name__ == "__main__":
    main()
