# SPDX-License-Identifier: Apache-2.0
"""Prototype of the single-stage Arbiter loop, outside the AIRK runtime.

Evaluation tooling for a development diagnostic, never product and never a
judged pass. It answers one question before the loop is built in the kernel:
with one judge model doing every model job, do the fixture's own read tools
reach the passages a task needs?

For each chosen case it takes the task and the ``needs`` and ``anchors`` a
calling agent wrote, and runs, against a ready fixture lifetime:

1. **Fetch.** One hybrid search per need, one per anchor, and one for the
   whole task. Anchor and task hits are offered to every need.
2. **Judge.** The model scores every (need, candidate) pair.
3. **Follow up**, for ``--rounds`` rounds, on each need's best passages:
   read the lines around them and judge the windows; and let the model
   choose which names found in them are worth a lookup, then search those
   names and judge the hits.
4. **Deliver.** Each need keeps its best-scored passages above ``--keep``,
   inside one byte budget shared in need order.

The judge is Qwen3-Reranker-4B in FP16 through PyTorch, the same prompt as
``arbiter_pair_probe.py``. The scenario's notes are never read here; scoring
the delivered passages against them is ``gather_context_loss_stage.py``'s
job, on the ``<case>.reply.json`` files this script writes.

    python -B arbiter_loop_prototype.py --handle <lifetime>/handle.json --scenarios <dir> \
        --needs <caller-needs.json> --model <checkpoint dir> --output <new dir> --cases munarium-C3 ...

``--arm task`` drops the caller inputs and uses the task as its only need.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import pathlib
import re
import sys
import time

import common
import evidence
import tool_names
from arbiter_pair_probe import PREFIX, SUFFIX, apply_block_quantization, pair_text
from arbiter_probe_cases import ACTION, PASSAGE

SCHEMA = "airk.private.arbiter-loop-prototype/1"
NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]{3,}")
WINDOW_LINES, WINDOW_STEP = 40, 25
MUST_READ = ("Given one need of a software development task, judge whether the Document is one of the few source "
             "passages a developer must read to meet that need, not merely a passage on the same topic.")
BUSY_RETRIES = 30
PLAN_STEPS = 6
FACT_NAME_LIMIT, FACT_PAGES = 6, 3
"""Steps one Parse plan may hold."""
GUIDANCE = ("CLAUDE.md", "AGENTS.md", "CONTRIBUTING.md", "README.md")
"""Files a project keeps its own working rules in; read at the root and beside the best passages."""
PLAIN_WORDS = frozenset("""this that with from have will would should could there their which where when what into
    than then them they were been being also only each every other some such more most must does none true false
    self return async await const static public private function class struct impl enum type string number""".split())


def names_in(text: str, limit: int) -> list[str]:
    """Identifier-like words of a passage: an underscore, an inner capital, or all capitals."""
    counts: dict[str, int] = {}
    for word in NAME.findall(text):
        shaped = "_" in word.strip("_") or word.isupper() or any(c.isupper() for c in word[1:])
        if shaped and len(word) >= 5 and word.lower() not in PLAIN_WORDS:
            counts[word] = counts.get(word, 0) + 1
    return [word for word, _ in sorted(counts.items(), key=lambda item: (-item[1], item[0]))[:limit]]


STANDING = {
    "rules": "Project rules that say what a change of this kind must include before it is done: required "
             "tests, test commands to run, logs or documents to update, review.",
    "run tests": "The script or command that runs this project's tests, its options, and which tests each "
                 "option includes.",
    "build file": "The build or project file that configures which tests run and what they need.",
    "error path": "Where an exception thrown by this code is caught and turned into the message the user sees.",
}
"""Needs every piece of work has and a caller seldom states. The kernel would choose them by deliverable."""
STANDING_BY_DELIVERABLE = {"code change": ("rules", "run tests", "build file", "error path"),
                           "code_change": ("rules", "run tests", "build file", "error path"),
                           "test": ("rules", "run tests", "build file")}
STANDING_DEFAULT = ("rules", "run tests", "build file")
CITED_PATH = re.compile(r"(?<![\w/.-])((?:[\w.-]+/)+[\w.-]+\.[A-Za-z0-9]{1,8})(?![\w/-])")
WORD = re.compile(r"[A-Za-z][a-z0-9]+|[A-Z]+(?![a-z])|\d+")


def cited_paths(texts: list[str], limit: int) -> list[str]:
    """Repository paths that evidence itself names, most cited first. An agent follows these."""
    counts: dict[str, int] = {}
    for text in texts:
        for path in CITED_PATH.findall(text):
            counts[path] = counts.get(path, 0) + 1
    return [path for path, _ in sorted(counts.items(), key=lambda item: (-item[1], item[0]))[:limit]]


def words(text: str) -> set[str]:
    return {word.lower() for word in WORD.findall(text) if len(word) >= 3}


def likeliest(need: str, candidates: dict, limit: int) -> list:
    """The candidates sharing the rarest words with a need: a cheap first cut before the model judges.
    The product would use the index's stored vectors here."""
    bags = {key: words(item["text"]) for key, item in candidates.items()}
    seen: dict[str, int] = {}
    for bag in bags.values():
        for word in bag:
            seen[word] = seen.get(word, 0) + 1
    wanted = words(need)
    weight = {word: 1.0 / (1 + seen.get(word, 0)) ** 0.5 for word in wanted}
    scored = sorted(((sum(weight[word] for word in wanted & bag), key) for key, bag in bags.items()),
                    key=lambda row: (-row[0], row[1]))
    return [key for value, key in scored[:limit] if value > 0]


