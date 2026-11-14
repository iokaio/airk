# SPDX-License-Identifier: Apache-2.0
"""Deterministic, development-only MiniLM retrieval baseline over pinned Git bytes.

This is a CPU embedding/retrieval component experiment, not canonical AIRK,
Workbench, MCP, sufficiency or downstream-agent qualification.
"""
import argparse
import hashlib
import importlib.metadata
import json
import math
from pathlib import Path
import re
import subprocess
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def retain_inputs(paths):
    return {str(path.resolve()): path.read_bytes() for path in paths}


def check_seals(seals):
    for path, expected in seals.items():
        try:
            actual = sha256(Path(path).read_bytes())
        except OSError as error:
            raise ValueError("baseline consumed input became unavailable") from error
        if actual != expected:
            raise ValueError("baseline consumed inputs changed during the run")


def git_bytes(revision, path):
    if not re.fullmatch(r"[0-9a-f]{40}", revision) or path.startswith(("/", "\\")) or ".." in Path(path).parts:
        raise ValueError("invalid fixed source identity")
    return subprocess.check_output(["git", "-c", f"safe.directory={ROOT.as_posix()}",
                                    "show", f"{revision}:{path}"], cwd=ROOT)


def documents(corpus):
    result = []
    for source in corpus["sources"]:
        data = git_bytes(corpus["revision"], source["path"])
        if sha256(data) != source["sha256"]:
            raise ValueError(f"frozen corpus source differs: {source['path']}")
        lines = data.decode("utf-8").splitlines(keepends=True)
        # This fixed developer recipe does not reproduce native extraction.
        for start in range(0, len(lines), 20):
            text = "".join(lines[start:start + 24])
            result.append({"id": f"{source['path']}:{start + 1}", "path": source["path"],
                           "start_line": start + 1, "end_line": min(start + 24, len(lines)),
                           "source_sha256": source["sha256"], "text": text})
    if not result:
        raise ValueError("empty corpus")
    return result


def gold_sets(tasks, docs):
    ids = set()
    result = {}
    for task in tasks:
        if task["id"] in ids or task["split"] not in {"train", "development", "calibration"}:
            raise ValueError("duplicate task or invalid pilot split")
        ids.add(task["id"])
        groups = []
        for role in task["required_evidence"]:
            matched = {index for index, doc in enumerate(docs)
                       if doc["path"] == role["path"] and role["anchor"] in doc["text"]}
            if not matched:
                raise ValueError(f"missing independent gold anchor: {task['id']}/{role['role']}")
            groups.append(matched)
        if not groups:
            raise ValueError("task has no independent evidence roles")
        result[task["id"]] = groups
    if not result:
        raise ValueError("empty task selection")
    return result


def assess(ranked, groups, top_k):
    selected = set(ranked[:top_k])
    hit = [bool(selected & group) for group in groups]
    relevant = set().union(*groups)
    gains = [int(index in relevant) / math.log2(rank + 2) for rank, index in enumerate(ranked[:top_k])]
    ideal = sum(1 / math.log2(rank + 2) for rank in range(min(top_k, len(relevant))))
    return {"role_recall": sum(hit) / len(hit), "all_roles": all(hit),
            "ndcg": sum(gains) / ideal, "roles_hit": hit}


def model_session(model_dir, catalog_bytes):
    import onnxruntime as ort
    from tokenizers import Tokenizer
    catalog = json.loads(catalog_bytes)
    manifest_path = model_dir / "model.json"
    retained = retain_inputs([manifest_path])
    manifest = json.loads(retained[str(manifest_path.resolve())])
    profile = next((p for p in catalog["profiles"] if p["manifest"] == manifest), None)
    if profile is None:
        raise ValueError("baseline needs an exact existing approved MiniLM profile")
    for asset in profile["assets"]:
        asset_path = model_dir / asset["name"]
        data = asset_path.read_bytes()
        if len(data) != asset["bytes"] or sha256(data) != asset["sha256"]:
            raise ValueError(f"baseline model asset differs: {asset['name']}")
        retained[str(asset_path.resolve())] = data
    tokenizer = Tokenizer.from_str(retained[str((model_dir / "tokenizer.json").resolve())].decode("utf-8"))
    tokenizer.no_padding()
    tokenizer.no_truncation()
    options = ort.SessionOptions()
    options.intra_op_num_threads = 1
    options.inter_op_num_threads = 1
    options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
    session = ort.InferenceSession(retained[str((model_dir / "model.onnx").resolve())], sess_options=options,
                                   providers=["CPUExecutionProvider"])
    return profile, tokenizer, session, {path: sha256(data) for path, data in retained.items()}


