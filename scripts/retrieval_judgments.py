# SPDX-License-Identifier: Apache-2.0
"""Frozen original relevance judgments and file-aware top-ten scoring."""
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import statistics


def load(path):
    raw = Path(path).read_bytes()
    if len(raw) > 1 << 20:
        raise ValueError("relevance corpus exceeds byte limit")
    corpus = json.loads(raw)
    if corpus.get("schema") != "airk.relevance-corpus/1":
        raise ValueError("unsupported relevance corpus schema")
    sources, queries = corpus["sources"], corpus["queries"]
    if not isinstance(sources, dict) or not 1 <= len(sources) <= 64:
        raise ValueError("invalid relevance source inventory")
    for name, text in sources.items():
        parts = PurePosixPath(name)
        if (not name or parts.is_absolute() or str(parts) != name
                or ".." in parts.parts or "\\" in name or ":" in name
                or not isinstance(text, str)):
            raise ValueError("invalid relevance source path or content")
    if not isinstance(queries, list) or not 1 <= len(queries) <= 64:
        raise ValueError("invalid relevance query inventory")
    control = corpus.get("index_control")
    if control is not None and (not isinstance(control, dict)
            or control.get("path") not in sources or not isinstance(control.get("append"), str)
            or not 1 <= len(control["append"].encode("utf-8")) <= 1024):
        raise ValueError("invalid owned index control")
    probe = corpus.get("compact_plan_probe")
    if probe is not None and (not isinstance(probe, dict) or probe.get("path") not in sources
            or not isinstance(probe.get("name"), str) or not 1 <= len(probe["name"]) <= 256):
        raise ValueError("invalid compact plan probe")
    ids = set()
    for query in queries:
        if query["id"] in ids:
            raise ValueError("duplicate relevance query ID")
        ids.add(query["id"])
        if query["split"] not in ("development", "held_out"):
            raise ValueError("invalid relevance split")
        if (not isinstance(query["query"], str) or not query["query"].strip()
                or not isinstance(query["grades"], dict)
                or not query["modes"] or len(set(query["modes"])) != len(query["modes"])
                or not set(query["modes"]) <= {"lexical", "vector", "hybrid"}):
            raise ValueError("invalid relevance query")
        prefix = query.get("path_prefix", "")
        for name, grade in query["grades"].items():
            if (name not in sources or not name.startswith(prefix)
                    or type(grade) is not int or not 0 <= grade <= 3):
                raise ValueError("invalid or out-of-scope relevance grade")
    return corpus, hashlib.sha256(raw).hexdigest()


def score(paths, grades):
    """Repeated files consume their original chunk rank without extra gain."""
    limit = 10
    seen, gains = set(), []
    for path in paths[:limit]:
        grade = grades.get(path, 0) if path not in seen else 0
        gains.append((2 ** grade - 1) / math.log2(len(gains) + 2))
        seen.add(path)
    ideal = sum((2 ** grade - 1) / math.log2(rank + 2)
                for rank, grade in enumerate(sorted(grades.values(), reverse=True)[:limit]))
    relevant = {path for path, grade in grades.items() if grade > 0}
    return {"ndcg_at_10": sum(gains) / ideal if ideal else None,
            "recall_at_10": len(seen & relevant) / len(relevant) if relevant else None,
            "relevant_files_returned": len(seen & relevant),
            "relevant_files": len(relevant), "returned_chunks": len(paths[:limit]),
            "negative_empty": not paths if not relevant else None}


def summarize(records):
    """Preserve failed attempts and never substitute a relevance score for them."""
    groups = {}
    for record in records:
        key = f"{record['mode']}/{record['split']}"
        group = groups.setdefault(key, {"attempts": 0, "failures": 0,
                                        "ndcg": [], "recall": [], "negative": [], "latency": []})
        group["attempts"] += 1
        if record.get("status") != "ok":
            group["failures"] += 1
            continue
        for field, name in (("ndcg_at_10", "ndcg"), ("recall_at_10", "recall"),
                            ("negative_empty", "negative")):
            if record[field] is not None:
                group[name].append(record[field])
        if "latency_ms" in record:
            group["latency"].append(record["latency_ms"])
    return {key: {"attempts": value["attempts"], "failures": value["failures"],
                  "scored_queries": len(value["ndcg"]),
                  "negative_queries": len(value["negative"]),
                  "latency_samples": len(value["latency"]),
                  "p50_ms": statistics.median(value["latency"]) if value["latency"] else None,
                  "p95_ms": sorted(value["latency"])[math.ceil(len(value["latency"]) * .95) - 1] if value["latency"] else None,
                  "ndcg_at_10": sum(value["ndcg"]) / len(value["ndcg"]) if value["ndcg"] else None,
                  "recall_at_10": sum(value["recall"]) / len(value["recall"]) if value["recall"] else None,
                  "negative_empty_rate": sum(value["negative"]) / len(value["negative"]) if value["negative"] else None}
            for key, value in groups.items()}
