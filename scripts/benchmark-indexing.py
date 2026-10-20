# SPDX-License-Identifier: Apache-2.0
"""Measure indexing stages and chunk granularity in fresh, isolated local state.

Requires the optional Python tokenizers package for the chunk audit. Never opens
an existing collection. Reports contain aggregate measurements, not source text.
"""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import sqlite3
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


def current_inventory(state):
    """Diagnostic audit of this harness's fresh, quiescent 1.2 state only.

    This is not an authenticated public reader: native requests own that contract.
    Bound decoding, check domain keys/counts, and verify retained payload hashes.
    """
    databases = list(state.glob("collections-1.2/*/current.sqlite3"))
    if len(databases) != 1:
        raise ValueError(f"expected one fresh current database, found {len(databases)}")
    database = sqlite3.connect(databases[0].resolve().as_uri() + "?mode=ro", uri=True)
    files = {}
    counts = None
    consumed = 0
    try:
        database.execute("BEGIN")
        if database.execute("PRAGMA user_version").fetchone()[0] != 120:
            raise ValueError("benchmark requires current format 120")
        phase, raw_head = database.execute("SELECT phase, head FROM current_state WHERE id=1").fetchone()
        if phase != "ready" or len(raw_head) > 65536:
            raise ValueError("benchmark requires a bounded Ready head")
        scope = json.loads(raw_head)["scope"]
        scope_hash = framed_digest(b"current-keyed-map-scope/1", scope["root"].encode(),
                                   scope["collection"].encode(), bytes.fromhex(scope["incarnation"]))

        def key(domain, identity):
            return framed_digest(b"current-keyed-map-key/1", scope_hash, domain, identity)

        for prefix, bucket in database.execute("SELECT prefix, content FROM current_map_buckets"):
            consumed += len(bucket)
            if not 2 <= len(bucket) <= 512 * 1024 or consumed > 512 * 1024 * 1024:
                raise ValueError("benchmark map byte bound exceeded")
            count = int.from_bytes(bucket[:2], "big")
            if not 1 <= count <= 128:
                raise ValueError("invalid map entry count")
            offset, previous = 2, b""
            for _ in range(count):
                if offset + 36 > len(bucket):
                    raise ValueError("truncated map entry")
                identity = bucket[offset:offset+32]
                length = int.from_bytes(bucket[offset+32:offset+36], "big")
                offset += 36
                if (length > 256 * 1024 or offset + length > len(bucket)
                        or int.from_bytes(identity[:2], "big") != prefix or identity <= previous):
                    raise ValueError("invalid map entry")
                raw = bucket[offset:offset+length]
                offset += length
                previous = identity
                if identity == key(b"metadata", b"collection"):
                    counts = json.loads(raw)["counts"]
                # Other map domains include binary handles and measured-size rows.
                if not raw.startswith(b'{"artifact":'):
                    continue
                value = json.loads(raw)["artifact"]
                if "retained" not in value or "content_hash" not in value:
                    continue
                path = value["path"]
                if identity != key(b"artifact", path.encode()) or path in files:
                    raise ValueError("invalid artifact map identity")
                if len(files) >= 100000:
                    raise ValueError("benchmark file bound exceeded")
                files[path] = {"state": {"source_hash": value["content_hash"],
                                          "payload_digest": value["retained"]["object"]}}
            if offset != len(bucket):
                raise ValueError("trailing map bytes")
        if counts is None or len(files) != counts["files"]:
            raise ValueError("incomplete current benchmark inventory")
    finally:
        database.close()
    return databases[0].parent, {"files": files}, counts


def audit(state, tokenizer_path):
    from tokenizers import Tokenizer

    tokenizer = Tokenizer.from_file(str(tokenizer_path))
    tokenizer.no_padding()
    tokenizer.no_truncation()
    collection, inventory, indexed_counts = current_inventory(state)
    corpus_digest = hashlib.sha256(json.dumps(
        [(path, record["state"]["source_hash"]) for path, record in sorted(inventory["files"].items())],
        ensure_ascii=True, separators=(",", ":")).encode()).hexdigest()
    counts = Counter()
    groups = {}
    unique = set()
    token_lengths = []
    per_file = []
    for path, record in inventory["files"].items():
        digest = record["state"]["payload_digest"]
        if len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
            raise ValueError("invalid retained payload digest")
        payload_path = collection / "objects" / "artifacts" / digest
        raw = payload_path.read_bytes()
        assert hashlib.sha256(raw).hexdigest() == digest
        payload = json.loads(raw)
        group = "tests" if "/tests/" in path or "/test/" in path else (
            "docs" if path.startswith("docs/") or path.endswith(".md") else (
                "locks" if Path(path).name in ("Cargo.lock", "package-lock.json") else
                "code/config/other"))
        subtotal = groups.setdefault(group, Counter())
        nonblank = 0
        for chunk in payload["chunks"]:
            region = chunk.get("region")
            text = payload["text"] if region is None else next(
                item["text"] for item in payload["regions"] if item["id"] == region)
            span = chunk["span"]
            text = text.encode()[span["start"]:span["end"]].decode()
            counts["chunks"] += 1
            subtotal["chunks"] += 1
            counts["chunk_bytes"] += len(text.encode())
            if not text.strip():
                counts["blank"] += 1
                subtotal["blank"] += 1
                continue
            ids = tokenizer.encode(text, add_special_tokens=True).ids
            size = len(ids)
            token_lengths.append(size)
            counts["nonblank"] += 1
            nonblank += 1
            subtotal["nonblank"] += 1
            subtotal["bytes"] += len(text.encode())
            unique.add(hashlib.sha256(text.encode()).digest())
            for threshold in (16, 32, 64):
                counts[f"tokens_le_{threshold}"] += size <= threshold
            counts["tokens_gt_256"] += size > 256
            counts["untruncated_tokens"] += size
            counts["used_slots_256"] += min(size, 256)
        counts["facts"] += len(payload["facts"])
        counts["payload_relationships"] += len(payload["relationships"])
        counts["continuity"] += len(payload["continuity"])
        per_file.append({"path": path, "nonblank_chunks": nonblank})
    counts["unique_nonblank_inputs"] = len(unique)
    counts["catalog_facts"] = indexed_counts["facts"]
    # No current aggregate includes every constructed native relationship.
    counts["catalog_relationships"] = None
    ordered = sorted(token_lengths)
    return {"corpus_path_hash_digest": corpus_digest, "counts": dict(counts), "groups": groups,
            "top_files_by_nonblank_chunks": sorted(per_file, key=lambda row: (-row["nonblank_chunks"], row["path"]))[:12],
            "token_quantiles": {str(p): ordered[min(len(ordered)-1, int(len(ordered)*p))]
                                for p in (0.5, 0.9, 0.95, 0.99)} if ordered else {},
            "padding_fraction_256": 1 - counts["used_slots_256"] / max(1, counts["nonblank"]*256)}


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
                              "audit reads only harness-owned quiescent current state; it is not an authenticated evidence API",
                              "catalog_relationships is unavailable; payload counts exclude constructed native relationships"]}

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
            entry["audit"] = audit(state, model / "tokenizer.json")
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