def embed(texts, tokenizer, session):
    import numpy as np
    vectors, observations = [], []
    for text in texts:
        tokenizer.no_truncation()
        original = len(tokenizer.encode(text).ids)
        tokenizer.enable_truncation(max_length=256, stride=0)
        encoding = tokenizer.encode(text)
        length = next(n for n in (32, 64, 128, 256) if n >= len(encoding.ids))
        encoding.pad(length, pad_id=tokenizer.token_to_id("[PAD]"), pad_token="[PAD]")
        mask = np.asarray([encoding.attention_mask], dtype=np.int64)
        started = time.monotonic()
        output = session.run(["last_hidden_state"], {
            "input_ids": np.asarray([encoding.ids], dtype=np.int64), "attention_mask": mask,
            "token_type_ids": np.asarray([encoding.type_ids], dtype=np.int64),
        })[0]
        pooled = (output.astype(np.float64) * mask[..., None]).sum(axis=1)[0]
        norm = np.linalg.norm(pooled)
        if pooled.shape != (384,) or not np.isfinite(pooled).all() or norm == 0:
            raise ValueError("invalid native-oracle embedding")
        vectors.append(pooled / norm)
        observations.append({"input_tokens": original, "truncated": original > 256,
                             "shape": length, "elapsed_seconds": time.monotonic() - started})
    return np.asarray(vectors), observations


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--corpus", type=Path, default=HERE / "pilot.json")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    destination = args.output.resolve()
    if not destination.is_relative_to(ROOT / "artifacts"):
        parser.error("output must be private campaign evidence under artifacts")
    if destination.exists():
        parser.error("output already exists; retain each run under a fresh identity")
    started = time.monotonic()
    input_paths = [Path(__file__), args.corpus, ROOT / "crates/airk-retrieval/src/model_catalog.json"]
    retained = retain_inputs(input_paths)
    seals = {path: sha256(data) for path, data in retained.items()}
    corpus_bytes = retained[str(args.corpus.resolve())]
    corpus = json.loads(corpus_bytes)
    docs = documents(corpus)
    gold = gold_sets(corpus["tasks"], docs)
    profile, tokenizer, session, model_seals = model_session(
        args.model_dir, retained[str((ROOT / "crates/airk-retrieval/src/model_catalog.json").resolve())])
    seals.update(model_seals)
    vectors, doc_observations = embed([row["text"] for row in docs], tokenizer, session)
    queries, query_observations = embed([row["question"] for row in corpus["tasks"]], tokenizer, session)
    rows = []
    for task, query, observation in zip(corpus["tasks"], queries, query_observations):
        scores = vectors @ query
        ranked = sorted(range(len(docs)), key=lambda i: (-float(scores[i]), docs[i]["id"]))
        rows.append({"id": task["id"], "split": task["split"], "question": task["question"],
                     "required_evidence": task["required_evidence"], "query_observation": observation,
                     "at_5": assess(ranked, gold[task["id"]], 5),
                     "at_10": assess(ranked, gold[task["id"]], 10),
                     "hits": [{**{key: value for key, value in docs[i].items() if key != "text"},
                               "score": float(scores[i]), "text_sha256": sha256(docs[i]["text"].encode())}
                              for i in ranked[:10]]})
    report = {"schema": "airk.development-retrieval-baseline/1", "corpus_sha256": sha256(corpus_bytes),
              "source_revision": corpus["revision"], "sources": corpus["sources"],
              "author": corpus["author"], "review": corpus["review"],
              "recipe": {"lines": 24, "stride": 20, "max_tokens": 256, "batch": 1, "threads": 1},
              "model": profile, "documents": len(docs), "document_observations": doc_observations,
              "versions": {name: importlib.metadata.version(name) for name in ("numpy", "onnxruntime", "tokenizers")},
              "tasks": rows, "selected_tasks": len(rows),
              "all_roles_at_10": sum(row["at_10"]["all_roles"] for row in rows),
              "mean_role_recall_at_10": sum(row["at_10"]["role_recall"] for row in rows) / len(rows),
              "mean_ndcg_at_10": sum(row["at_10"]["ndcg"] for row in rows) / len(rows),
              "elapsed_seconds": time.monotonic() - started,
              "input_seals": seals,
              "scope": "Development-only fixed-corpus CPU retrieval pilot. Not native extraction, canonical transport, context sufficiency, downstream goal or final model qualification."}
    destination.parent.mkdir(parents=True, exist_ok=True)
    check_seals(seals)
    with destination.open("x", encoding="utf-8") as output:
        output.write(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: report[key] for key in ("selected_tasks", "documents", "all_roles_at_10", "mean_role_recall_at_10", "mean_ndcg_at_10", "elapsed_seconds")}))


if __name__ == "__main__":
    main()
