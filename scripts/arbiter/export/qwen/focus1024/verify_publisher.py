# SPDX-License-Identifier: Apache-2.0
"""Compare the wider weight8 CPU graph against unmodified publisher HF weights."""
import gc
import json
from pathlib import Path
import sys
from types import SimpleNamespace

from export_graph import ROOT, SOURCE, PUBLISHER, CHECKPOINT, sha, require, save

QWEN = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(QWEN / "qwen3-reranker06-experiment-r1"))
from run_focus_export import (prepare_torch, load_official, local_tokenizer, compile_pair, tensors, errors,
                              INSTRUCTION, PREFIX, SUFFIX, FALSE_TOKEN, TRUE_TOKEN, ort_session)


def controls(tokenizer):
    original = SOURCE / "native-runtime512-fixture.json"
    sha(original, "b07dbc3bf8167d54d2cb43a07b1cfa361173e7bf1c5207870dfafdcf5d71b080")
    rows = [{k: row[k] for k in ("id","original_query","original_document","scoring_query","scoring_document","native_model_view")}
            for row in json.loads(original.read_bytes())["rows"]]
    require(len(rows) == 12, "original control roster differs")
    query = "Where is the maintenance record?"
    for target in (513, 1024):
        base = "The maintenance record is in the notebook."
        count = compile_pair(tokenizer, query, base, 1024).observed_tokens
        doc = base + " note" * (target - count)
        require(compile_pair(tokenizer, query, doc, 1024).observed_tokens == target, "synthetic boundary changed")
        rows.append(dict(id=f"synthetic-{target}", original_query=query, original_document=doc,
                         scoring_query=query, scoring_document=doc, native_model_view=True))
    try:
        compile_pair(tokenizer, query, doc + " note", 1024)
    except ValueError as error:
        require("1025 tokens; maximum=1024" in str(error), "unexpected over-budget refusal")
    else:
        raise ValueError("1025-token input silently truncated")
    return rows


def verify(output, graph):
    import numpy as np
    import torch
    import onnxruntime as ort
    from transformers import AutoTokenizer
    from importlib.metadata import version

    prepare_torch(torch, SimpleNamespace(device="cpu",threads=4))
    official = load_official(torch, "cpu")
    backend = local_tokenizer()
    publisher = AutoTokenizer.from_pretrained(PUBLISHER, local_files_only=True, trust_remote_code=False, padding_side="left")
    rows = controls(backend)
    for row in rows:
        query, doc = row["scoring_query"], row["scoring_document"]
        pair = compile_pair(backend, query, doc, 1024)
        body = f"<Instruct>: {INSTRUCTION}\n<Query>: {query}\n<Document>: {doc}"
        observed = (publisher.encode(PREFIX, add_special_tokens=False) + publisher(body, padding=False, truncation=False)["input_ids"]
                    + publisher.encode(SUFFIX, add_special_tokens=False))
        require(observed == list(pair.input_ids[-pair.observed_tokens:]), "independent publisher tokens differ")
        row["expected"] = dict(prompt=PREFIX+body+SUFFIX, input_ids=list(pair.input_ids), attention_mask=list(pair.attention_mask),
                               position_ids=list(pair.position_ids), decision_position=1023, candidate_positions=[2152,9693],
                               option_mask=[True,True])
        ids, mask, _ = tensors(torch, pair, "cpu")
        with torch.inference_mode():
            result = official(input_ids=ids, attention_mask=mask, use_cache=False, return_dict=True, logits_to_keep=1)
            baseline = result.logits[:, -1, [FALSE_TOKEN, TRUE_TOKEN]].float().cpu().numpy()
        row["official_logits"] = baseline.flatten().tolist()
        row["unpadded_tokens"] = pair.observed_tokens
        require(np.isfinite(baseline).all(), "nonfinite publisher readout")
    del official, result, baseline
    gc.collect()
    save(output / "publisher.private.json", {"graph":graph["graph"], "rows":rows})
    session = ort_session(ort, output / "assets/model.onnx", 4)
    require(all(inp.shape == [1,1024] for inp in session.get_inputs()), "width incorrect in actual ORT session")
    for row in rows:
        args = {name:np.array([row["expected"][name]], dtype=np.int64) for name in ("input_ids","attention_mask","position_ids")}
        actual = session.run(["logits"], args)[0]
        baseline = np.array([row["official_logits"]], dtype=np.float32)
        require(actual.shape == (1,2) and np.isfinite(actual).all(), "invalid original graph score")
        raw, score = errors(np, actual, baseline)
        row.update(python_weight8_logits=actual.flatten().tolist(), raw_max_error=raw, relevance_error=score,
                   argmax_matches=bool(np.argmax(actual) == np.argmax(baseline)))
    save(output / "native-runtime1024-fixture.json", {"schema":"airk.private-independent-focus-publisher1024-fixture/1",
         "scope":"Publisher synthetic controls, CPU FP32 versus weight8. Original tokens retained.", "input_pins":graph, "rows":rows})
    sha(PUBLISHER/"model.safetensors", CHECKPOINT)
    summary = {"model":"arbiter-focus-qwen3-reranker-06b","width":1024, "reference_rows":len(rows),
               "all_argmax":all(row["argmax_matches"] for row in rows), "max_raw_error":max(row["raw_max_error"] for row in rows),
               "max_relevance_error":max(row["relevance_error"] for row in rows),"overwidth_refusal":1025,
               "versions":{p:version(p) for p in ("torch", "transformers","onnx","onnxruntime","tokenizers")},
               "fixture_sha256":sha(output/"native-runtime1024-fixture.json"),"graph_sha256":graph["graph"]}
    summary["publisher_weight8_limits_passed"] = summary["all_argmax"] and summary["max_raw_error"] <= 0.25 and summary["max_relevance_error"] <= 0.05
    save(output/"numeric.private.json",summary)
    require(summary["publisher_weight8_limits_passed"], "weight8 numerical controls failed")
