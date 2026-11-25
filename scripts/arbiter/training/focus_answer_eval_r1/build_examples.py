# SPDX-License-Identifier: Apache-2.0
"""Build Focus training and evaluation examples from mined Answer-evaluation pairs.

Inputs: the miner's `{repo}.jsonl` + `stats.json`
(`scripts/arbiter/training/answer_eval_pairs/mine_pairs.py`), each repository's
checkout (read-only git plumbing only), the Focus tokenizer, and the pilot's
replay files. Output: a fresh private folder with `train.jsonl`, `eval.jsonl`,
`stats.json` and `manifest.json` (every input pinned by SHA-256).

Every document is rendered the way the evaluation kernel shows a hit to Focus:
a hit-like line window around the region, then `focus_window` (ported in
`focus_text.py`), then the SoftwareEvidenceV1 pair template at width 1024.
Over-width pairs are refused and counted, never cropped.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import importlib.util
import json
import math
import random
import re
import sys
from pathlib import Path, PurePosixPath

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import corpus_admission as admission  # noqa: E402
import focus_text as ft  # noqa: E402

SCHEMA = "airk.private.focus-answer-eval-examples/1"
SEED = "airk.focus-answer-eval.r1.seed1908"
QUERY_CHARS = 600
HIT_TARGET = 1200
HIT_CAP = 4096
WINDOW_BYTES = 1200
WINDOW_LINES = 60
PATH_HELDOUT_PERCENT = 10
POSITIVES_PER_PAIR = 2
PER_PATH_CAP = 3
CODE_EXT = {".rs", ".ts", ".tsx", ".js", ".mjs", ".cjs", ".vue", ".cs", ".py"}
DOC_EXT = {".md"}
NEVER_READ = {"aikeys.json"}


def h(*parts: str) -> str:
    return hashlib.sha256("\n".join((SEED,) + parts).encode("utf-8")).hexdigest()


def file_sha(path: Path) -> str:
    with Path(path).open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def load_miner():
    path = HERE.parent / "answer_eval_pairs" / "mine_pairs.py"
    spec = importlib.util.spec_from_file_location("answer_eval_mine_pairs", path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module, path


def cap_query(text: str, limit: int = QUERY_CHARS) -> str:
    text = text.strip()
    if len(text) <= limit:
        return text
    cut = text[:limit]
    space = max(cut.rfind(" "), cut.rfind("\n"))
    return (cut[:space] if space > limit // 2 else cut).rstrip()


def ext_class(path: str) -> str:
    suffix = PurePosixPath(path.lower()).suffix
    return "doc" if suffix in DOC_EXT else "code"


TOKEN_RE = re.compile(r"[A-Za-z][A-Za-z0-9]+")
CAMEL_RE = re.compile(r"[A-Z]+(?![a-z])|[A-Z]?[a-z]+|[0-9]+")
STOPWORDS = set("the and for that this with from are was were not but you your have has can will into when then than its use used using test tests should".split())


def terms(text: str):
    out = []
    for word in TOKEN_RE.findall(text):
        for part in CAMEL_RE.findall(word) or [word]:
            part = part.lower()
            if len(part) >= 3 and part not in STOPWORDS:
                out.append(part)
    return out


class Bm25:
    def __init__(self, docs):
        self.docs = docs
        self.lengths = []
        self.postings = collections.defaultdict(list)
        for index, doc in enumerate(docs):
            counts = collections.Counter(terms(doc))
            self.lengths.append(sum(counts.values()) or 1)
            for term, count in counts.items():
                self.postings[term].append((index, count))
        self.n = len(docs)
        self.avg = sum(self.lengths) / max(self.n, 1)

    def top(self, query: str, allowed, limit: int = 20):
        scores = collections.defaultdict(float)
        qterms = sorted(set(terms(query)), key=lambda t: len(self.postings.get(t, ())))[:32]
        for term in qterms:
            posting = self.postings.get(term)
            if not posting or len(posting) > self.n // 4:
                continue
            idf = math.log(1 + (self.n - len(posting) + 0.5) / (len(posting) + 0.5))
            for index, count in posting:
                norm = count * 2.2 / (count + 1.2 * (0.25 + 0.75 * self.lengths[index] / self.avg))
                scores[index] += idf * norm
        ranked = sorted(scores.items(), key=lambda item: (-item[1], item[0]))
        return [index for index, _ in ranked if allowed(index)][:limit]


class Repository:
    """Read-only text access to one repository through the miner's git plumbing."""

    def __init__(self, miner, name: str, path: Path, snapshot: str, *, protected=None):
        self.miner, self.name, self.snapshot = miner, name, snapshot
        self.protected = protected
        self.git = miner.Git(path, l7=True) if protected is not None else miner.Git(path)
        self.cache = {}
        self.snapshot_paths = None

    def readable(self, path: str) -> bool:
        if self.protected is not None and (not self.miner.l7_policy.safe_relative_path(path) or self.protected(path)):
            return False
        if PurePosixPath(path).name.lower() in NEVER_READ:
            return False
        if self.miner.path_exclusion(self.name, path) is not None:
            return False
        return PurePosixPath(path.lower()).suffix in CODE_EXT | DOC_EXT

    def regular_paths(self, commit: str, *selectors: str) -> set[str]:
        if not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", commit):
            raise ValueError("Focus source read requires an exact Git commit")
        tree = self.git.run("ls-tree", "-r", "--full-tree", "-z", commit, "--", *selectors).split("\0")
        if tree[-1]:
            raise ValueError("malformed Focus Git tree listing")
        paths = set()
        for row in tree[:-1]:
            metadata, separator, path = row.partition("\t")
            fields = metadata.split()
            if not separator or len(fields) != 3:
                raise ValueError("malformed Focus Git tree row")
            mode, kind, _oid = fields
            if mode in self.miner.REGULAR_MODES and kind == "blob" and self.readable(path):
                paths.add(path)
        return paths

    def lines(self, commit: str, path: str):
        key = (commit, path)
        if key not in self.cache:
            text = None
            if self.readable(path):
                if self.protected is None:
                    allowed = True
                elif commit == self.snapshot and self.snapshot_paths is not None:
                    allowed = path in self.snapshot_paths
                else:
                    allowed = path in self.regular_paths(commit, path)
                if allowed:
                    text = self.miner.text_of(self.git.blob(commit, path))
            self.cache[key] = None if text is None else text.replace("\r\n", "\n").split("\n")
            if len(self.cache) > 4096:
                self.cache.pop(next(iter(self.cache)))
        return self.cache[key]

    def snapshot_windows(self):
        if self.protected is None:
            listing = self.git.run("ls-tree", "-r", "--name-only", self.snapshot)
            paths = [path for path in listing.splitlines() if path and self.readable(path)]
        else:
            self.snapshot_paths = self.regular_paths(self.snapshot)
            paths = self.snapshot_paths
        windows = []
        for path in sorted(paths):
            lines = self.lines(self.snapshot, path)
            if not lines:
                continue
            start, size = 1, 0
            for number, line in enumerate(lines, 1):
                size += len(line.encode("utf-8")) + 1
                if size >= WINDOW_BYTES or number - start + 1 >= WINDOW_LINES or number == len(lines):
                    text = "\n".join(lines[start - 1:number])
                    if text.strip():
                        windows.append({"path": path, "lines": [start, number], "text": text})
                    start, size = number + 1, 0
        return windows