QUOTED = re.compile(r"`([^`\n]{2,60})`")
CLAUSE = re.compile(r"\s*(?:[,;:()]|\band\b|\bor\b)\s*")


def clauses(need: str, limit: int = 4) -> list[str]:
    """The separate things one need asks about, as search queries of their own."""
    parts = [part.strip() for part in CLAUSE.split(need)]
    return [part for part in dict.fromkeys(parts) if len(part) >= 12 and part != need.strip()][:limit]


def windows(path: str, start: int, text: str) -> list[dict]:
    """Overlapping line windows of one read, each a candidate with its own lines."""
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    found = []
    for offset in range(0, max(len(lines) - WINDOW_LINES + WINDOW_STEP, 1), WINDOW_STEP):
        part = lines[offset:offset + WINDOW_LINES]
        if part:
            found.append({"path": path, "start": start + offset, "end": start + offset + len(part) - 1,
                          "text": "\n".join(part)})
    return found


def merge(passages: list[dict]) -> list[dict]:
    """Passages of one file whose lines touch or overlap become one, keeping the best score."""
    merged: list[dict] = []
    for passage in sorted(passages, key=lambda item: (item["path"], item["start"], item["end"])):
        last = merged[-1] if merged else None
        if last and last["path"] == passage["path"] and passage["start"] <= last["end"] + 1:
            if passage["end"] > last["end"]:
                extra = passage["text"].split("\n")[last["end"] - passage["start"] + 1:]
                last["text"] = "\n".join([last["text"], *extra]) if extra else last["text"]
                last["end"] = passage["end"]
            last["score"] = max(last["score"], passage["score"])
            last["needs"] = sorted(set(last["needs"]) | set(passage["needs"]))
        else:
            merged.append({**passage, "needs": list(passage["needs"])})
    return merged


