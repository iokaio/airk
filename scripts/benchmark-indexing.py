# SPDX-License-Identifier: Apache-2.0
"""Measure indexing stages in fresh, isolated local state.

Never opens
an existing collection. Reports contain aggregate measurements, not source text.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time


def padding_probe(model):
    """Independent-engine feasibility only; this does not benchmark AIRK tract."""
    import numpy as np
    import onnxruntime as ort
    from tokenizers import Tokenizer

    tok = Tokenizer.from_file(str(model / "tokenizer.json"))
    tok.no_padding()
    tok.no_truncation()
    options = ort.SessionOptions()
    options.intra_op_num_threads = 1
    options.inter_op_num_threads = 1
    session = ort.InferenceSession(str(model / "model.onnx"), options,
                                   providers=["CPUExecutionProvider"])
    texts = [f"Reuse unchanged embedding {n} from the local content cache." for n in range(32)]
    encoded = tok.encode_batch(texts)
    assert max(len(e.ids) for e in encoded) <= 32

    def embed(length, batch):
        result = []
        for start in range(0, len(encoded), batch):
            part = encoded[start:start+batch]
            ids = np.zeros((len(part), length), dtype=np.int64)
            mask = np.zeros_like(ids)
            for i, value in enumerate(part):
                ids[i, :len(value.ids)] = value.ids
                mask[i, :len(value.ids)] = 1
            states = session.run(None, {"input_ids": ids, "attention_mask": mask,
                                        "token_type_ids": np.zeros_like(ids)})[0]
            summed = (states.astype(np.float64) * mask[:, :, None]).sum(axis=1)
            result.extend(summed / np.linalg.norm(summed, axis=1, keepdims=True))
        return np.asarray(result)

    baseline = embed(256, 1)
    observations = []
    for length, batch in [(256, 1), (128, 1), (64, 1), (32, 1), (64, 8), (32, 8)]:
        embed(length, batch)
        times = []
        for _ in range(3):
            start = time.perf_counter()
            actual = embed(length, batch)
            times.append(time.perf_counter() - start)
        observations.append({"length": length, "batch": batch, "seconds": times,
                             "median_seconds": statistics.median(times),
                             "min_cosine": float((actual * baseline).sum(axis=1).min()),
                             "max_component_delta": float(np.abs(actual-baseline).max())})
    return {"engine": "onnxruntime " + ort.__version__, "threads": 1, "texts": len(texts),
            "max_tokens_including_special": max(len(e.ids) for e in encoded),
            "limitations": "Independent-engine feasibility; not AIRK tract performance or exact-cache compatibility.",
            "observations": observations}


def framed_digest(domain, *fields):
    digest = hashlib.sha256(b"AIRK-DIGEST-1\0")
    for field in (domain, *fields):
        digest.update(len(field).to_bytes(8, "big"))
        digest.update(field)
    return digest.digest()


def fixture(root):
    topics = [
        ("account", "Reset a forgotten password using a time-limited recovery link sent to a verified email address."),
        ("bread", "Mix flour and yeast, allow dough to rise, then bake the bread in a hot oven."),
        ("cache", "Reuse an unchanged embedding from a content hash cache instead of repeating model inference."),
        ("publish", "Commit captured index changes atomically so readers receive one coherent current revision."),
    ]
    for name, text in topics:
        (root / f"{name}.md").write_text(
            f"# {name.title()}\n\n" + "\n\n".join(f"{text} Example {n}." for n in range(12)) + "\n", encoding="utf-8")
        (root / f"{name}.rs").write_text("\n".join(
            f"/// {text}\npub fn {name}_{n}() -> usize {{ {n} }}\n" for n in range(12)), encoding="utf-8")
    (root / "settings.json").write_text(json.dumps({f"option_{n}": n for n in range(40)}, indent=2), encoding="utf-8")


def tail_probe(binary, model, base):
    root = base / "source"
    root.mkdir()
    prefix = "General operational background describes routine procedures and standard maintenance. " * 60
    endings = {"a_bread.md": "Mix flour and yeast to bake bread in a hot oven.",
               "z_account.md": "Reset a forgotten password using a recovery link sent to the verified email address."}
    for name, ending in endings.items():
        (root / name).write_text(prefix + "\n\n" + ending + "\n", encoding="utf-8")
    results = []
    for detail in ("detailed", "balanced", "essential"):
        state = base / detail

        def run(*command):
            result = subprocess.run([str(binary), "--root", str(root), "--data-dir", str(state), *command],
                                    capture_output=True, timeout=300,
                                    env={**os.environ, "AIRK_USAGE_MODE": "disabled", "AIRK_INDEX_WORKERS": "4"})
            if result.returncode:
                raise RuntimeError(result.stderr.decode(errors="replace"))
            return json.loads(result.stdout)

        run("collections", "extraction", "detail", detail)
        run("collections", "vectors", "install", str(model))
        indexed = run("index")
        status = run("status")
        searches = []
        for mode, query in [("vector", "Recover account access after forgetting a login secret"),
                            ("lexical", "password recovery"),
                            ("hybrid", "password recovery")]:
            found = run("search", query, "--mode", mode, "--limit", "1")
            searches.append({"mode": mode, "query": query,
                             "citations": [h["citation"] for h in found["hits"]]})
        results.append({"detail": detail, "chunks": status["chunks"],
                        "vectors": status["indexed_vectors"], "index": indexed, "searches": searches})
    return {"schema": "airk-indexing-tail-probe/1", "results": results,
            "limitation": "Two original documents with identical long prefixes; one adversarial query, not a quality benchmark."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--corpus", type=Path, help="Read-only corpus audit; vectors disabled in isolated state")
    parser.add_argument("--work-dir", type=Path, default=Path("target"))
    parser.add_argument("--padding-probe", action="store_true", help="Only run independent ONNX Runtime padding/batch probe")
    parser.add_argument("--tail-probe", action="store_true", help="Only test retrieval of evidence after a long shared prefix")
    args = parser.parse_args()
    if sum((bool(args.corpus), args.padding_probe, args.tail_probe)) > 1:
        parser.error("choose only one of --corpus, --padding-probe and --tail-probe")
    binary = args.binary.resolve(strict=True)
    model = args.model_dir.resolve(strict=True)
    if args.padding_probe:
        report = padding_probe(model)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(report, indent=2))
        return
    args.work_dir.mkdir(parents=True, exist_ok=True)
    # Keep generated state for local diagnosis; never stage/package this directory.
    base = Path(tempfile.mkdtemp(prefix="indexing-review-", dir=args.work_dir.resolve()))
    if args.tail_probe:
        report = tail_probe(binary, model, base)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(report, indent=2))
        return
    root = args.corpus.resolve(strict=True) if args.corpus else base / "source"
    if not args.corpus:
        root.mkdir()
        fixture(root)
    results = []
    report = {"schema": "airk-indexing-review/1", "platform": platform.platform(),
              "logical_cpus": os.cpu_count(), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "model_manifest": json.loads((model / "model.json").read_bytes()), "results": results,
              "limitations": ["fresh processes; OS caches uncontrolled; timings are observations, not performance guarantees",
                              "synthetic retrieval relevance is a smoke test, not production quality qualification",
                              "report excludes source text and retained state; working files remain under work-dir",
                              "measurements use native CLI contracts; private database formats are not decoded"]}

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    for detail in ("detailed", "balanced", "essential"):
        for vectors in ((False,) if args.corpus else (False, True)):
            state = base / f"{detail}-{'vectors' if vectors else 'lexical'}"
            entry = {"detail": detail, "vectors": vectors, "runs": []}
            results.append(entry)

            def run(label, *command, workers=4):
                started = time.perf_counter_ns()
                process = subprocess.run([str(binary), "--root", str(root), "--data-dir", str(state),
                                          "--collection", "review", *command], capture_output=True,
                                         env={**os.environ, "AIRK_USAGE_MODE": "disabled", "AIRK_INDEX_WORKERS": str(workers)},
                                         timeout=1800)
                elapsed = (time.perf_counter_ns() - started) // 1000
                if process.returncode:
                    raise RuntimeError(process.stderr.decode(errors="replace"))
                value = json.loads(process.stdout)
                if label:
                    entry["runs"].append({"label": label, "wall_us": elapsed, "result": value})
                    save()
                    print(detail, vectors, label, round(elapsed/1e6, 3), flush=True)
                return value

            run(None, "collections", "extraction", "detail", detail)
            if vectors:
                run(None, "collections", "vectors", "install", str(model))
            else:
                run(None, "collections", "vectors", "set", "--enabled", "false")
            run("cold", "index")
            status = run(None, "status")
            entry["status"] = {key: status[key] for key in ("files", "chunks", "indexed_vectors", "source_bytes")}
            entry["logical_disk_bytes_after_cold"] = sum(p.stat().st_size for p in state.rglob("*") if p.is_file())
            run("unchanged", "index")
            if not args.corpus:
                queries = ["Recover account access after forgetting a login secret", "avoid recomputing unchanged embeddings",
                           "bread dough yeast", "coherent revision after failure"]
                entry["retrieval"] = []
                for mode in (("lexical", "vector", "hybrid") if vectors else ("lexical",)):
                    for query in queries:
                        value = run(None, "search", query, "--mode", mode, "--limit", "5", "--max-bytes", "12000")
                        entry["retrieval"].append({"mode": mode, "query": query,
                            "paths": [h["citation"]["path"] for h in value["hits"]]})
                if vectors and detail == "detailed":
                    run("full_serial", "index", "--full", workers=1)
                    run("full_parallel", "index", "--full", workers=4)
                (root / "added.txt").write_text("A new original passage about indexing latency.\n", encoding="utf-8")
                changed = run("one_file_added", "index")
                if vectors:
                    assert changed["current"]["changed_artifacts"] == 1
                    assert changed["embedding_cache_hits"] == 0
                    assert changed["embedding_inputs"] <= changed["vector_units"]
                (root / "added.txt").unlink()
                run("one_file_deleted", "index")
            save()
    print(f"Report: {args.output}; isolated working files: {base}")


if __name__ == "__main__":
    main()