def normalized(text: str) -> str:
    return hashlib.sha256(re.sub(r"\s+", " ", text).strip().encode("utf-8")).hexdigest()


class Builder:
    def __init__(self, tokenizer, width: int, instruction_id: str):
        self.tokenizer, self.width = tokenizer, width
        self.instruction_id = instruction_id
        self.instruction = ft.INSTRUCTIONS[instruction_id]
        self.refusals = collections.Counter()
        self.lengths = collections.Counter()

    def row(self, *, row_id, query, hit_text, label, meta, instruction_id=None, window=True):
        instruction_id = instruction_id or self.instruction_id
        document = ft.focus_document(hit_text, query, self.width) if window else hit_text
        if not document.strip():
            self.refusals["empty_document"] += 1
            return None
        ids = ft.compile_live_ids(self.tokenizer, ft.INSTRUCTIONS[instruction_id], query, document, self.width)
        if ids is None:
            self.refusals["over_width_or_over_bytes"] += 1
            return None
        self.lengths[min(len(ids) // 128, 8) * 128] += 1
        return dict(meta, id=row_id, label=label, instruction=instruction_id, query=query, document=document,
                    query_sha256=ft.sha256_text(query), document_sha256=ft.sha256_text(document),
                    live_ids=ids, live_tokens=len(ids))


def path_heldout(repo: str, path: str) -> bool:
    return int(h("pathheld", repo, path)[:8], 16) % 100 < PATH_HELDOUT_PERCENT


def build_pair_rows(builder, repo, pair, windows, bm25, by_class, held_paths, purpose):
    """Positives (<=2) and up to three same-repository hard negatives for one pair."""
    query = cap_query(pair["instruction"])
    evidence = pair["evidence"]
    evidence_paths = {e["path"] for e in evidence}
    taken = []
    rows = []
    meta = {"pair_id": pair["id"], "repo": repo["name"], "kind": pair["kind"], "set": purpose}
    if purpose == "heldout_path":
        candidates = [e for e in evidence if e["path"] in held_paths]
    else:
        candidates = list(evidence)
    candidates.sort(key=lambda e: (-(e["lines"][1] - e["lines"][0]), e["path"], e["lines"][0]))
    positives = []
    for e in candidates:
        if len(positives) >= POSITIVES_PER_PAIR:
            break
        lines = repo["repo"].lines(pair["commit"], e["path"])
        if not lines or e["lines"][0] > len(lines):
            builder.refusals["evidence_unreadable"] += 1
            continue
        text, a, b = ft.window_around(lines, e["lines"][0], e["lines"][1], HIT_TARGET, HIT_CAP)
        row = builder.row(row_id=f"{pair['id']}:pos{len(positives)}", query=query, hit_text=text, label=1,
                          meta=dict(meta, role="positive", path=e["path"], lines=[a, b]))
        if row:
            positives.append(row)
            taken.append(normalized(row["document"]))
    if not positives:
        builder.refusals["pair_without_positive"] += 1
        return []
    rows.extend(positives)
    excluded = evidence_paths | (held_paths if purpose == "train" else set())

    def add_negative(role, path, text, a, b):
        row = builder.row(row_id=f"{pair['id']}:{role}", query=query, hit_text=text, label=0,
                          meta=dict(meta, role=role, path=path, lines=[a, b]))
        if row and normalized(row["document"]) not in taken:
            taken.append(normalized(row["document"]))
            rows.append(row)

    # 1. Same file, another region, at the pair's commit.
    first = positives[0]
    lines = repo["repo"].lines(pair["commit"], first["path"])
    spans = [e["lines"] for e in evidence if e["path"] == first["path"]]
    if lines and len(lines) > 40:
        rng = random.Random(h("samefile", pair["id"]))
        for _ in range(12):
            centre = rng.randint(1, len(lines))
            text, a, b = ft.window_around(lines, centre, centre, HIT_TARGET, HIT_CAP)
            if all(b < s[0] - 3 or a > s[1] + 3 for s in spans) and (a > first["lines"][1] or b < first["lines"][0]):
                add_negative("neg_samefile", first["path"], text, a, b)
                break
    # 2. Lexical near-miss over the repository snapshot.
    allowed = lambda index: windows[index]["path"] not in excluded
    for index in bm25.top(query, allowed, 8):
        w = windows[index]
        before = len(rows)
        add_negative("neg_bm25", w["path"], w["text"], *w["lines"])
        if len(rows) > before:
            break
    # 3. Random window from another file of the same class.
    pool = by_class[ext_class(first["path"])]
    rng = random.Random(h("random", pair["id"]))
    for _ in range(20):
        w = windows[pool[rng.randrange(len(pool))]]
        if w["path"] not in excluded:
            add_negative("neg_random", w["path"], w["text"], *w["lines"])
            break
    return rows


def ordered(values, key):
    return sorted(values, key=lambda v: h("order", key(v)))


def sample_train(pairs, per_kind):
    """All commit pairs, then hash-ordered doc/test pairs; at most PER_PATH_CAP per first path."""
    chosen, per_path = [], collections.Counter()
    for kind in ("commit", "doc", "test"):
        limit = per_kind.get(kind)
        taken = 0
        for pair in ordered([p for p in pairs if p["kind"] == kind], lambda p: p["id"]):
            if limit is not None and taken >= limit:
                break
            first = pair["evidence"][0]["path"]
            if per_path[first] >= PER_PATH_CAP:
                continue
            per_path[first] += 1
            chosen.append(pair)
            taken += 1
    return chosen


def sample_eval(pairs, total):
    by_kind = {k: ordered([p for p in pairs if p["kind"] == k], lambda p: p["id"]) for k in ("commit", "doc", "test")}
    share = {k: max(1, round(total * len(v) / max(1, len(pairs)))) for k, v in by_kind.items() if v}
    # Keep commits and docs visible even though test pairs dominate the mined counts.
    for k in ("commit", "doc"):
        if k in share:
            share[k] = max(share[k], min(len(by_kind[k]), total // 4))
    share["test"] = max(0, total - sum(v for k, v in share.items() if k != "test"))
    return [p for k in ("commit", "doc", "test") for p in by_kind[k][: share.get(k, 0)]]


def replay_rows(builder, execution_root: Path, pins, *, admitted=None):
    """Pilot replay: 400 judged code TRAIN rows + 45 QASPER + 45 TechQA (training), 85 DEV (eval)."""
    if admitted is None:
        admitted = {}
        for name, relative in admission.REPLAY_FILES.items():
            path = execution_root / relative
            pins[str(path)] = file_sha(path)
            admitted[name] = path.read_bytes()
    census = json.loads(admitted["census"])["census"]["rows"]
    roster = {r["id"]: r for r in json.loads(admitted["roster"])}
    train, evaluation = [], []
    for record in census:
        pair = record["pair"]
        if ft.sha256_text(pair["question"]) != pair["question_sha256"] or ft.sha256_text(pair["document"]) != pair["document_sha256"]:
            raise ValueError("replay census bytes differ from their pins")
        if pair["rights_scope"] != "PERMITTED_PRIVATE_RESEARCH_SCOPE":
            continue
        role = pair["role"]
        if role == "TRAIN" and roster.get(pair["id"], {}).get("eligibility") == "NATIVE_FIT_SUPPORTED_PAIR":
            target, instruction, purpose = train, "software_evidence_v1", "replay_train"
        elif role == "DEV":
            target, instruction, purpose = evaluation, "software_evidence_v1", "replay_dev"
        elif role == "REPLAY":
            target, instruction, purpose = train, "publisher", "replay_train"
        else:
            continue
        row = builder.row(row_id="replay:" + pair["id"], query=pair["question"], hit_text=pair["document"],
                          label=int(pair["label"]), instruction_id=instruction, window=False,
                          meta={"pair_id": "replay:" + pair["id"], "repo": "replay", "kind": "qasper" if role == "REPLAY" else "code",
                                "set": purpose, "role": "replay"})
        if row:
            target.append(row)
    for draw in json.loads(admitted["techqa"]):
        source = Path(draw["source_pin"]["path"])
        raw = source.read_bytes()
        if hashlib.sha256(raw).hexdigest() != draw["source_pin"]["sha256"]:
            raise ValueError("TechQA source differs from its pin")
        span = draw["span"]
        window = raw[span["start"]:span["end"]].decode("utf-8")
        if ft.sha256_text(window) != draw["window_sha256"]:
            window = raw.decode("utf-8")[span["start"]:span["end"]]
        if ft.sha256_text(window) != draw["window_sha256"] or ft.sha256_text(draw["question"]) != draw["question_sha256"]:
            raise ValueError("TechQA window/question differs from its pin")
        row = builder.row(row_id="replay:techqa:" + draw["id"], query=draw["question"], hit_text=window, label=1,
                          instruction_id="publisher", window=False,
                          meta={"pair_id": "replay:techqa:" + draw["id"], "repo": "replay", "kind": "techqa",
                                "set": "replay_train", "role": "replay"})
        if row:
            train.append(row)
    return train, evaluation


def interleave(main, replay):
    if not replay:
        return main
    step = max(1, len(main) // len(replay))
    out, r = [], 0
    for i, row in enumerate(main, 1):
        out.append(row)
        if i % step == 0 and r < len(replay):
            out.append(replay[r])
            r += 1
    out.extend(replay[r:])
    return out


class HfTokenizer:
    def __init__(self, path: Path):
        from tokenizers import Tokenizer
        self.backend = Tokenizer.from_file(str(path))
        self.backend.no_padding()
        self.backend.no_truncation()
        for text, expected in (("no", ft.NO_ID), ("yes", ft.YES_ID)):
            if self.encode(text) != [expected]:
                raise ValueError("Focus readout token identity differs")

    def encode(self, text: str):
        return self.backend.encode(text, add_special_tokens=False).ids


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--pairs", type=Path, required=True)
    p.add_argument("--repo", action="append", required=True, help="name=checkout path")
    p.add_argument("--tokenizer", type=Path, required=True)
    p.add_argument("--execution-root", type=Path, required=True, help="campaign root holding the pilot replay files")
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--width", type=int, default=1024)
    p.add_argument("--instruction", default="software_evidence_v1", choices=sorted(ft.INSTRUCTIONS))
    p.add_argument("--commits", type=int, default=None, help="commit pairs per repository (default all)")
    p.add_argument("--docs", type=int, default=500)
    p.add_argument("--tests", type=int, default=500)
    p.add_argument("--eval-pairs", type=int, default=150, help="pairs per repository per evaluation set")
    p.add_argument("--protected-paths", type=Path, help="enable the pinned two-repository L7 corpus")
    p.add_argument("--protected-paths-sha256")
    p.add_argument("--pairs-stats-sha256")
    p.add_argument("--replay-sha256", action="append", default=[], help="pinned census=SHA, roster=SHA, techqa=SHA")
    args = p.parse_args(argv)
    strict = any((args.protected_paths, args.protected_paths_sha256, args.pairs_stats_sha256, args.replay_sha256))
    if strict and not all((args.protected_paths, args.protected_paths_sha256, args.pairs_stats_sha256, args.replay_sha256)):
        p.error("L7 requires the protected paths, its SHA-256, the pair stats SHA-256, and all replay pins")
    out = args.output.resolve()
    if out.exists() or "artifacts" not in out.parts:
        raise SystemExit("build_examples: output must be a fresh folder under artifacts")
    miner, miner_path = load_miner()
    stats_path = args.pairs / "stats.json"
    pins = {str(Path(__file__).resolve()): file_sha(Path(__file__)), str(HERE / "focus_text.py"): file_sha(HERE / "focus_text.py"),
            str(miner_path): file_sha(miner_path), str(args.tokenizer.resolve()): file_sha(args.tokenizer)}
    protected, local_pairs, replay = {}, {}, None
    if strict:
        helper = HERE / "corpus_admission.py"
        pins[str(helper)] = file_sha(helper)
        repos, mined, protected, local_pairs = admission.admit_local(
            args.pairs, args.repo, args.pairs_stats_sha256, args.protected_paths, args.protected_paths_sha256, pins)
        replay = admission.admit_replay(args.execution_root.resolve(), args.replay_sha256, pins)
        repo_specs = list(repos.items())
    else:
        pins[str(stats_path.resolve())] = file_sha(stats_path)
        mined = json.loads(stats_path.read_bytes())
        repo_specs = [(name, location) for name, _, location in (spec.partition("=") for spec in args.repo)]
    tokenizer = HfTokenizer(args.tokenizer)
    builder = Builder(tokenizer, args.width, args.instruction)
    train_rows, eval_rows, counts = [], [], collections.Counter()
    for name, location in repo_specs:
        if strict:
            pairs = local_pairs[name]
        else:
            jsonl = args.pairs / f"{name}.jsonl"
            pins[str(jsonl.resolve())] = file_sha(jsonl)
            pairs = [json.loads(line) for line in jsonl.read_text(encoding="utf-8").splitlines() if line.strip()]
        snapshot = mined["mine"]["repos"][name]["snapshot_commit"]
        repo = {"name": name, "repo": Repository(miner, name, Path(location), snapshot, protected=protected.get(name))}
        all_paths = {e["path"] for pair in pairs for e in pair["evidence"]}
        held = {path for path in all_paths if path_heldout(name, path)}
        touches = lambda pair: any(e["path"] in held for e in pair["evidence"])
        train_pool = [x for x in pairs if x["split"] == "train" and not touches(x)]
        id_pool = [x for x in pairs if x["split"] == "heldout" and not touches(x)]
        path_pool = [x for x in pairs if touches(x)]
        counts.update({f"{name}:paths": len(all_paths), f"{name}:held_paths": len(held),
                       f"{name}:train_pool": len(train_pool), f"{name}:heldout_id_pool": len(id_pool),
                       f"{name}:heldout_path_pool": len(path_pool),
                       f"{name}:train_dropped_for_path": sum(1 for x in pairs if x["split"] == "train" and touches(x))})
        windows = repo["repo"].snapshot_windows()
        bm25 = Bm25([w["path"] + "\n" + w["text"] for w in windows])
        by_class = collections.defaultdict(list)
        for index, w in enumerate(windows):
            by_class[ext_class(w["path"])].append(index)
        counts[f"{name}:snapshot_windows"] = len(windows)
        chosen = sample_train(train_pool, {"commit": args.commits, "doc": args.docs, "test": args.tests})
        id_eval = sample_eval(id_pool, args.eval_pairs)
        path_eval = sample_eval(path_pool, args.eval_pairs)
        id_kinds = collections.Counter(x["kind"] for x in id_eval)
        chosen_ids = {x["id"] for x in chosen}
        train_eval = [x for k in ("commit", "doc", "test")
                      for x in ordered([c for c in chosen if c["kind"] == k], lambda c: "trainsample" + c["id"])[: id_kinds[k]]]
        for purpose, group, target in (("train", chosen, train_rows), ("heldout_id", id_eval, eval_rows),
                                       ("heldout_path", path_eval, eval_rows), ("train_sample", train_eval, eval_rows)):
            for pair in group:
                rows = build_pair_rows(builder, repo, pair, windows, bm25, by_class, held, purpose)
                target.extend(rows)
                counts[f"{name}:{purpose}:pairs"] += 1 if rows else 0
                counts[f"{name}:{purpose}:{pair['kind']}:pairs"] += 1 if rows else 0
                for row in rows:
                    counts[f"{name}:{purpose}:{'pos' if row['label'] else 'neg'}"] += 1
        assert all(x["id"] in chosen_ids for x in train_eval)
        repo["repo"].git.close()
    if strict and any(counts[f"{name}:train:pos"] == 0 for name in admission.REPOSITORIES):
        raise ValueError("L7 needs admitted training positives from both repositories")
    replay_train, replay_eval = replay_rows(builder, args.execution_root.resolve(), pins, admitted=replay)
    counts["replay_train_rows"] = len(replay_train)
    counts["replay_dev_rows"] = len(replay_eval)
    eval_rows.extend(replay_eval)
    train_rows = interleave(ordered(train_rows, lambda r: r["id"]), ordered(replay_train, lambda r: r["id"]))
    for row in train_rows:
        row["set"] = row["set"] if row["set"].startswith("replay") else "train"
    out.mkdir(parents=False)
    for name, rows in (("train.jsonl", train_rows), ("eval.jsonl", eval_rows)):
        with (out / name).open("w", encoding="utf-8", newline="\n") as f:
            for row in rows:
                f.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")
    stats = {"schema": SCHEMA, "width": args.width, "instruction": args.instruction,
             "instruction_text": ft.INSTRUCTIONS[args.instruction], "query_chars": QUERY_CHARS,
             "hit_target_bytes": HIT_TARGET, "path_heldout_percent": PATH_HELDOUT_PERCENT,
             "train_rows": len(train_rows), "eval_rows": len(eval_rows),
             "train_labels": dict(collections.Counter(r["label"] for r in train_rows)),
             "counts": dict(sorted(counts.items())), "refusals": dict(builder.refusals),
             "live_token_histogram_128": dict(sorted(builder.lengths.items()))}
    (out / "stats.json").write_text(json.dumps(stats, indent=2) + "\n", encoding="utf-8", newline="\n")
    manifest = {"schema": SCHEMA + "/manifest", "inputs": pins,
                "outputs": {n: file_sha(out / n) for n in ("train.jsonl", "eval.jsonl", "stats.json")}}
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps(stats, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