def deliver(per_need: list[list[dict]], keep: float, per_need_limit: int, budget: int, floor: int = 0,
            relative: float = 0.0, shares: bool = False, weak: list[bool] | None = None) -> list[dict]:
    """Each need's best passages above ``keep``, taken in rounds across needs inside one byte budget. A
    need's first ``floor`` passages are delivered whatever they score, so no need comes back empty.
    With ``shares`` each need first spends its own part of the budget: a quarter of the budget is split
    evenly, the rest by how many passages each need kept. What a need leaves unspent goes to the others in
    a second sweep. A need marked ``weak`` is where the judge is least sure, so it keeps passages down to
    a third of the usual relative cutoff, at least three of them, and its share of the budget counts double."""
    chosen, used, seen = [], 0, set()
    weak = weak or [False] * len(per_need)
    kept_all = []
    for ranked, unsure in zip(per_need, weak):
        if relative:
            # Dynamic: a passage stays while it scores at least ``relative`` of this need's best.
            best = ranked[0]["score"] if ranked else 0.0
            cutoff = relative / 3 if unsure else relative
            kept = [item for item in ranked if item["score"] >= cutoff * best and item["score"] > 0.0]
            kept = kept if len(kept) >= (3 if unsure else 1) else ranked[:3 if unsure else 1]
            kept_all.append(kept[:per_need_limit])
        else:
            kept_all.append((ranked[:floor] + [item for item in ranked[floor:] if item["score"] >= keep])
                            [:per_need_limit])
    weight = [len(kept) * (2 if unsure else 1) for kept, unsure in zip(kept_all, weak)]
    total = sum(weight) or 1
    allowance = [budget // (4 * len(per_need)) + (budget * 3 // 4) * part // total for part in weight]
    spent = [0] * len(per_need)
    sweeps = (True, False) if shares else (False,)
    for limited, rank, index in ((limited, rank, index) for limited in sweeps for rank in range(per_need_limit)
                                 for index in range(len(per_need))):
        if True:
            kept = kept_all[index]
            if rank >= len(kept):
                continue
            item = kept[rank]
            key = (item["path"], item["start"], item["end"])
            size = len(item["text"].encode())
            if key in seen:
                row = next(row for row in chosen if (row["path"], row["start"], row["end"]) == key)
                if index not in row["needs"]:
                    row["needs"].append(index)
                continue
            if limited and spent[index] and spent[index] + size > allowance[index]:
                continue
            lines = set(range(item["start"], item["end"] + 1))
            held = {line for row in chosen if row["path"] == item["path"]
                    for line in range(row["start"], row["end"] + 1)}
            if len(lines & held) * 5 >= len(lines) * 4:
                continue  # at least four fifths of it is already delivered
            if used + size > budget:
                continue
            seen.add(key)
            used += size
            spent[index] += size
            chosen.append({"path": item["path"], "start": item["start"], "end": item["end"], "text": item["text"],
                           "score": item["score"], "needs": [index]})
    return merge(chosen)


class Judge:
    """Qwen3-Reranker yes/no pair scoring, FP16 on CUDA. With ``bits`` the linear weights are first rounded
    to that many bits in blocks and expanded back to FP16, which stands in for 8-bit stored weights
    computed in FP16 (user decision, 26 November 2026)."""

    def __init__(self, model_dir: pathlib.Path, max_tokens: int, batch: int, bits: int = 0):
        import torch
        from transformers import AutoModelForCausalLM, AutoTokenizer
        if not torch.cuda.is_available():
            raise RuntimeError("CUDA is not available; the prototype does not fall back to CPU")
        self.torch = torch
        self.tokenizer = AutoTokenizer.from_pretrained(model_dir, padding_side="left", local_files_only=True)
        self.model = AutoModelForCausalLM.from_pretrained(
            model_dir, local_files_only=True, device_map={"": 0}, torch_dtype=torch.float16).eval()
        self.weights = apply_block_quantization(self.model, bits, 32, False) if bits else None
        self.yes = self.tokenizer.convert_tokens_to_ids("yes")
        self.no = self.tokenizer.convert_tokens_to_ids("no")
        self.prefix = self.tokenizer.encode(PREFIX, add_special_tokens=False)
        self.suffix = self.tokenizer.encode(SUFFIX, add_special_tokens=False)
        self.room = max_tokens - len(self.prefix) - len(self.suffix)
        self.batch, self.pairs, self.seconds, self.clipped = batch, 0, 0.0, 0

    def score(self, instruction: str, pairs: list[tuple[str, str]]) -> list[float]:
        torch, started = self.torch, time.monotonic()
        encoded = []
        for query, document in pairs:
            ids = self.tokenizer.encode(pair_text(instruction, query, document), add_special_tokens=False)
            self.clipped += len(ids) > self.room
            encoded.append(self.prefix + ids[:self.room] + self.suffix)
        order = sorted(range(len(encoded)), key=lambda index: len(encoded[index]))
        scores = [0.0] * len(encoded)
        for offset in range(0, len(order), self.batch):
            chunk = order[offset:offset + self.batch]
            inputs = self.tokenizer.pad([{"input_ids": encoded[index]} for index in chunk], padding=True,
                                        return_tensors="pt").to(self.model.device)
            with torch.no_grad():
                logits = self.model(**inputs).logits[:, -1, :]
            both = torch.stack([logits[:, self.no], logits[:, self.yes]], dim=1).float()
            for index, value in zip(chunk, torch.nn.functional.softmax(both, dim=1)[:, 1].tolist()):
                scores[index] = round(float(value), 6)
        self.pairs += len(pairs)
        self.seconds += time.monotonic() - started
        return scores


class CheapJudge:
    """No model: a pair scores by the share of the need's words the passage holds, rarer words counting
    more. It answers one question: how far does the fetching alone reach, with nothing judging?"""

    def __init__(self):
        self.pairs, self.seconds, self.clipped = 0, 0.0, 0

    def score(self, instruction: str, pairs: list[tuple[str, str]]) -> list[float]:
        started = time.monotonic()
        bags = [words(document) for _, document in pairs]
        seen: dict[str, int] = {}
        for bag in bags:
            for word in bag:
                seen[word] = seen.get(word, 0) + 1
        values = []
        for (query, _), bag in zip(pairs, bags):
            wanted = words(query)
            weight = {word: 1.0 / (1 + seen.get(word, 0)) ** 0.5 for word in wanted}
            total = sum(weight.values()) or 1.0
            values.append(round(sum(weight[word] for word in wanted & bag) / total, 6))
        self.pairs += len(pairs)
        self.seconds += time.monotonic() - started
        return values


class Tools:
    """The fixture's read tools, with a count of calls."""

    def __init__(self, handle, repository: str, timeout: float, workers: int):
        self.handle, self.repository, self.timeout, self.calls, self.failed = handle, repository, timeout, 0, 0
        self.errors: list[dict] = []
        self.busy = 0
        self.plan_steps = 0
        self.lengths: dict[str, int] = {}
        self.pool = concurrent.futures.ThreadPoolExecutor(workers)

    def _call(self, tool: str, arguments: dict):
        """One tool call. The service answers "scoped_connection_unavailable" with a retry delay when its
        connections are busy; that is waited out here, up to ``BUSY_RETRIES`` times."""
        for attempt in range(BUSY_RETRIES + 1):
            # Native names select the same tool on pre-1.9 and 1.9 images (tool_names).
            outcome, _ = common.call_tool_retrying(self.handle, tool_names.native(tool), self.repository,
                                                   {**arguments, "response_mode": "full"}, self.timeout)
            if outcome.ok or "scoped_connection_unavailable" not in (common.envelope_text(outcome.envelope) or ""):
                break
            self.busy += 1
            time.sleep(min(0.2 * (attempt + 1), 2.0))
        self.calls += 1
        return outcome

    def _spans(self, tool: str, arguments: dict) -> list[dict]:
        outcome = self._call(tool, arguments)
        if not outcome.ok:
            self.failed += 1
            self.errors.append({"tool": tool, "arguments": arguments, "error": outcome.error,
                                "text": (common.envelope_text(outcome.envelope) or "")[:300]})
            return []
        return [{"path": span.path, "start": span.start, "end": span.end, "text": span.text}
                for span in evidence.extract(common.envelope_body(outcome.envelope))
                if span.start is not None and span.end is not None and span.text]

    def _body(self, tool: str, arguments: dict):
        outcome = self._call(tool, arguments)
        if not outcome.ok:
            self.failed += 1
            self.errors.append({"tool": tool, "arguments": arguments, "error": outcome.error,
                                "text": (common.envelope_text(outcome.envelope) or "")[:300]})
            return None
        return common.envelope_body(outcome.envelope)

    def search_many(self, queries: list[str], limit: int, lexical: bool = False,
                    code: bool | str = False) -> list[list[dict]]:
        mode = {"mode": "lexical"} if lexical else {}
        lane = {"evidence_kind": code if isinstance(code, str) else "code"} if code else {}
        return list(self.pool.map(lambda query: self._spans(
            "search", {"query": query, "limit": limit, "max_bytes": 65536, "full_chunks": True, **mode, **lane}),
            queries))

    @staticmethod
    def _fact_rows(body) -> list[dict]:
        rows = []
        for item in (body or {}).get("items", []):
            place, code = item.get("citation") or {}, item.get("code") or {}
            if isinstance(place.get("start_line"), int) and isinstance(place.get("end_line"), int):
                rows.append({"kind": item.get("kind"), "name": item.get("name") or "", "path": place["path"],
                             "start": place["start_line"], "end": place["end_line"],
                             "owner": (code.get("scope") or [""])[-1]})
        return rows

    def facts_many(self, asks: list[dict]) -> list[list[dict]]:
        """Code facts (the indexer's own syntax facts): definitions with their line spans, the calls each
        one makes and the types it inherits.

        Lookups by name are small, so they run as Parse plans, AIRK's own multi-step evidence runner: up
        to ``PLAN_STEPS`` reads in one call under one budget. A whole file's facts cannot: one Parse reply
        is capped at 65,536 bytes and a fact row is about 800 bytes even in its compact form (found on
        26 November 2026), so those are read directly, a page at a time."""
        named = [index for index, ask in enumerate(asks) if "path" not in ask]
        groups = [named[offset:offset + PLAN_STEPS] for offset in range(0, len(named), PLAN_STEPS)]
        found: list[list[dict]] = [[] for _ in asks]

        def planned(group):
            steps = [{"id": f"s{index}", "tool": "code_facts",
                      "args": {**asks[index], "compact": True, "limit": FACT_NAME_LIMIT, "max_bytes": 8192}}
                     for index in group]
            body = self._body("parse", {"schema": "airk.parse.request/1", "mode": "execute", "verification": "off",
                                        "plan": {"steps": steps, "output": {"view": "raw", "max_bytes": 65536}}})
            done = {step.get("id"): step.get("result") for step in (body or {}).get("steps", [])}
            self.plan_steps += len(group)
            return [(index, self._fact_rows(done.get(f"s{index}"))) for index in group]

        def direct(index):
            rows, cursor = [], None
            for _ in range(FACT_PAGES):
                body = self._body("code_facts", {**asks[index], "compact": True, "limit": 100, "max_bytes": 65536,
                                                 **({"cursor": cursor} if cursor else {})})
                rows += self._fact_rows(body)
                cursor = (body or {}).get("cursor")
                if not cursor:
                    break
            return index, rows

        for group in self.pool.map(planned, groups):
            for index, rows in group:
                found[index] = rows
        for index, rows in self.pool.map(direct, [index for index, ask in enumerate(asks) if "path" in ask]):
            found[index] = rows
        return found

    def references_many(self, asks: list[tuple[str, str]], limit: int) -> list[list[dict]]:
        """Where a name is written, source code first, outside one folder: who uses a type other than the
        files that implement it."""
        def users(ask):
            spans = self._spans("references", {"name": ask[0], "order": "code_first",
                                               "exclude_path_prefix": ask[1], "limit": 100, "max_bytes": 32768})
            first: dict[str, dict] = {}
            for span in spans:
                first.setdefault(span["path"], span)  # one use per file: a registry file can hold hundreds
            return list(first.values())[:limit]
        return list(self.pool.map(users, asks))

    def length(self, path: str) -> int:
        """Indexed lines of a file, 0 when it cannot be read by line."""
        if path not in self.lengths:
            body = self._body("file_info", {"path": path})
            self.lengths[path] = int((body or {}).get("indexed_lines") or 0)
        return self.lengths[path]

    def read_many(self, ranges: list[tuple[str, int, int]]) -> list[list[dict]]:
        """Bounded reads, each clamped to its file's own length."""
        def read(item):
            path, start, end = item
            last = self.length(path)
            if last == 0 or start > last:
                return []
            return self._spans("read_file", {"path": path, "start_line": max(start, 1), "end_line": min(end, last)})
        return list(self.pool.map(read, ranges))

    def around_many(self, spots: list[dict], margin: int) -> list[list[dict]]:
        return self.read_many([(spot["path"], spot["start"] - margin, spot["end"] + margin) for spot in spots])

    def list_many(self, prefixes: list[str]) -> list[list[str]]:
        def listing(prefix):
            body = self._body("list_files", {"prefix": prefix, "limit": 200} if prefix else {"limit": 200})
            return [path for path in (body or {}).get("paths", []) if isinstance(path, str)]
        return list(self.pool.map(listing, prefixes))


def add(pool: dict, items: list[dict]) -> list[tuple]:
    keys = []
    for item in items:
        key = (item["path"], item["start"], item["end"])
        pool.setdefault(key, item)
        keys.append(key)
    return keys


def run_case(tools: Tools, judge, task: str, needs: list[str], anchors: list[str], args, finisher=None) -> dict:
    pool: dict[tuple, dict] = {}
    scores: list[dict[tuple, float]] = [{} for _ in needs]
    trace = []

    def judge_new(offered: list[list[tuple]], ordered: bool = False) -> None:
        pairs, where = [], []
        for index, keys in enumerate(offered):
            fresh = [key for key in dict.fromkeys(keys) if key not in scores[index]]
            if args.first and len(fresh) > args.first:
                # Speed: the model judges only the few a cheap first cut likes best. The rest stay in the
                # pool, where a later step can still show them to this need.
                fresh = fresh[:args.first] if ordered else likeliest(
                    needs[index], {key: pool[key] for key in fresh}, args.first)
            for key in fresh:
                pairs.append((needs[index], pool[key]["text"]))
                where.append((index, key))
        for (index, key), value in zip(where, judge.score(MUST_READ if args.single else PASSAGE, pairs)):
            scores[index][key] = value

    strict: list[dict[tuple, float]] = [{} for _ in needs]

    def final(index: int, key: tuple) -> float:
        """Relevance alone, or relevance times the stricter second judgment when that is on. Relevance
        saturates (many candidates of a need score above 0.98), so alone its order is noise."""
        if not args.second_pass:
            return scores[index][key]
        return round(scores[index][key] * strict[index].get(key, 0.0), 6)

    def ranked(index: int) -> list[tuple]:
        return sorted(scores[index], key=lambda key: (-final(index, key), -scores[index][key], key))

    crossed = [0]

    def settle() -> None:
        """Give each need's most relevant candidates the stricter second judgment, once each. First, with
        ``--cross``, show every need the fetched passages it has not seen and is likeliest to want: a
        passage fetched for one need is often the answer to another (run r10: three required passages
        scored 0.84 to 1.00 for a need that never saw them)."""
        if args.cross:
            before = judge.pairs
            judge_new([likeliest(needs[index], {key: pool[key] for key in pool if key not in scores[index]},
                                 args.cross) for index in range(len(needs))])
            crossed[0] += judge.pairs - before
        if not args.second_pass:
            return
        where = []
        for index in range(len(needs)):
            by_relevance = sorted(scores[index], key=lambda key: (-scores[index][key], key))[:args.second_pass]
            where += [(index, key) for key in by_relevance if key not in strict[index]]
        values = judge.score(MUST_READ, [(needs[index], pool[key]["text"]) for index, key in where])
        for (index, key), value in zip(where, values):
            strict[index][key] = value

    def best(index: int) -> float:
        return max((final(index, key) for key in scores[index]), default=0.0)

    def open_tops(round_number: int) -> list[list[dict]]:
        """The best passages of every need still open. From round two a need whose best passage already
        scores ``--settled`` stops fetching, so later rounds go to the needs that lack evidence. Round one
        stays open for every need: one strong passage does not show a need is covered (run r8 stopped
        Munarium C1 before any read-around and lost three passages)."""
        return [[] if args.settled and round_number >= 2 and best(index) >= args.settled
                else [pool[key] for key in ranked(index)[:args.follow]] for index in range(len(needs))]

    def follow_structure(tops: list[list[dict]]) -> dict:
        """From each need's best passages, through the indexer's own syntax facts: the whole functions
        they sit in, the functions those call, and the places that call them. No name is guessed."""
        files = sorted({item["path"] for row in tops for item in row})
        asked = tools.facts_many([{"path": path, "kind": kind} for path in files
                                  for kind in ("definition", "call", "inherits")])
        definitions = {path: asked[3 * index] for index, path in enumerate(files)}
        calls = {path: asked[3 * index + 1] for index, path in enumerate(files)}
        bases = {path: [name for fact in asked[3 * index + 2]
                        for name in NAME.findall(fact["name"]) if len(name) >= 5][:2]
                 for index, path in enumerate(files)}
        ranges: list[list[tuple]] = []
        outside: list[list[str]] = []
        owners: list[list[str]] = []
        for row in tops:
            mine, names, held = [], [], []
            for item in row:
                inside = [fact for fact in definitions[item["path"]]
                          if fact["start"] <= item["end"] and fact["end"] >= item["start"]
                          and fact["end"] - fact["start"] < args.function_lines]
                mine += [(fact["path"], fact["start"], fact["end"]) for fact in inside]
                held += [fact["name"] for fact in inside]
                local = {fact["name"]: fact for fact in definitions[item["path"]]}
                for call in calls[item["path"]]:
                    if not any(fact["start"] <= call["start"] <= fact["end"] for fact in inside):
                        continue
                    name = re.split(r"[.:>?]+", call["name"].split("<")[0].split("(")[0])[-1].strip()
                    if name in local and local[name]["end"] - local[name]["start"] < args.function_lines:
                        mine.append((item["path"], local[name]["start"], local[name]["end"]))
                    elif len(name) >= 5 and name not in names:
                        names.append(name)
            ranges.append(list(dict.fromkeys(mine)))
            outside.append(names[:args.callees])
            owners.append(list(dict.fromkeys(name for name in held if len(name) >= 5))[:args.callers])
        callee_names = sorted({name for row in outside for name in row})
        elsewhere = dict(zip(callee_names, tools.facts_many(
            [{"name": name, "kind": "definition"} for name in callee_names])))
        owner_names = sorted({name for row in owners for name in row})
        callers = dict(zip(owner_names, tools.facts_many(
            [{"name": name, "kind": "call", "name_match": "suffix"} for name in owner_names])))
        for index in range(len(tops)):
            ranges[index] += [(fact["path"], fact["start"], fact["end"]) for name in outside[index]
                              for fact in elsewhere[name][:2] if fact["end"] - fact["start"] < args.function_lines]
            ranges[index] += [(fact["path"], fact["start"] - 12, fact["end"] + 12) for name in owners[index]
                              for fact in callers[name][:3]]
        # A type this code inherits is used somewhere else: the dispatcher, the registry, the caller that
        # holds the interface. Those users are fetched from outside the implementing folder.
        used = sorted({(name, item["path"].rpartition("/")[0] + "/") for row in tops for item in row
                       for name in bases[item["path"]]})
        users = dict(zip(used, tools.references_many(used, args.users)))
        for index, row in enumerate(tops):
            for item in row:
                for name in bases[item["path"]]:
                    ranges[index] += [(span["path"], span["start"] - 15, span["end"] + 15)
                                      for span in users[(name, item["path"].rpartition("/")[0] + "/")]]
        unique = sorted({item for row in ranges for item in row})
        read = dict(zip(unique, tools.read_many(unique)))
        offered = [[key for item in row for span in read[item] for key in add(pool, [span])] for row in ranges]
        judge_new(offered)
        return {"functions": sum(len(row) for row in ranges), "callees": outside, "callers_of": owners,
                "inherited": sorted({name for name, _ in used})}

    lanes = [tools.search_many([*needs, *anchors, task], args.search_limit)]
    if args.lanes:
        # A second lane of source code only, so documents cannot crowd code out of a need's candidates.
        lanes.append(tools.search_many([*needs, *anchors, task], args.search_limit, code=True))
        # A third lane of configuration only (build files, project files, workflows): what the standing
        # needs about tests and builds ask for is seldom prose or code.
        lanes.append(tools.search_many([*needs, *anchors, task], max(args.search_limit // 3, 4),
                                       code="configuration"))
    shared = [key for found in lanes for hits in found[len(needs):] for key in add(pool, hits)]
    # Each need's own hits come first, in the search's own order, lane by lane in turn.
    own = [[key for group in zip(*[add(pool, found[index]) for found in lanes]) for key in group] +
           [key for found in lanes for key in add(pool, found[index])] for index in range(len(needs))]
    judge_new([row + shared for row in own], ordered=True)
    settle()
    trace.append({"round": 0, "candidates": len(pool), "best": [round(best(index), 3) for index in range(len(needs))]})
    for round_number in range(1, args.rounds + 1):
        tops = open_tops(round_number)
        if not any(tops):
            break
        if round_number == 1:
            # Structure: the project's own guidance files, and the files beside each need's best passages.
            folders = [sorted({item["path"].rpartition("/")[0] for item in row}) for row in tops]
            every = sorted({folder for row in folders for folder in row})
            roots = sorted({""} | {folder.split("/")[0] for folder in every} | set(every))
            guides = [f"{root}/{name}" if root else name for root in roots for name in GUIDANCE]
            guide_keys = [key for spans in tools.read_many([(path, 1, args.head * 3) for path in guides])
                          for span in spans for key in add(pool, windows(span["path"], span["start"], span["text"]))]
            if args.cited:
                # Files the evidence itself names (a script in a rule, a project file in a command) are read
                # too. Their windows join the pool, and each need is shown the ones it is likeliest to want.
                named = cited_paths([pool[key]["text"] for key in guide_keys] +
                                    [item["text"] for row in tops for item in row], args.cited)
                for spans in tools.read_many([(path, 1, args.head * 2) for path in named]):
                    for span in spans:
                        add(pool, windows(span["path"], span["start"], span["text"]))
                trace.append({"round": "cited", "paths": named})
            listings = dict(zip(every, tools.list_many([folder + "/" if folder else "" for folder in every])))
            known = {key[0] for key in pool}
            beside = [[path for folder in row for path in listings[folder] if path not in known][:args.beside_offer]
                      for row in folders]
            values = iter(judge.score(ACTION, [(needs[index], f"Read the file `{path}`.")
                                               for index, paths in enumerate(beside) for path in paths]))
            picked = []
            for paths in beside:
                scored = sorted(((next(values), path) for path in paths), reverse=True)
                picked.append([path for value, path in scored[:args.beside] if value >= args.lookup_keep])
            wanted_files = sorted({path for row in picked for path in row})
            heads = dict(zip(wanted_files, tools.read_many([(path, 1, args.head) for path in wanted_files])))
            judge_new([guide_keys + [key for path in row for span in heads[path]
                                     for key in add(pool, windows(span["path"], span["start"], span["text"]))]
                       for row in picked])
            trace.append({"round": "structure", "candidates": len(pool), "guides": sorted({key[0] for key in guide_keys}),
                          "beside": picked})
            settle()
            tops = open_tops(round_number)
        spots = list({(item["path"], item["start"], item["end"]): item for row in tops for item in row}.values())
        reads = dict(zip([(spot["path"], spot["start"], spot["end"]) for spot in spots],
                         tools.around_many(spots, args.margin)))
        offered = []
        for row in tops:
            keys = []
            for item in row:
                for span in reads[(item["path"], item["start"], item["end"])]:
                    keys += add(pool, windows(span["path"], span["start"], span["text"]))
            offered.append(keys)
        judge_new(offered)
        followed = follow_structure(tops) if args.structure else None
        # The model chooses which names in a need's best passages are worth a lookup.
        candidates = [names_in("\n".join(item["text"] for item in row), args.names) for row in tops]
        flat = [(needs[index], f"Search the repository for the name `{name}`.")
                for index, names in enumerate(candidates) for name in names]
        values = iter(judge.score(ACTION, flat))
        lookups: list[list[str]] = []
        for names in candidates:
            scored = sorted(((next(values), name) for name in names), reverse=True)
            lookups.append([name for value, name in scored[:args.lookups] if value >= args.lookup_keep])
        wanted = sorted({name for row in lookups for name in row})
        hits = dict(zip(wanted, tools.search_many(wanted, args.lookup_limit, lexical=True)))
        judge_new([[key for name in row for key in add(pool, hits[name])] for row in lookups])
        settle()
        trace.append({"round": round_number, "candidates": len(pool), "reads": len(spots), "lookups": lookups,
                      "open": sum(bool(row) for row in tops),
                      "followed": followed, "cross_pairs": crossed[0],
                      "best": [round(best(index), 3) for index in range(len(needs))]})
    def weakness(index: int) -> str | None:
        """Why a need's evidence is weak, or None. Deliberately broad (user direction, 26 November 2026:
        make the larger change and dial it back): a low best score, thin evidence, or best evidence
        that the task itself does not want."""
        finals = sorted((final(index, key) for key in scores[index]), reverse=True)
        if not finals or finals[0] < args.weak:
            return "low best score"
        if sum(value >= args.weak / 2 for value in finals) < 2:
            return "one passage only"
        whole = len(needs) - 1
        if index != whole and scores[whole].get(ranked(index)[0], 1.0) < args.off_task:
            return "best passage is off the task"
        return None

    if finisher is not None:
        # Minimal use of the model: the cheap ranking chooses each need's few best passages, and the model
        # judges only those, once, to order them and cut the ones not worth delivering.
        heads = [ranked(index)[:args.final_judge] for index in range(len(needs))]
        values = iter(finisher.score(MUST_READ, [(needs[index], pool[key]["text"])
                                               for index, keys in enumerate(heads) for key in keys]))
        for index, keys in enumerate(heads):
            judged = {key: next(values) for key in keys}
            scores[index] = {key: judged.get(key, 0.0) for key in scores[index]}
    weak_before = [weakness(index) for index in range(len(needs))]
    if args.rescue and any(weak_before):
        # A weak need gets a second, different fetch: itself joined to the task, its clauses one by one,
        # and exact lookups of the names it spells out; then a read around its new best passages.
        rows = [index for index, reason in enumerate(weak_before) if reason]
        queries = [[f"{needs[index]} {task[:400]}", *clauses(needs[index])] for index in rows]
        exact = [names_in(needs[index], 6) + QUOTED.findall(needs[index])[:4] for index in rows]
        flat = sorted({query for row in queries for query in row})
        wide = dict(zip(flat, tools.search_many(flat, args.search_limit * 2)))
        names = sorted({name for row in exact for name in row})
        narrow = dict(zip(names, tools.search_many(names, args.lookup_limit, lexical=True)))
        offered: list[list[tuple]] = [[] for _ in needs]
        for index, asked, spelled in zip(rows, queries, exact):
            offered[index] = [key for query in asked for key in add(pool, wide[query])] + \
                             [key for name in spelled for key in add(pool, narrow[name])]
        judge_new(offered)
        settle()
        tops = [[pool[key] for key in ranked(index)[:args.follow]] if index in rows else []
                for index in range(len(needs))]
        spots = list({(item["path"], item["start"], item["end"]): item for row in tops for item in row}.values())
        reads = dict(zip([(spot["path"], spot["start"], spot["end"]) for spot in spots],
                         tools.around_many(spots, args.margin)))
        judge_new([[key for item in row for span in reads[(item["path"], item["start"], item["end"])]
                    for key in add(pool, windows(span["path"], span["start"], span["text"]))] for row in tops])
        settle()
        trace.append({"round": "rescue", "candidates": len(pool), "needs": rows, "queries": queries, "exact": exact,
                      "best": [round(best(index), 3) for index in range(len(needs))]})
    weak = [weakness(index) for index in range(len(needs))]
    per_need = [[{**pool[key], "score": final(index, key)} for key in ranked(index)] for index in range(len(needs))]
    delivered = deliver(per_need, args.keep, args.per_need, args.budget, args.floor, args.relative, args.shares,
                        [bool(reason) for reason in weak] if args.rescue else None)
    return {"needs": [{"need": need, "weak": weak[index], "weak_before_rescue": weak_before[index],
                       "best": [{"path": item["path"], "start": item["start"], "end": item["end"],
                                               "score": item["score"]} for item in per_need[index][:5]]}
                      for index, need in enumerate(needs)],
            "trace": trace, "delivered": delivered, "tool_errors": tools.errors,
            # Every judged candidate, without text, so a miss can be told apart: never fetched, or fetched
            # and scored low.
            "judged": [[[key[0], key[1], key[2], final(index, key)] for key in ranked(index)]
                       for index in range(len(needs))]}


def reply_of(delivered: list[dict]) -> dict:
    """The delivered passages in the record shape ``gather_context_loss_stage.py`` reads."""
    return {"records": [{"operation": "read_file", "arguments": {"path": item["path"]},
                         "canonical_result": {"hits": [{"citation": {
                             "path": item["path"], "start_line": item["start"], "end_line": item["end"],
                             "location": {"Lines": {"start": item["start"], "end": item["end"]}}},
                             "text": item["text"]}]}} for item in delivered]}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handle", required=True, type=pathlib.Path)
    parser.add_argument("--scenarios", required=True, type=pathlib.Path)
    parser.add_argument("--needs", required=True, type=pathlib.Path)
    parser.add_argument("--model", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--cases", required=True, nargs="+")
    parser.add_argument("--arm", choices=("needs", "task"), default="needs")
    parser.add_argument("--rounds", type=int, default=2)
    parser.add_argument("--search-limit", type=int, default=24)
    parser.add_argument("--follow", type=int, default=3, help="best passages per need to read around")
    parser.add_argument("--margin", type=int, default=60, help="lines read on each side of a passage")
    parser.add_argument("--names", type=int, default=24, help="names offered to the model per need")
    parser.add_argument("--lookups", type=int, default=3, help="names looked up per need and round")
    parser.add_argument("--lookup-keep", type=float, default=0.3)
    parser.add_argument("--lookup-limit", type=int, default=8)
    parser.add_argument("--beside", type=int, default=3, help="neighbouring files opened per need")
    parser.add_argument("--beside-offer", type=int, default=60, help="neighbouring files offered per need")
    parser.add_argument("--head", type=int, default=160, help="lines read from the top of a neighbouring file")
    parser.add_argument("--keep", type=float, default=0.3, help="lowest score a delivered passage may have")
    parser.add_argument("--per-need", type=int, default=12)
    parser.add_argument("--floor", type=int, default=0, help="best passages per need delivered whatever they score")
    parser.add_argument("--relative", type=float, default=0.0,
                        help="dynamic cutoff: keep a passage scoring at least this share of its need's best "
                             "(0 uses --keep and --floor)")
    parser.add_argument("--first", type=int, default=0,
                        help="most new passages one need has judged per step; a cheap first cut picks them "
                             "(0: judge every offered passage)")
    parser.add_argument("--cheap", action="store_true",
                        help="no model in the loop: word overlap with the need ranks every passage")
    parser.add_argument("--final-judge", type=int, default=0,
                        help="with --cheap: the model judges only each need's best N passages, once, at the end")
    parser.add_argument("--bits", type=int, default=0,
                        help="round the model's linear weights to this many bits, expanded to FP16 (0: FP16 as published)")
    parser.add_argument("--single", action="store_true",
                        help="one judgment per pair, the strict one, in place of relevance then strict")
    parser.add_argument("--cross", type=int, default=0,
                        help="unseen fetched passages shown to each need at every judging step (0 turns it off)")
    parser.add_argument("--standing", action="store_true",
                        help="add the standing needs chosen by the deliverable (rules, tests, build file, errors)")
    parser.add_argument("--lanes", action="store_true", help="search source code in a lane of its own")
    parser.add_argument("--cited", type=int, default=0, help="files named by the evidence itself to read (0: none)")
    parser.add_argument("--structure", action="store_true",
                        help="follow the indexer's syntax facts: whole functions, their callees and callers")
    parser.add_argument("--function-lines", type=int, default=160, help="longest function taken whole")
    parser.add_argument("--callees", type=int, default=6, help="called names looked up per need and round")
    parser.add_argument("--users", type=int, default=6, help="uses of an inherited type read, per type")
    parser.add_argument("--callers", type=int, default=2, help="functions whose callers are looked up, per need")
    parser.add_argument("--shares", action="store_true", help="give each need its own part of the byte budget")
    parser.add_argument("--settled", type=float, default=0.0,
                        help="a need whose best passage scores this much stops fetching (0 keeps every need open)")
    parser.add_argument("--weak", type=float, default=0.5, help="a need whose best score is below this is weak")
    parser.add_argument("--off-task", type=float, default=0.1,
                        help="a need whose best passage scores below this for the whole task is weak")
    parser.add_argument("--rescue", action="store_true",
                        help="give weak needs a second, different fetch and a larger share of the reply")
    parser.add_argument("--second-pass", type=int, default=0,
                        help="best candidates per need given the stricter second judgment (0 turns it off)")
    parser.add_argument("--budget", type=int, default=100_000, help="delivered passage bytes per case")
    parser.add_argument("--max-tokens", type=int, default=2048)
    parser.add_argument("--batch", type=int, default=16)
    parser.add_argument("--timeout", type=float, default=120.0)
    parser.add_argument("--workers", type=int, default=8)
    args = parser.parse_args(argv)
    written = json.loads(args.needs.read_text(encoding="utf-8"))["cases"]
    handle = common.load_handle(args.handle.resolve(strict=True))
    args.output.mkdir()
    if args.single or args.cheap:
        args.second_pass = 0
    model = None if args.cheap and not args.final_judge else Judge(args.model, args.max_tokens, args.batch, args.bits)
    judge = CheapJudge() if args.cheap else model
    finisher = model if args.cheap and args.final_judge else None
    rows = []
    for case_id in args.cases:
        repository, slot = case_id.split("-", 1)
        task = json.loads((args.scenarios / repository / f"{slot}.json").read_text(encoding="utf-8"))["instruction"]
        # The task itself is the last need, so evidence for the task as a whole (project rules, for one)
        # is judged too, after every stated need has been served.
        stated = list(written[case_id]["needs"]) if args.arm == "needs" else []
        chosen = STANDING_BY_DELIVERABLE.get(str(written[case_id].get("deliverable") or "").strip().lower(),
                                             STANDING_DEFAULT) if args.standing else ()
        needs = [*stated, *(STANDING[name] for name in chosen), task]
        anchors = list(written[case_id].get("anchors") or []) if args.arm == "needs" else []
        tools = Tools(handle, repository, args.timeout, args.workers)
        started = time.monotonic()
        counted = finisher or judge
        pairs, seconds = counted.pairs, counted.seconds
        result = run_case(tools, judge, task, needs, anchors, args, finisher)
        delivered = result["delivered"]
        row = {"case": case_id, "needs": len(needs), "stated": len(stated), "standing": list(chosen),
               "anchors": len(anchors), "tool_calls": tools.calls,
               "tool_failures": tools.failed, "busy_retries": tools.busy, "plan_steps": tools.plan_steps, "pairs": counted.pairs - pairs,
               "judge_seconds": round(counted.seconds - seconds, 1),
               "seconds": round(time.monotonic() - started, 1), "passages": len(delivered),
               "delivered_bytes": sum(len(item["text"].encode()) for item in delivered)}
        rows.append(row)
        for name, value in ((f"{case_id}.reply.json", reply_of(delivered)), (f"{case_id}.trace.json", result)):
            with (args.output / name).open("x", encoding="utf-8", newline="\n") as file:
                file.write(json.dumps(value, indent=1, sort_keys=True) + "\n")
        print(" ".join(f"{key}={value}" for key, value in row.items()), flush=True)
    run = {"schema": SCHEMA, "handle_sha256": handle.sha256, "stages": handle.live_stages(),
           "model": str(args.model), "precision": "none" if model is None else "float16", "weight_bits": args.bits or 16,
           "weights": None if model is None else model.weights, "clipped_inputs": (model or judge).clipped,
           "settings": {key: value for key, value in vars(args).items()
                        if isinstance(value, (int, float, str)) and key not in ("arm",)},
           "arm": args.arm, "rows": rows}
    with (args.output / "run.json").open("x", encoding="utf-8", newline="\n") as file:
        file.write(json.dumps(run, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
